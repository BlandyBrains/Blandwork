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
    db::ConnectionPool, 
    feature::Feature, 
    layout::{Layout, LayoutLayer}, 
    Config,
};

#[derive(Clone)]
pub struct NoPool;

#[derive(Clone)]
pub struct NoFeatures;

pub type Features = Vec<Box<(dyn Feature + 'static)>>;

pub struct App<P, F, L> where L: Layout {
    // application configuration
    config: Config,

    // application router
    router: Router,

    // application layout
    layout: L,

    // features should be decoupled from navigator/template/theme.
    // they can reference the current theme in their handlers.
    features: F,

    // optional and only matters for Extension() on router
    // Features could use it in their handlers, but we can't know that during build.
    pool: P,
}

impl<L> App<NoPool, NoFeatures, L> where L: Layout {
    pub fn new(config: Config, layout: L) -> App<NoPool, NoFeatures, L> {
        App{
            config,
            layout,
            router: Router::new(),
            pool: NoPool,
            features: NoFeatures,
        }
    }
}

impl<L> App<NoPool, NoFeatures, L> where L: Layout + 'static {
    pub async fn connect(&mut self) -> App<ConnectionPool, NoFeatures, L> { 
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
            layout: self.layout.clone()
        };
    }

    pub fn register_feature_default<F: Feature + Default + 'static>(&self) ->  App<NoPool, Features, L>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(F::default())
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            layout: self.layout.clone(),
            pool: NoPool,
            features,
        };
    }

    pub fn register_feature(&self, feature: impl Feature + 'static) ->  App<NoPool, Features, L>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(feature)
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: NoPool,
            layout: self.layout.clone(),
            features,
        };
    }
}

impl<L> App<NoPool, Features, L> where L: Layout + 'static  {
    pub fn register_feature_default<F: Feature + Default + 'static>(&mut self) ->  App<NoPool, Features, L>{
        self.features.push(Box::new(F::default()));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: NoPool,
            layout: self.layout.clone(),
            features,
        };
    }

    pub fn register_feature(&mut self, feature: impl Feature + 'static) ->  App<NoPool, Features, L>{         
        self.features.push(Box::new(feature));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: NoPool,
            layout: self.layout.clone(),
            features,
        };
    }

    pub fn apply_fallback(&mut self) -> App<NoPool, Features, L> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        async fn handler_404() -> impl IntoResponse {
            (StatusCode::NOT_FOUND, "nothing to see here")
        }

        router = router.fallback(handler_404);

        return App { 
            config: self.config.clone(),
            pool: NoPool,
            layout: self.layout.clone(),
            router,
            features
        };
    }

    pub fn apply_extension<S: Clone + Send + Sync + 'static>(&mut self, state: S) -> App<NoPool, Features, L> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        router = router.layer(Extension(state));

        return App {
            config: self.config.clone(),
            pool: NoPool,
            layout: self.layout.clone(),
            router,
            features,
        };
    }

    pub fn layout<F: Layout + 'static>(&mut self, layout: L) -> App<NoPool, Features, L> {
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: NoPool,
            features,
            layout,
        }
    }

    pub fn build(&mut self) -> App<NoPool, Features, L>{
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
    
        // 1. scan features and extract links for navigator
        for feature in features.iter() {
            self.layout.register(feature)
        };

        // 2. scan features and apply routers
        for feature in features.iter() {
            router = match feature.api() {
                Some(api) => {
                    // what about feature specific middleware?
                    router.merge(api)
                }, 
                None => router
            };

            router = match feature.web() {
                Some(mut web) => {
                    web = web
                        .layer(LayoutLayer::new(self.layout.clone()))
                        .layer(ContextLayer::new());
                    
                    router.merge(web)
                }, 
                None => router
            };
        }
    
        router = router

            // web assets (css, javascript, etc)
            .nest_service("/web", ServeDir::new("web"))
            
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
            layout: self.layout.clone(),
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

impl<L> App<ConnectionPool, NoFeatures, L>  where L: Layout + 'static  {
    pub fn register_feature_default<F: Feature + Default + 'static>(&self) ->  App<ConnectionPool, Features, L>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(F::default())
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: self.pool.clone(),
            layout: self.layout.clone(),
            features,
        };
    }

    pub fn register_feature(&self, feature: impl Feature + 'static) ->  App<ConnectionPool, Features, L>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(feature)
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: self.pool.clone(),
            layout: self.layout.clone(),
            features,
        };
    }

    pub fn layout<F: Layout + 'static>(&mut self, layout: L) -> App<NoPool, NoFeatures, L> {
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: NoPool,
            features: NoFeatures,
            layout,
        }
    }

}

impl<L> App<ConnectionPool, Features, L> where L: Layout + 'static  {
    pub fn register_feature_default<F: Feature + Default + 'static>(&mut self) ->  App<ConnectionPool, Features, L>{
        self.features.push(Box::new(F::default()));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: self.pool.clone(),
            layout: self.layout.clone(),
            features,
        };
    }

    pub fn register_feature(&mut self, feature: impl Feature + 'static) ->  App<ConnectionPool, Features, L>{         
        self.features.push(Box::new(feature));

        // relocate features into new App
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: self.pool.clone(),
            layout: self.layout.clone(),
            features,
        };
    }

    pub fn apply_fallback(&mut self) -> App<ConnectionPool, Features, L> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());

        async fn handler_404() -> impl IntoResponse {
            (StatusCode::NOT_FOUND, "nothing to see here")
        }

        router = router.fallback(handler_404);

        return App { 
            config: self.config.clone(),
            pool: self.pool.clone(),
            layout: self.layout.clone(),
            router,
            features
        };
    }

    pub fn apply_extension<S: Clone + Send + Sync + 'static>(&mut self, state: S) -> App<ConnectionPool, Features, L> {
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        router = router.layer(Extension(state));

        return App {
            config: self.config.clone(),
            pool: self.pool.clone(),
            layout: self.layout.clone(),
            router,
            features,
        };
    }

    pub fn layout<F: Layout + 'static>(&mut self, layout: L) -> App<ConnectionPool, Features, L> {
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: self.pool.clone(),
            features,
            layout,
        }
    }

    pub fn build(&mut self) -> App<ConnectionPool, Features, L>{
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
    
        // 1. scan features and extract links for navigator
        for feature in features.iter() {
            self.layout.register(feature)
        };

        // 2. scan features and apply routers
        for feature in features.iter() {
            router = match feature.api() {
                Some(api) => {
                    // what about feature specific middleware?
                    router.merge(api)
                }, 
                None => router
            };

            router = match feature.web() {
                Some(mut web) => {
                    web = web
                        .layer(LayoutLayer::new(self.layout.clone()))
                        .layer(ContextLayer::new());
                       
                    router.merge(web)
                }, 
                None => router
            };
        }
    
        router = router

            // web assets (css, javascript, etc)
            .nest_service("/web", ServeDir::new("../web"))
            
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
            layout: self.layout.clone(),
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
