use std::{mem, str::FromStr, sync::Arc, time::Duration, vec};
use axum::{ response::IntoResponse, Extension, Router};
use axum_login::{AuthManagerLayer, AuthManagerLayerBuilder};
use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use hyper::StatusCode;
use minijinja::{path_loader, Environment};
use minijinja_autoreload::AutoReloader;
use tokio::{net::TcpListener, sync::Mutex};
use tower_sessions::{cookie::Key, Expiry, SessionManagerLayer};
use tracing_subscriber::{layer::SubscriberExt, Registry};
use tower::builder::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer, 
    cors::CorsLayer, 
    timeout::TimeoutLayer,
    trace::TraceLayer};

use crate::{
    authentication::Vault, context::ContextLayer, db::ConnectionPool, feature::Feature, session::BlandworkSessionStore, template::TemplateLayer, Config, TemplateAccessor
};

#[derive(Clone)]
pub struct NoPool;

#[derive(Clone)]
pub struct NoFeatures;

pub type Features = Vec<Box<dyn Feature>>;

#[derive(Clone)]
pub struct Sessions {
    common: SessionManagerLayer<BlandworkSessionStore, tower_sessions::service::SignedCookie>,
    protected: AuthManagerLayer<Vault, BlandworkSessionStore, tower_sessions::service::SignedCookie>
}

pub struct App<P, F> {
    // application configuration
    config: Arc<Config>,

    // application router
    router: Router,

    // template reloader
    autoloader: TemplateAccessor,

    // sessions
    sessions: Option<Sessions>,

    // features should be decoupled from navigator/template/theme.
    // they can reference the current theme in their handlers.
    features: F,

    // optional and only matters for Extension() on router
    // Features could use it in their handlers, but we can't know that during build.
    pub pool: P,
}

impl App<NoPool, NoFeatures> {
    pub fn new(config: Config) -> App<NoPool, NoFeatures> {
        let template_path: String = config.server.template_path.clone();

        let autoloader: TemplateAccessor = TemplateAccessor(Arc::new(Mutex::new(AutoReloader::new(move |notifier| {
            let mut env: Environment = Environment::new();
            env.set_loader(path_loader(&template_path));

            notifier.set_fast_reload(true);
            notifier.watch_path(&template_path, true);
            Ok(env)
        }))));
        
        App {
            config: Arc::new(config),
            autoloader,
            sessions: None,
            router: Router::new(),
            pool: NoPool,
            features: NoFeatures,
        }
    }
}

impl App<NoPool, NoFeatures> {
    pub async fn connect(&mut self) -> App<ConnectionPool, NoFeatures> { 
        let tokio_config = tokio_postgres::config::Config::from_str(
            &self.config.database.connection_string()
        )
        .unwrap();
    
        let pg_mgr: PostgresConnectionManager<tokio_postgres::NoTls> = PostgresConnectionManager::new(tokio_config, tokio_postgres::NoTls);
        
        let pool: ConnectionPool = match Pool::builder()
            .max_size(10)
            // .min_idle(1)
            .build(pg_mgr).await {
                Ok(pool) => pool,
                Err(e) => panic!("App error: {e:?}"),
            };

        return App{
            config: self.config.clone(),
            router: self.router.clone(),
            pool,
            sessions: None,
            features: NoFeatures,
            autoloader: self.autoloader.clone(),
        };
    }

    pub fn register_feature_default<F: Feature + Default + 'static>(&self) ->  App<NoPool, Features>{         
        let features: Vec<Box<dyn Feature>> = vec![
            Box::new(F::default())
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            sessions: None,
            autoloader: self.autoloader.clone(),
            pool: NoPool,
            features,
        };
    }

    pub fn register_feature(&self, feature: impl Feature + 'static) ->  App<NoPool, Features>{         
        let features: Vec<Box<dyn Feature>> = vec![
            Box::new(feature)
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            sessions: None,
            pool: NoPool,
            autoloader: self.autoloader.clone(),
            features,
        };
    }
}

impl App<NoPool, Features> {
    pub fn register_feature_default<F: Feature + Default + 'static>(&mut self) ->  App<NoPool, Features>{
        self.features.push(Box::new(F::default()));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            sessions: None,
            pool: NoPool,
            autoloader: self.autoloader.clone(),
            features,
        };
    }

    pub fn register_feature(&mut self, feature: impl Feature + 'static) ->  App<NoPool, Features>{         
        self.features.push(Box::new(feature));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            sessions: None,
            pool: NoPool,
            autoloader: self.autoloader.clone(),
            features,
        };
    }

    pub fn apply_fallback(&mut self) -> App<NoPool, Features> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        async fn handler_404() -> impl IntoResponse {
            (StatusCode::NOT_FOUND, "nothing to see here")
        }

        router = router.fallback(handler_404);

        return App { 
            config: self.config.clone(),
            sessions: None,
            pool: NoPool,
            autoloader: self.autoloader.clone(),
            router,
            features
        };
    }

    pub fn apply_extension<S: Clone + Send + Sync + 'static>(&mut self, state: S) -> App<NoPool, Features> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        router = router.layer(Extension(state));

        return App {
            config: self.config.clone(),
            pool: NoPool,
            sessions: None,
            autoloader: self.autoloader.clone(),
            router,
            features,
        };
    }

    pub fn build(&mut self) -> App<NoPool, Features>{
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        let mut context_layer: ContextLayer = ContextLayer::new(self.config.clone());

        // 1. scan features and extract links for navigator 
        for feature in features.iter() {
            match feature.link() {
                Some(link) => {
                    context_layer.add_link(link);
                },
                _ => {}
            }
        }

        // 2. scan features and apply routers
        for feature in features.iter() {
            router = match feature.api() {
                Some(mut api) => {
                    api = api
                        .layer(context_layer.clone());

                    router.merge(api)
                }, 
                None => router
            };

            router = match feature.supplemental() {
                Some(mut supp) => {
                    supp = supp
                    .layer(TemplateLayer::new(None,
                    self.autoloader.clone()))
                    .layer(context_layer.clone());
                    
                    router.merge(supp)
                }, 
                None => router
            };

            router = match feature.web() {
                Some(mut web) => {
                    web = web
                        .layer(TemplateLayer::new(Some(self.config.server.shell_template.clone()),
                        self.autoloader.clone()))
                        .layer(context_layer.clone());
                       
                    router.merge(web)
                }, 
                None => router
            };
        }
    
        router = router
            // core layers
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    
                    // Vanilla middleware
                    .layer(CorsLayer::new())
                    .layer(CompressionLayer::new())
                    .layer(TimeoutLayer::new(Duration::from_secs(10)))
            )

            // base extensions (database connection)
            .layer(Extension(self.pool.clone()));

        App {
            config: self.config.clone(),
            pool: NoPool,
            sessions: None,
            autoloader: self.autoloader.clone(),
            features,
            router,
        }
    }

    pub async fn run(&mut self) {
        let listener: TcpListener = TcpListener::bind(format!("{host}:{port}", host=self.config.server.host, port=self.config.server.port))
            .await
            .unwrap();
        
        // tracing_subscriber::fmt::fmt().with_env_filter(EnvFilter::from_default_env()).init();
        let stdout = tracing_subscriber::fmt::layer().pretty();
        let subscriber = Registry::default().with(stdout);
    
        tracing::subscriber::set_global_default(subscriber)
            .expect("Unable to set global subscriber");
        
        axum::serve(listener, self.router.clone()).await.unwrap();
    }
}

impl App<ConnectionPool, NoFeatures> {
    pub fn register_feature_default<F: Feature + Default + 'static>(&self) ->  App<ConnectionPool, Features>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(F::default())
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            sessions: self.sessions.clone(),
            pool: self.pool.clone(),
            autoloader: self.autoloader.clone(),
            features,
        };
    }

    pub fn register_feature(&self, feature: impl Feature + 'static) ->  App<ConnectionPool, Features>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(feature)
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            sessions: self.sessions.clone(),
            pool: self.pool.clone(),
            autoloader: self.autoloader.clone(),
            features,
        };
    }
}

impl App<ConnectionPool, Features> {
    pub fn register_feature_default<F: Feature + Default + 'static>(&mut self) ->  App<ConnectionPool, Features>{
        self.features.push(Box::new(F::default()));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            sessions: self.sessions.clone(),
            pool: self.pool.clone(),
            autoloader: self.autoloader.clone(),
            features,
        };
    }

    pub fn register_feature(&mut self, feature: impl Feature + 'static) ->  App<ConnectionPool, Features>{         
        self.features.push(Box::new(feature));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            sessions: self.sessions.clone(),
            pool: self.pool.clone(),
            autoloader: self.autoloader.clone(),
            features,
        };
    }

    pub fn apply_fallback(&mut self) -> App<ConnectionPool, Features> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        async fn handler_404() -> impl IntoResponse {
            (StatusCode::NOT_FOUND, "nothing to see here")
        }

        router = router.fallback(handler_404);

        return App { 
            config: self.config.clone(),
            pool: self.pool.clone(),
            sessions: self.sessions.clone(),
            autoloader: self.autoloader.clone(),
            router,
            features
        };
    }

    pub fn apply_extension<S: Clone + Send + Sync + 'static>(&mut self, state: S) -> App<ConnectionPool, Features> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        router = router.layer(Extension(state));

        return App {
            config: self.config.clone(),
            pool: self.pool.clone(),
            sessions: self.sessions.clone(),
            autoloader: self.autoloader.clone(),
            router,
            features,
        };
    }

    pub fn build(&mut self) -> App<ConnectionPool, Features>{
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        let mut context_layer: ContextLayer = ContextLayer::new(self.config.clone());

        // 1. scan features and extract links for navigator 
        for feature in features.iter() {
            match feature.link() {
                Some(link) => {
                    context_layer.add_link(link);
                },
                _ => {}
            }
        }

        // 2. scan features and apply routers
        for feature in features.iter() {
            router = match feature.api() {
                Some(mut r) => {
                    r = r.layer(context_layer.clone());
                    router.merge(r)
                }, 
                None => router
            };

            router = match feature.supplemental() {
                Some(mut r) => {
                    r = r.layer(TemplateLayer::new(None,
                    self.autoloader.clone()))
                        .layer(context_layer.clone());
                    router.merge(r)
                }, 
                None => router
            };

            router = match feature.web() {
                Some(mut r) => {
                    r = r
                        .layer(TemplateLayer::new(Some(self.config.server.shell_template.clone()),
                        self.autoloader.clone()))
                        .layer(context_layer.clone());
                    router.merge(r)
                }, 
                None => router
            };

            router = match feature.protected_api() {
                Some(mut r) => {
                    r = r
                        .layer(context_layer.clone());
                    router.merge(r)
                }, 
                None => router
            };

            router = match feature.protected_supplemental() {
                Some(mut r) => {
                    r = r
                        .layer(TemplateLayer::new(None, self.autoloader.clone()))
                        .layer(context_layer.clone());

                        router.merge(r)
                }, 
                None => router
            };

            router = match feature.protected_web() {
                Some(mut r) => {
                    r = r
                        .layer(TemplateLayer::new(Some(self.config.server.shell_template.clone()),
                        self.autoloader.clone()))
                        .layer(context_layer.clone());
                        
                    router.merge(r)
                }, 
                None => router
            };
        }
    
        router = router

            // core layers
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    // Vanilla middleware
                    .layer(CorsLayer::new())
                    .layer(CompressionLayer::new())
                    .layer(TimeoutLayer::new(Duration::from_secs(10)))
            )

            // base extensions (database connection)
            .layer(Extension(self.pool.clone()));

            if self.sessions.is_some(){
                // apply global session
                router = router
                    .layer(self.sessions.clone().unwrap().common.clone())
                    .layer(self.sessions.clone().unwrap().protected.clone());
            }

        App {
            config: self.config.clone(),
            pool: self.pool.clone(),
            sessions: self.sessions.clone(),
            autoloader: self.autoloader.clone(),
            features,
            router,
        }
    }

    pub fn apply_session(&mut self, schema: &str, table: &str) -> App<ConnectionPool, Features>{
        let router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        let session_store: BlandworkSessionStore = BlandworkSessionStore::new(
            &self.pool,
            schema.to_string(),
            table.to_string()
        );

        // Generate a cryptographic key to sign the session cookie.
        let key: Key = Key::generate();
        
        // common session layer, provides Session extract
        let session_layer: SessionManagerLayer<BlandworkSessionStore, tower_sessions::service::SignedCookie> = SessionManagerLayer::new(session_store)
            .with_secure(false)
            // .with_expiry(Expiry::OnSessionEnd)
            .with_expiry(Expiry::OnInactivity(time::Duration::days(1)))
            .with_signed(key);

        let vault: Vault = Vault::new(self.pool.clone());

        let auth_layer = AuthManagerLayerBuilder::new(vault, session_layer.clone()).build();

        App {
            config: self.config.clone(),
            pool: self.pool.clone(),
            sessions: Some(Sessions{
                common: session_layer,
                protected: auth_layer
            }),
            autoloader: self.autoloader.clone(),
            features,
            router,
        }
    }

    pub async fn run(&mut self) {
        let listener: TcpListener = TcpListener::bind(format!("{host}:{port}", host=self.config.server.host, port=self.config.server.port))
            .await
            .unwrap();
        
        // tracing_subscriber::fmt::fmt().with_env_filter(EnvFilter::from_default_env()).init();
        let stdout = tracing_subscriber::fmt::layer().pretty();
        let subscriber = Registry::default().with(stdout);
    
        tracing::subscriber::set_global_default(subscriber)
            .expect("Unable to set global subscriber");
        
        axum::serve(listener, self.router.clone()).await.unwrap();
    }
}
