use std::{mem, str::FromStr, time::Duration, vec};
use axum::{ response::IntoResponse, Extension, Router};
use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use hyper::StatusCode;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, Registry};
use tower::builder::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer, 
    cors::CorsLayer, 
    timeout::TimeoutLayer,
    services::ServeDir, 
    trace::TraceLayer};

use crate::{
    context::ContextLayer,
    template::{TemplateLayer, Template},
    db::ConnectionPool, 
    feature::Feature, Config
};

#[derive(Clone)]
pub struct NoPool;

#[derive(Clone)]
pub struct NoFeatures;

pub type Features = Vec<Box<dyn Feature>>;

pub struct App<P, F, T> where T: Template {
    // application configuration
    config: Config,

    // application router
    router: Router,

    // application template
    template: T,

    // features should be decoupled from navigator/template/theme.
    // they can reference the current theme in their handlers.
    features: F,

    // optional and only matters for Extension() on router
    // Features could use it in their handlers, but we can't know that during build.
    pub pool: P,
}

impl<T> App<NoPool, NoFeatures, T> where T: Template {
    pub fn new(config: Config, template: T) -> App<NoPool, NoFeatures, T> {
        App{
            config,
            template,
            router: Router::new(),
            pool: NoPool,
            features: NoFeatures,
        }
    }
}

impl<T> App<NoPool, NoFeatures, T> where T: Template + 'static {
    pub async fn connect(&mut self) -> App<ConnectionPool, NoFeatures, T> { 
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
            features: NoFeatures,
            template: self.template.clone()
        };
    }

    pub fn register_feature_default<F: Feature + Default + 'static>(&self) ->  App<NoPool, Features, T>{         
        let features: Vec<Box<dyn Feature>> = vec![
            Box::new(F::default())
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            template: self.template.clone(),
            pool: NoPool,
            features,
        };
    }

    pub fn register_feature(&self, feature: impl Feature + 'static) ->  App<NoPool, Features, T>{         
        let features: Vec<Box<dyn Feature>> = vec![
            Box::new(feature)
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: NoPool,
            template: self.template.clone(),
            features,
        };
    }
}

impl<T> App<NoPool, Features, T> where T: Template + 'static  {
    pub fn register_feature_default<F: Feature + Default + 'static>(&mut self) ->  App<NoPool, Features, T>{
        self.features.push(Box::new(F::default()));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: NoPool,
            template: self.template.clone(),
            features,
        };
    }

    pub fn register_feature(&mut self, feature: impl Feature + 'static) ->  App<NoPool, Features, T>{         
        self.features.push(Box::new(feature));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: NoPool,
            template: self.template.clone(),
            features,
        };
    }

    pub fn apply_fallback(&mut self) -> App<NoPool, Features, T> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        async fn handler_404() -> impl IntoResponse {
            (StatusCode::NOT_FOUND, "nothing to see here")
        }

        router = router.fallback(handler_404);

        return App { 
            config: self.config.clone(),
            pool: NoPool,
            template: self.template.clone(),
            router,
            features
        };
    }

    pub fn apply_extension<S: Clone + Send + Sync + 'static>(&mut self, state: S) -> App<NoPool, Features, T> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        router = router.layer(Extension(state));

        return App {
            config: self.config.clone(),
            pool: NoPool,
            template: self.template.clone(),
            router,
            features,
        };
    }

    pub fn template<F: Template + 'static>(&mut self, template: T) -> App<NoPool, Features, T> {
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: NoPool,
            features,
            template,
        }
    }

    pub fn build(&mut self) -> App<NoPool, Features, T>{
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
    
        // 1. scan features and extract links for navigator
        for feature in features.into_iter() {
            self.template.register(&feature);

            router = match feature.api() {
                Some(mut api) => {
                    api = api.layer(ContextLayer::new());

                    router.merge(api)
                }, 
                None => router
            };

            router = match feature.supplemental() {
                Some(mut supp) => {
                    supp = supp
                        .layer(ContextLayer::new());
                    
                    router.merge(supp)
                }, 
                None => router
            };

            router = match feature.web() {
                Some(mut web) => {
                    web = web
                        .layer(TemplateLayer::new(self.template.clone()))
                        .layer(ContextLayer::new());
                    
                    router.merge(web)
                }, 
                None => router
            };
        }
    
        router = router

            // web assets (css, javascript, etc)
            // .nest_service("/web", ServeDir::new(self.config.server.asset_path.clone()))
            
            // core layers
            .layer(
                ServiceBuilder::new()
                
                    // build a layer for handling HTMX templating
                    // requirements
                        // define navigator (remove from extension)
                        // handle boost/non-boosted request
                    
                    // raw handlers only need to return

                    // requires more finesse
                    // https://docs.rs/axum/latest/axum/error_handling/index.html

                    // .layer(HandleErrorLayer::new(|m: Method, u: Uri, e: BoxError| async {
                    //     (
                    //     hyper::StatusCode::REQUEST_TIMEOUT,
                    //     format!("ERROR {:#?}", e)
                    //     )
                    // }))
                
                    .layer(TraceLayer::new_for_http())
                    
                    // Vanilla middleware
                    .layer(CorsLayer::new())
                    .layer(CompressionLayer::new())
                    .layer(TimeoutLayer::new(Duration::from_secs(10)))
            );

        return App {
            config: self.config.clone(),
            pool: self.pool.clone(),
            template: self.template.clone(),
            features: Vec::new(),
            router,
        };
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

impl<T> App<ConnectionPool, NoFeatures, T>  where T: Template + 'static  {
    pub fn register_feature_default<F: Feature + Default + 'static>(&self) ->  App<ConnectionPool, Features, T>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(F::default())
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: self.pool.clone(),
            template: self.template.clone(),
            features,
        };
    }

    pub fn register_feature(&self, feature: impl Feature + 'static) ->  App<ConnectionPool, Features, T>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(feature)
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: self.pool.clone(),
            template: self.template.clone(),
            features,
        };
    }

    pub fn template<F: Template + 'static>(&mut self, template: T) -> App<NoPool, NoFeatures, T> {
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: NoPool,
            features: NoFeatures,
            template,
        }
    }

}

impl<T> App<ConnectionPool, Features, T> where T: Template + 'static  {
    pub fn register_feature_default<F: Feature + Default + 'static>(&mut self) ->  App<ConnectionPool, Features, T>{
        self.features.push(Box::new(F::default()));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: self.pool.clone(),
            template: self.template.clone(),
            features,
        };
    }

    pub fn register_feature(&mut self, feature: impl Feature + 'static) ->  App<ConnectionPool, Features, T>{         
        self.features.push(Box::new(feature));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: self.pool.clone(),
            template: self.template.clone(),
            features,
        };
    }

    pub fn apply_fallback(&mut self) -> App<ConnectionPool, Features, T> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        async fn handler_404() -> impl IntoResponse {
            (StatusCode::NOT_FOUND, "nothing to see here")
        }

        router = router.fallback(handler_404);

        return App { 
            config: self.config.clone(),
            pool: self.pool.clone(),
            template: self.template.clone(),
            router,
            features
        };
    }

    pub fn apply_extension<S: Clone + Send + Sync + 'static>(&mut self, state: S) -> App<ConnectionPool, Features, T> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        router = router.layer(Extension(state));

        return App {
            config: self.config.clone(),
            pool: self.pool.clone(),
            template: self.template.clone(),
            router,
            features,
        };
    }

    pub fn template<F: Template + 'static>(&mut self, template: T) -> App<ConnectionPool, Features, T> {
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: self.pool.clone(),
            features,
            template,
        }
    }

    pub fn build(&mut self) -> App<ConnectionPool, Features, T>{
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
    
        // 1. scan features and extract links for navigator
        // for feature in features.iter() {
        //     self.layout.register(feature)
        // };

        // 2. scan features and apply routers
        for feature in features.iter() {
            router = match feature.api() {
                Some(mut api) => {
                    api = api.layer(ContextLayer::new());

                    router.merge(api)
                }, 
                None => router
            };

            router = match feature.supplemental() {
                Some(mut supp) => {
                    supp = supp
                        .layer(ContextLayer::new());
                    
                    router.merge(supp)
                }, 
                None => router
            };

            router = match feature.web() {
                Some(mut web) => {
                    web = web
                        .layer(TemplateLayer::new(self.template.clone()))
                        .layer(ContextLayer::new());
                       
                    router.merge(web)
                }, 
                None => router
            };
        }
    
        router = router

            // web assets (css, javascript, etc)
            // .nest_service("/web", ServeDir::new(self.config.server.asset_path.clone()))
            
            // core layers
            .layer(
                ServiceBuilder::new()
                
                    // build a layer for handling HTMX templating
                    // requirements
                        // define navigator (remove from extension)
                        // handle boost/non-boosted request
                    
                    // raw handlers only need to return

                    // requires more finesse
                    // https://docs.rs/axum/latest/axum/error_handling/index.html

                    // .layer(HandleErrorLayer::new(|m: Method, u: Uri, e: BoxError| async {
                    //     (
                    //     hyper::StatusCode::REQUEST_TIMEOUT,
                    //     format!("ERROR {:#?}", e)
                    //     )
                    // }))
                
                    .layer(TraceLayer::new_for_http())
                    
                    // Vanilla middleware
                    .layer(CorsLayer::new())
                    .layer(CompressionLayer::new())
                    .layer(TimeoutLayer::new(Duration::from_secs(10)))
                        
            )

            // base extensions (database connection)
            .layer(Extension(self.pool.clone()));
            
            // others? Feature specific data/configurations?

        return App {
            config: self.config.clone(),
            pool: self.pool.clone(),
            template: self.template.clone(),
            features,
            router,
        };
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

#[cfg(test)]
mod test {

}
