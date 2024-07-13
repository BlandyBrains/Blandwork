use axum::{
    body::{to_bytes, Body, Bytes}, 
    extract::Request, response::{Html, IntoResponse}
};
use axum_core::response::Response;

use std::{future::Future, pin::Pin, sync::Arc, task::{Context as TaskContext, Poll}};
use minijinja::{context, Template};
use minijinja_autoreload::{AutoReloader, EnvironmentGuard};
use hyper::StatusCode;
use serde::Serialize;
use tokio::sync::{Mutex, MutexGuard};
use tower::{Layer, Service};
use crate::{PageContext, ContextAccessor};


#[derive(Clone)]
pub struct TemplateAccessor(pub Arc<Mutex<AutoReloader>>);

impl TemplateAccessor { 
    pub async fn render<S: Serialize>(&self, template_name: &str, ctx: S) -> Response {
        let reloader: MutexGuard<AutoReloader> = self.0.lock().await;
        
        let env: EnvironmentGuard = match reloader.acquire_env() {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("error acquiring template environment {:#?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "").into_response();
            }
        };

        let template: Template = match env.get_template(template_name){
            Ok(e) => e,
            Err(e) => {
                tracing::error!("error fetching template {:#?} {:#?}", template_name, e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "").into_response();
            }
        };

        match template.render(ctx){
            Ok(s) => {
                (StatusCode::OK, Html(s)).into_response()
            },
            Err(e) => {
                tracing::error!("error rendering template {:#?} {:#?}", template_name, e);
                (StatusCode::INTERNAL_SERVER_ERROR, "").into_response()
            }
        }
    }
}

#[derive(Clone)]
pub struct TemplateLayer {
    shell_template: String,
    loader: TemplateAccessor
}

impl TemplateLayer{
    pub fn new(shell_template: String, loader: TemplateAccessor) -> Self {
        Self { shell_template, loader }
    }
}

impl<S> Layer<S> for TemplateLayer {
    type Service = TemplateService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TemplateService { 
            inner, 
            shell_template: self.shell_template.clone(),
            loader: self.loader.clone(),
        }
    }
}

#[derive(Clone)]
pub struct TemplateService<S> {
    inner: S,
    shell_template: String,
    loader: TemplateAccessor 
}

impl<S> Service<Request> for TemplateService<S>
where
    S: Service<Request, Response = Response<axum::body::Body>> + Send + 'static,
    S::Future: Send + 'static
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {

        tracing::info!("Template request begin...");

        let shell_template: String = self.shell_template.clone();
        let loader: TemplateAccessor = self.loader.clone();

        let extensions = req.extensions_mut();
        extensions.insert( loader.clone());

        let accessor: ContextAccessor = extensions.get::<ContextAccessor>().unwrap().clone();

        let inner = self.inner.call(req);
        
        Box::pin(async move {
            let response: Response<axum::body::Body> = inner.await?;

            let mut context: PageContext = accessor.get().await;

            tracing::info!("Template request end...");

            if context.is_boosted() {
                return Ok(response);
            }

            let body: Body = response.into_body();

            // read the entire inner response body into bytes
            // then convert to string and pass into page template
            let bytes: Bytes = match to_bytes(body, usize::MAX).await{
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("error converting bytes {:#?}", e);
                    return Ok((axum::http::StatusCode::INTERNAL_SERVER_ERROR, "").into_response());
                }
            };
            
            let content: String = String::from_utf8(bytes.to_vec()).unwrap();

            Ok(loader.render(&shell_template, context!(
                title => context.title(""), 
                links => context.links(),
                content => content)).await)
        })
    }

}
