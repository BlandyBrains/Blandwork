use axum::Router;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub name: String,
    pub route: String,
    pub icon: Option<String>,
    pub css: Option<String>
}

/// Features are not Clone + Send + Sync due to our application builder.
/// They are meant to be for definition and configuration purposes
/// and are not accessible during requests.
pub trait Feature {
    
    /// Navigation hook to the entrypoint into the feature
    fn link(&self) -> Option<Link> {
        None
    }

    // fn menu(&self) -> Option<Markup> {
    //     None
    // }

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
