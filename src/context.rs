use axum::extract::Request;
use axum_htmx::HX_BOOSTED;
use hyper::HeaderMap;
use maud::Markup;

use crate::{navigator, Navigator};


/// Trait for rendering maud components with context
pub trait Component {
    fn render(&self, context: &Context) -> Markup;
}

/// The Blandwork Context is responsible for communicating 
/// non-session specific UI state such as:
/// - Incoming Headers (HTMX-aware)
/// - Navigation Options
///   - Including current selection based on route.
#[derive(Debug)]
pub struct Context {
    pub headers: HeaderMap,
    pub path: String,
    pub navigator: Navigator
}

impl Context {
    pub fn build(request: &Request, navigator: Navigator) -> Self {
        let headers: HeaderMap = request.headers().clone();
        let path: String = request.uri().path().to_owned();

        Context{
            path,
            headers,
            navigator: navigator.clone(),
        }
    }

    pub fn title(&self) -> String {
        match self.navigator.current_link() {
            Some(l) => {
                l.title.to_owned()
            },
            None => {
                "".to_owned()
            }
        }
    }

    pub fn is_boosted(&self) -> bool {
        return self.headers.contains_key(HX_BOOSTED);
    }
}
