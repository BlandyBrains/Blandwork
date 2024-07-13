use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Extension, Router};
use blandwork::{App, Config, ContextAccessor, Feature, Link, TemplateAccessor};
use minijinja::context;
use serde::Serialize;

// Say we want to send a custom event from our feature to HTMX.
#[derive(Serialize)]
pub struct SampleEvent{
    pub data: String
}

#[derive(Clone, Default)]
struct SampleFeature;

impl SampleFeature {
    async fn endpoint(
        Extension(template): Extension<TemplateAccessor>) -> impl IntoResponse {
        tracing::info!("inside of endpoint");
        template.render("endpoint.html", context!{}).await
    }

    async fn more(
        Extension(context): Extension<ContextAccessor>,
        Extension(template): Extension<TemplateAccessor>,
       ) -> impl IntoResponse {

        // using context from handler
        let mut context: blandwork::PageContext = context.get().await;

        tracing::info!("from handler context={} , is_boosted {}", context.id(), context.is_boosted());

        context.add_trigger(
            "MY_FEATURE_TRIGGER".to_owned(), 
            SampleEvent { data: "THIS WOULD BE SOME DATA".to_string() });

        template.render("nested/template.html", context!{}).await
    }
}

impl Feature for SampleFeature {
    fn link(&self) -> Option<Link> {
        Some(Link {
            name: "A".to_string(),
            route: "/sample/web".to_string(),
            icon: None,
            css: None
        })
    }

    fn web(&self) -> Option<Router> {
        Some(Router::new()
            .route("/", get(SampleFeature::endpoint))
            .route("/more", get(SampleFeature::more))
            // .route("/sample/select-", get(SampleFeature::select))
            // .route("/sample/other", get(SampleFeature::other))
        )
    }
}

#[tokio::main]
async fn main() {
    App::new(Config::default())
        .register_feature_default::<SampleFeature>()
        .apply_fallback()
        .build()
        .run().await;
}