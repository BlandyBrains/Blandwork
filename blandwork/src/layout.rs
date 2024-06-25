use crate::{
    template::Template, Context, Feature
};
use std::{future::Future, pin::Pin, sync::Arc, 
    task::{Context as TaskContext, Poll}
};
use tokio::sync::Mutex;

use hyper::Response;
use maud::PreEscaped;
use tower::{Layer, Service};
use axum::{
    body::{to_bytes, Body}, 
    extract::Request, response::IntoResponse
    // http:{Request, Response}
};


pub trait Layout: Clone + Send + Sync {
    fn template(&self) -> impl Template;
    fn register(&mut self, feature: &Box<dyn Feature + 'static>);
}

#[derive(Clone)]
pub struct LayoutLayer<L: Layout> {
    layout: L
}

impl<L> LayoutLayer<L>
where L: Layout {
    pub fn new(layout: L) -> Self {
        Self { layout }
    }
}

impl<S, L> Layer<S> for LayoutLayer<L>
where L: Layout {
    type Service = LayoutService<S, L>;

    fn layer(&self, inner: S) -> Self::Service {
        LayoutService { 
            inner, 
            layout: self.layout.clone(),
        }
    }
}

#[derive(Clone)]
pub struct LayoutService<S, L> {
    inner: S,
    layout: L
}

impl<S, L> Service<Request> for LayoutService<S, L>
where
    S: Service<Request, Response = Response<axum::body::Body>> + Send + 'static,
    S::Future: Send + 'static,
    L: Layout + Clone + Send + Sync + 'static
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {

        let layout = self.layout.clone();

        tracing::info!("Framework request begin...");

        let extensions = req.extensions_mut();
        
        extensions.insert(layout.clone());

        let context = extensions.get::<Arc<Mutex<Context>>>().unwrap().clone();

        let inner = self.inner.call(req);
        
        Box::pin(async move {
            let mut response: Response<axum::body::Body> = inner.await?;

            let ctx = context.lock().await;
            
            tracing::info!("Framework request end...");

            if ctx.is_boosted() {
                return Ok(response);
            }

            let body: Body = response.into_body();

            // read the entire inner response body into bytes
            // then convert to string and pass into page template
            response = match to_bytes(body, usize::MAX).await {
                Ok(s) => {
                    let new_body = layout.template().page(
                        &ctx,
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
