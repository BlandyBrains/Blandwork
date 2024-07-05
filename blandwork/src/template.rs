use std::{future::Future, pin::Pin, sync::Arc, 
    task::{Context as TaskContext, Poll}
};
use tokio::sync::Mutex;

use hyper::Response;
use maud::{Markup, PreEscaped};
use tower::{Layer, Service};
use axum::{
    body::{to_bytes, Body}, 
    extract::Request, response::IntoResponse
    // http:{Request, Response}
};

use crate::{Context, ContextAccessor, Feature};

/// Defines the root frame for rendering components
pub trait Template: Clone + Send + Sync {
    /// when called informs service not to use for this request
    /// regardless of HTMX Boosted status.
    fn ignore(&mut self) {
        // default no-op
    }

    fn ignored(&self) -> bool { false }

    fn register(&mut self, feature: &Box<dyn Feature>) {}

    fn page(&self, context: &Context, body: Markup) -> Markup;
}

#[derive(Clone)]
pub struct TemplateLayer<T: Template> {
    template: T
}

impl<T> TemplateLayer<T>
where T: Template {
    pub fn new(template: T) -> Self {
        Self { template }
    }
}

impl<S, T> Layer<S> for TemplateLayer<T>
where T: Template {
    type Service = TemplateService<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        TemplateService { 
            inner, 
            template: self.template.clone(),
        }
    }
}

#[derive(Clone)]
pub struct TemplateService<S, T> {
    inner: S,
    template: T
}

impl<S, T> Service<Request> for TemplateService<S, T>
where
    S: Service<Request, Response = Response<axum::body::Body>> + Send + 'static,
    S::Future: Send + 'static,
    T: Template + Clone + Send + Sync + 'static
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {

        tracing::info!("Template request begin...");

        let template: Arc<Mutex<T>> = Arc::new(Mutex::new(self.template.clone()));

        let extensions = req.extensions_mut();
        extensions.insert(template.clone());

        let accessor: ContextAccessor = extensions.get::<ContextAccessor>().unwrap().clone();

        let inner = self.inner.call(req);
        
        Box::pin(async move {
            let mut response: Response<axum::body::Body> = inner.await?;

            let context: Context = accessor.context().await;

            let template = template.lock().await;
            
            tracing::info!("Framework request end...");

            if context.is_boosted() {
                return Ok(response);
            }

            if template.ignored() {
                return Ok(response);
            }

            let body: Body = response.into_body();

            // read the entire inner response body into bytes
            // then convert to string and pass into page template
            response = match to_bytes(body, usize::MAX).await {
                Ok(s) => {
                    let new_body = template.page(&context,
                    PreEscaped(String::from_utf8(s.to_vec()).unwrap()));
                    
                    new_body.into_response()
                },
                Err(_e) => {
                    Response::new("FAILED!".into())
                }
            };

            Ok(response)
        })
    }

}
