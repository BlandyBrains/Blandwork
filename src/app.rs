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
    db::ConnectionPool, 
    feature::Feature, 
    template::{Template, VanillaTemplate}, 
    Config, FrameworkLayer, Navigator};

#[derive(Clone)]
pub struct NoPool;

#[derive(Clone)]
pub struct NoFeatures;

pub type Features = Vec<Box<(dyn Feature + 'static)>>;

pub struct App<P, F, T> where T: Template{
    config: Config,
    router: Router,
    pool: P,
    features: F,
    navigator: Navigator,
    template: T
}

impl App<NoPool, NoFeatures, VanillaTemplate>{
    pub fn new(config: Config) -> App<NoPool, NoFeatures, VanillaTemplate> {
        App{
            config,
            router: Router::new(),
            pool: NoPool,
            features: NoFeatures,
            navigator: Navigator::default(),
            template: VanillaTemplate{}
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
            navigator: self.navigator.clone(),
            template: self.template.clone()
        };
    }

    pub fn template<F: Template>(&self, template: F) -> App<NoPool, NoFeatures, F> {
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: NoPool,
            features: NoFeatures,
            navigator: Navigator::default(),
            template,
        }
    }

    pub fn register_feature_default<F: Feature + Default + 'static>(&self) ->  App<NoPool, Features, T>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(F::default())
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: NoPool,
            navigator: self.navigator.clone(),
            template: self.template.clone(),
            features,
        };
    }

    pub fn register_feature(&self, feature: impl Feature + 'static) ->  App<NoPool, Features, T>{         
        let features: Vec<Box<dyn Feature + 'static>> = vec![
            Box::new(feature)
        ];

        return App { 
            config: self.config.clone(),
            router: self.router.clone(),
            pool: NoPool,
            navigator: self.navigator.clone(),
            features,
            template: self.template.clone()
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
            navigator: self.navigator.clone(),
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
            navigator: self.navigator.clone(),
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
            navigator: self.navigator.clone(),
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
            navigator: self.navigator.clone(),
            template: self.template.clone(),
            router,
            features,
        };
    }

    pub fn template<F: Template + 'static>(&mut self, template: F) -> App<NoPool, Features, F> {
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: NoPool,
            navigator: Navigator::default(),
            features,
            template,
        }
    }

    pub fn build(&mut self) -> App<NoPool, Features, T>{
        let mut navigator: Navigator = self.navigator.clone();
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
    
        // 1. scan features and extract links for navigator
        for feature in features.iter() {
            match feature.link() {
                Some(x) => {
                    navigator.add_link(x)
                },
                None => {}
            }
        }

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
                    //
                    // right here ---
                    // the middleware must have a reference to the template
                    // the feature should choose the templates and where they should be applied.
                    web = web.layer(FrameworkLayer::new(navigator.clone(), self.template.clone()));
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

            // base extensions (database connection)
            // .layer(Extension(self.pool.clone()));

            // others? Feature specific data/configurations?

        return App {
            config: self.config.clone(),
            pool: self.pool.clone(),
            navigator: self.navigator.clone(),
            features: Vec::new(),
            template: self.template.clone(),
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
            navigator: self.navigator.clone(),
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
            navigator: self.navigator.clone(),
            features,
            template: self.template.clone()
        };
    }

    pub fn template<F: Template>(&self, template: F) -> App<ConnectionPool, NoFeatures, F> {
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: self.pool.clone(),
            features: NoFeatures,
            navigator: Navigator::default(),
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
            navigator: self.navigator.clone(),
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
            navigator: self.navigator.clone(),
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
            navigator: self.navigator.clone(),
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
            navigator: self.navigator.clone(),
            template: self.template.clone(),
            router,
            features,
        };
    }

    pub fn template<F: Template + 'static>(&mut self, template: F) -> App<ConnectionPool, Features, F> {
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
        
        App { 
            config: self.config.clone(), 
            router: self.router.clone(), 
            pool: self.pool.clone(),
            navigator: Navigator::default(),
            features,
            template,
        }
    }

    pub fn build(&mut self) -> App<ConnectionPool, Features, T>{
        let mut navigator: Navigator = self.navigator.clone();
        let mut router: Router = mem::replace(&mut self.router, Router::new());
        let features: Vec<Box<dyn Feature>> = mem::replace(&mut self.features, Vec::new());
    
        // 1. scan features and extract links for navigator
        for feature in features.iter() {
            match feature.link() {
                Some(x) => {
                    navigator.add_link(x)
                },
                None => {}
            }
        }

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
                    //
                    // right here ---
                    // the middleware must have a reference to the template
                    // the feature should choose the templates and where they should be applied.
                    web = web.layer(FrameworkLayer::new(navigator.clone(), self.template.clone()));
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
            navigator: self.navigator.clone(),
            features: Vec::new(),
            template: self.template.clone(),
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
    use std::time::Duration;

    use axum::{response::IntoResponse, routing::get, Router};
    use hyper::{HeaderMap, StatusCode};
    use maud::html;

    use crate::{
        config::Config, Link
    };
    use super::{App, Feature};

    #[derive(Default)]
    struct SampleFeature;

    impl SampleFeature {
        async fn endpoint() -> impl IntoResponse {
            let body = html!{
                div 
                    class="flex flex-col justify-start items-center w-full" {
                    div {
                        b { "Hi! From Sample Feature." }
                    }
                    br;
                    div #new-content {
                        b { "I will be replaced" }
                    }
                    br;
                    a href="/sample/more" 
                        // hx-target="#new-content" 
                        hx-target="#content"
                        hx-swap="innerHTML"
                        {
                        strong {"Click here"} " to replace all content"
                    }
                    br;

                    // Don't do this! 
                    div hx-boost="true" {
                        button
                            hx-boost="true"
                            hx-get="/sample/web" 
                            // this works
                            // hx-headers="{\"HX-Boosted\":\"true\"}"

                            // hx-target="#new-content" 
                            // hx-select="#other"
                            hx-target="#content"
                            // hx-swap="innerHTML"
                            hx-push-url="true" {    
                            strong {"Click here"} " to replace select content"
                        }
                    }
                }
            };

            let mut headers = HeaderMap::new();
            // headers.insert(header::ETAG, "1".parse().unwrap());
            // headers.insert(header::LAST_MODIFIED, "Wed, 21 Oct 2015 07:28:00 GMT".parse().unwrap());
            // headers.insert(header::VARY, "HX-Request".parse().unwrap());

            tracing::info!("inside of endpoint");
            return (
                StatusCode::OK,
                headers,
                body
            );
        }

        async fn other() -> maud::Markup {
            let body = html!{
                div class="flex flex-col justify-start items-center w-full" {
                    div {
                        b { "Some Other Page!" }
                    }
                }
            };

            tokio::time::sleep(Duration::from_secs(2)).await;
            body
        }

        async fn more() -> maud::Markup {
            return maud::html!{
                b { "More content" }
            }
        }

        async fn select() -> maud::Markup {
            return maud::html!{
                b { "outer content (should not see this)" }
                div #other {
                    b { "the inner content" }
                }
            }
        }
    }

    impl Feature for SampleFeature {
        fn name(&self) -> String {
            return "SampleFeature".to_owned();
        }

        fn web(&self) -> Option<axum::Router> {
            Some(Router::new()
                .route("/sample/web", get(SampleFeature::endpoint))
                .route("/sample/more", get(SampleFeature::more))
                .route("/sample/select-", get(SampleFeature::select))
                .route("/sample/other", get(SampleFeature::other))
                
                // a feature has a choice to use the framework middleware
                // or to be a vanilla handler

                // the problem is hooking the navigator + template into the feature for consumption
                // .layer(FrameworkLayer::new(navigator.clone(), VanillaHtmxTemplate{}))
            )
        }

        fn link(&self) -> Option<Link> {
            Some(Link {
                title: "A".to_string(),
                label: "A".to_string(),
                active: false,
                route: "/sample/web".to_string(),
                icon: None,
                css: None
            })
        }
    }

    #[derive(Default)]
    struct SampleFeatureToo;

    impl SampleFeatureToo {
        async fn endpoint() -> impl IntoResponse {
            let body = html!{
                div 
                    class="flex flex-col justify-start items-center w-full" {
                    div {
                        b { "Hi! From Sample Feature Too!" }
                    }
                }
            };
            return (
                StatusCode::OK,
                body
            );
        }
    }

    impl Feature for SampleFeatureToo {
        fn name(&self) -> String {
            return "SampleFeatureToo".to_owned();
        }
        
        fn link(&self) -> Option<Link> {
            Some(Link {
                active: false,
                title: "B".to_string(),
                label: "B".to_string(),
                route: "/sample-too/web".to_string(),
                icon: None,
                css: None
            })
        }

        fn web(&self) -> Option<axum::Router> {
            Some(Router::new()
                .route("/sample-too/web", get(SampleFeatureToo::endpoint))
            )
        }
    }

    #[tokio::test]
    async fn test_app_sample() {
        let config: Config = Config::default();

        App::new(config)
            .register_feature_default::<SampleFeature>()
            .register_feature_default::<SampleFeatureToo>()
            .apply_fallback()
            // .template(VanillaHtmxTemplate{})
            .build()     
            .run().await;
    }
}
