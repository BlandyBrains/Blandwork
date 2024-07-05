use template::VanillaTemplate;

use blandwork::{App, Config, Context, ContextAccessor, Feature, HeaderMap, IntoResponse, Link, Router, StatusCode};
use maud::{html, Markup};
use axum::routing::get;
use axum::Extension;
use serde::Serialize;

mod template;
mod navigator;


// Say we want to send a custom event from our feature to HTMX.
#[derive(Serialize)]
pub struct SampleEvent{
    pub data: String
}

#[derive(Clone, Default)]
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

        let headers = HeaderMap::new();
        // headers.insert(header::ETAG, "1".parse().unwrap());
        // headers.insert(header::LAST_MODIFIED, "Wed, 21 Oct 2015 07:28:00 GMT".parse().unwrap());
        // headers.insert(header::VARY, "HX-Request".parse().unwrap());

        // tracing::info!("inside of endpoint");
        return (
            StatusCode::OK,
            headers,
            body
        );
    }

    async fn other() -> Markup {
        let body = html!{
            div class="flex flex-col justify-start items-center w-full" {
                div {
                    b { "Some Other Page!" }
                }
            }
        };

        body
    }

    async fn more(Extension(accessor): Extension<ContextAccessor>) -> Markup {
        // using context from handler
        let mut context = accessor.context().await;

        tracing::info!("from handler context={} , is_boosted {}", context.id(), context.is_boosted());

        context.add_trigger(
            "MY_FEATURE_TRIGGER".to_owned(), 
            SampleEvent { data: "THIS WOULD BE SOME DATA".to_string() });

        return html!{
            b { "More content" }
        }
    }

    async fn select() -> Markup {
        return html!{
            b { "outer content (should not see this)" }
            div #other {
                b { "the inner content" }
            }
        }
    }
}


impl Feature for SampleFeature {
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

    fn web(&self) -> Option<Router> {
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
}

#[tokio::main]
async fn main() {
    App::new(Config::default(), VanillaTemplate::default())
        .register_feature_default::<SampleFeature>()
        .apply_fallback()
        .build()
        .run().await;
}