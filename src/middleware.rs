use std::{future::Future, pin::Pin, task::{Context as TaskContext, Poll}};

use axum_htmx::HX_TRIGGER;
use hyper::{HeaderMap, Response};
use maud::PreEscaped;
use serde_json::json;
use tower::{Layer, Service};
use axum::{
    body::{to_bytes, Body}, 
    extract::Request,
    response::IntoResponse,
    // http:{Request, Response}
};

use crate::{template::Template, Context, Navigator};


#[derive(Clone)]
pub struct FrameworkLayer<T: Template> {
    navigator: Navigator,
    template: T
}

impl<T> FrameworkLayer<T>
where T: Template {
    pub fn new(navigator: Navigator, template: T) -> Self {
        Self { navigator, template }
    }
}

impl<S, T> Layer<S> for FrameworkLayer<T>
where T: Template + Clone {
    type Service = FrameworkMiddleware<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        FrameworkMiddleware { 
            inner, 
            navigator: self.navigator.clone(),
            template: self.template.clone()
        }
    }
}

#[derive(Clone)]
pub struct FrameworkMiddleware<S, T> {
    inner: S,
    navigator: Navigator,
    template: T
}

impl<S, T> Service<Request> for FrameworkMiddleware<S, T>
where
    S: Service<Request, Response = Response<axum::body::Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    T: Template + Clone + Send + 'static
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let context: Context = self.new_context(&req);

        tracing::info!("Before request... context={:#?}", context);

        let template = self.template.clone();

        let inner = self.inner.call(req);

        Box::pin(async move {
            let mut response: Response<axum::body::Body> = inner.await?;

            tracing::info!("After request...");

            if context.is_boosted() {
                // HX-Trigger https://htmx.org/headers/hx-trigger/
                let mut headers: HeaderMap = HeaderMap::new();
                
                headers.insert(HX_TRIGGER, json!({
                    "navigator": context.navigator.current_link()
                })
                    .to_string()
                    .parse()
                    .unwrap());

                response.headers_mut().extend(headers);

                return Ok(response);
            }

            let body: Body = response.into_body();

            // read the entire inner response body into bytes
            // then convert to string and pass into page template
            response = match to_bytes(body, usize::MAX).await {
                Ok(s) => {
                    let new_body = template.page(
                        &context,
                    PreEscaped(String::from_utf8(s.to_vec()).unwrap()));
                    
                    new_body.into_response()
                },
                Err(_e) => {
                    Response::new("FAILED!".into())
                }
            };

            // future case: if we apply HX-* headers on initial requests

            Ok(response)
        })
    }

}

impl<S, T> FrameworkMiddleware<S, T> {
    fn new_context(&self, request: &Request) -> Context {
        let mut ctx = Context::build(request, self.navigator.clone());
        ctx.navigator.set_current(&ctx.path);
        ctx
    }
}


/*
impl<S> tower::Service<Request<Body>> for FrameworkMiddleware<S>
where
    S: tower::Service<Request<Body>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        tracing::info!("Before request...");
        let mut inner = self.inner.call(req);

        Box::pin(async move {
            let mut response = inner.await?;
            tracing::info!("After request...");
            Ok(response)

            // let body = response.body_mut().await?;
            // let body = axum::body::to_bytes(response.body(),usize::MAX).await.unwrap();

            // let etag = generate_etag(&body);

            // let headers = HeaderMap::new();
            // headers.insert(axum::http::header::ETAG, etag.parse().unwrap());

            // *response.body_mut() = Body::from(body);
            // response.headers_mut().extend(headers);

            // Ok(response)
        })
    }
}
*/