use axum::Router;
use maud::{html, Markup};
use serde::Serialize;

use crate::{ConnectionPool, Context};

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub active: bool,
    pub title: String,
    pub label: String,
    pub route: String,
    pub icon: Option<String>,
    pub css: Option<String>
}
impl Link {
    pub fn render(&self, _: &Context) -> Markup {
        let active_class: String = match self.active {
            true => "bg-gray-400".to_owned(),
            false => "bg-gray-600".to_owned()
        };

        html!{
            a href=(self.route)
                hx-target="#content"
                hx-swap="innerHTML"
                class={"w-14 h-14 my-1 flex justify-center items-center no-underline duration-200 rounded-xl hover:bg-gray-500 " (active_class) ""} {
                    (self.label) 
                }
        }
    }
}

/// Features are not Clone + Send + Sync due to our application builder.
/// They are meant to be for definition and configuration purposes
/// and are not accessible during requests.
pub trait Feature {
    
    /// Navigation hook to the entrypoint into the feature
    fn link(&self) -> Option<Link> {
        None
    }

    fn menu(&self) -> Option<Markup> {
        None
    }

    /// API endpoints exposed from the feature
    fn api(&self) -> Option<Router> {
        return None;
    }

    /// Supplemental endpoints are routes that should only be accessed from the web endpoints.
    /// These routes are wrapped in the Context middleware and are not HTMX aware.
    /// use cases:
    /// - Search Results
    fn supplemental(&self) -> Option<Router> {
        return None;
    }

    /// Web endpoints are routes that can be accessed directly or boosted after entering the application.
    /// These routes are wrapped in the Context and Template middleware, the template will ALWAYS be applied 
    /// if the incoming request is not HX-Boosted.
    fn web(&self) -> Option<Router> {
        return None;
    }
}

pub type FeatureError = Box<dyn std::error::Error>;

pub trait Component {
    fn render(&self, context: &Context) -> Markup {
        html!{
            b { 
                "Component has not been implemented!"
            }
        }
    }
}