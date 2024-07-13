use std::{
    collections::HashMap, 
    fmt::Display, future::Future, pin::Pin, 
    sync::Arc, task::{Context as TaskContext, Poll}
};
use tokio::sync::{Mutex, MutexGuard};

use axum::{extract::Request, http::HeaderValue};
use axum_htmx::{HX_BOOSTED, HX_REQUEST, HX_TRIGGER};
use hyper::{HeaderMap, Response};
use serde::{ser::SerializeMap, Serialize};
use serde_json::{json, to_string};
use tower::{Layer, Service};
use uuid::Uuid;

use crate::{Config, Feature, Link};

pub trait Serializable: Send + Sync {
    fn serialize(&self) -> String;
}

impl<T> Serializable for T
where
    T: Serialize + Send + Sync,
{
    fn serialize(&self) -> String {
        to_string(self).unwrap()
    }
}

pub struct Event {
    pub key: String,
    pub data: Option<Box<dyn Serializable>>
}

impl Event {
    pub fn new<T: Serializable + 'static>(key: String, data: T) -> Self {
        Self { key, data: Some(Box::new(data)) }
    }

    pub fn empty(key: String) -> Self{
        Self {key, data: None}
    }
}

impl Serialize for Event {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
            // Flatten the `data` field if it exists
            if let Some(ref data) = self.data {
                let mut map = serializer.serialize_map(None)?;
                let json_string = data.serialize();
                let data_map: HashMap<String, serde_json::Value> = serde_json::from_str(&json_string).map_err(serde::ser::Error::custom)?;
                
                for (k, v) in data_map {
                    map.serialize_entry(&k, &v)?;
                }
                map.end()
            } else {
                serializer.serialize_none()
            }       
    }
}

pub struct Triggers {
    triggers: Vec<Event>
}

impl Triggers {
    pub fn new() -> Self {
        Self { triggers: Vec::new() }
    }

    pub fn add(&mut self, event: Event) {
        self.triggers.push(event)
    }

    fn group_triggers(&self) -> HashMap<String, Vec<&Event>> {
        let mut grouped_events: HashMap<String, Vec<&Event>> = HashMap::new();
    
        for event in self.triggers.iter() {
            grouped_events.entry(event.key.clone())
                .or_insert_with(Vec::new)
                .push(event);
        }
    
        grouped_events
    }
}

impl Serialize for Triggers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let groups: HashMap<String, Vec<&Event>> = self.group_triggers();
        let mut map = serializer.serialize_map(None)?;

        for (key, g) in &groups {
            let count = g.len();

            if count > 1 {
                map.serialize_entry(key, g)?;
            } 
            else {
                map.serialize_entry(key, g[0])?;
            }
        }
        map.end()
    }
}

impl Display for Triggers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", to_string(self).unwrap())
    }
}

pub struct Ctx {
    pub title: String,
    pub context_id: String,
    pub path: String,

    // request headers
    headers: HeaderMap,

    // response triggers
    triggers: Triggers,

    links: Vec<Link>
}

impl Ctx {
    pub fn build(config: &Config, links: Vec<Link>, request: &Request) -> Self  {
        let headers: HeaderMap = request.headers().clone();
        let path: String = request.uri().path().to_owned();

        Ctx {
            links,
            title: config.title.clone(),
            context_id: Uuid::new_v4().to_string(),
            path,
            headers,
            triggers: Triggers::new(),
        }
    }
}

#[derive(Clone)]
pub struct ContextAccessor(Arc<Mutex<Ctx>>);

impl ContextAccessor { 
    pub async fn get(&self) -> PageContext {
        let ctx = self.0.lock().await;
        PageContext(ctx)
    }

    pub fn from_request(config: &Config, links: Vec<Link>, request: &Request) -> Self {
        let ctx: Ctx = Ctx::build(&config, links, &request);
        return ContextAccessor(Arc::new(Mutex::new(ctx)));
    }
}

pub struct PageContext<'a>(MutexGuard<'a, Ctx>);

impl<'a> PageContext<'a> {

    pub fn title(&mut self, section: &str) -> String {
        let title: String;

        if section.len() > 0 {
            title =  format!("{} | {}", self.0.title, section);
        } else {
            title = format!("{}", self.0.title);
        }
        self.0.title = title.clone();
        self.add_trigger("titleUpdate".to_owned(), json!({ "title": title.clone() }));
        
        return title;
    }

    pub fn links(&self) -> Vec<Link> {
        return self.0.links.clone();
    }

    pub fn id(&self) -> String {
        return self.0.context_id.clone();
    }
    
    pub fn is_htmx(&self) -> bool {
        return self.0.headers.contains_key(HX_REQUEST);
    }

    pub fn is_boosted(&self) -> bool {
        return self.0.headers.contains_key(HX_BOOSTED);
    }

    pub fn add_trigger<E: Serializable + 'static>(&mut self, key: String, data: E) {
        self.0.triggers.add(Event::new(key, data));
    }

    pub fn empty_trigger(&mut self, key: String) {
        self.0.triggers.add(Event::empty(key));
    }

    pub fn triggers(&self) -> HeaderValue {
        self.0.triggers.to_string().parse().unwrap()
    }
}

#[derive(Clone)]
pub struct ContextLayer {
    config: Arc<Config>,
    links: Vec<Link>
}

impl ContextLayer {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config, links: vec![] }
    }

    pub fn add_link(&mut self, link: Link) {
        self.links.push(link);
    }
}

impl<S> Layer<S> for ContextLayer {
    type Service = ContextService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ContextService { 
            config: self.config.clone(),
            links: self.links.clone(),
            inner,
        }
    }
}

#[derive(Clone)]
pub struct ContextService<S> {
    config: Arc<Config>,
    links: Vec<Link>,
    inner: S,
}

impl<S> Service<Request> for ContextService<S>
where
    S: Service<Request, Response = Response<axum::body::Body>>  + Send + 'static,
    S::Future: Send + 'static
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        tracing::info!("context layer start");

        // build context
        let accessor: ContextAccessor = ContextAccessor::from_request(&self.config, self.links.clone(), &req);

        // send the context into the handler
        let extensions = req.extensions_mut();
        extensions.insert( accessor.clone());

        let inner = self.inner.call(req);

        Box::pin(async move {
            let mut response: Response<axum::body::Body> = inner.await?;

            let context: PageContext = accessor.get().await;

            tracing::info!("context layer wrap {:#?}", context.is_boosted());
            
            // only fire events on boosted requests
            // they will not register on initial load
            if context.is_boosted() {
                // HX-Trigger https://htmx.org/headers/hx-trigger/
                let mut headers: HeaderMap = HeaderMap::new();
                headers.insert(HX_TRIGGER, context.triggers());
                response.headers_mut().extend(headers);
            }

            tracing::info!("context layer end");
            Ok(response)
        })
    }

}

#[cfg(test)]
mod test {
    use serde::Serialize;

    use super::{Event, Triggers};

    #[derive(Serialize)]
    pub struct FakeData{
        pub name: String
    }

    #[test]
    fn test_trigger_serialize_event() {
        let mut triggers: Triggers = Triggers::new();
        
        triggers.add(Event::new("SOME_EVENT_KEY".to_owned(), FakeData{name: "SOME_EVENT_DATA".to_owned()}));
        
        assert_eq!(serde_json::to_string(&triggers).unwrap(), "{\"SOME_EVENT_KEY\":{\"name\":\"SOME_EVENT_DATA\"}}"); 
    }

    #[test]
    fn test_trigger_serialize_empty_event() {
        let mut triggers: Triggers = Triggers::new();
        
        triggers.add(Event::empty("SOME_EVENT_KEY".to_owned()));
        
        println!("{}", serde_json::to_string(&triggers).unwrap());
    }

    #[test]
    fn test_trigger_serialize_multiple_empty_event_same_key() {
        let mut triggers = Triggers::new();
        
        triggers.add(Event::empty("SOME_EVENT_KEY".to_owned()));
        triggers.add(Event::empty("SOME_EVENT_KEY".to_owned()));
        
        // todo - is this bad?
        // maybe it's helpful to know how many times an event is triggered?
        assert_eq!(serde_json::to_string(&triggers).unwrap(), "{\"SOME_EVENT_KEY\":[null,null]}"); 
    }

    #[test]
    fn test_trigger_serialize_multiple_mixed_event_same_key() {
        let mut triggers = Triggers::new();
        
        triggers.add(Event::empty("SOME_EVENT_KEY".to_owned()));
        triggers.add(Event::new("SOME_EVENT_KEY".to_owned(), FakeData{name: "SOME_EVENT_DATA".to_owned()}));
        
        assert_eq!(serde_json::to_string(&triggers).unwrap(), "{\"SOME_EVENT_KEY\":[null,{\"name\":\"SOME_EVENT_DATA\"}]}");
    }

    #[test]
    fn test_trigger_serialize_mixed_key() {
        let mut triggers = Triggers::new();
        
        triggers.add(Event::empty("SOME_EVENT_KEY".to_owned()));
        triggers.add(Event::empty("SOME_EVENT_KEY_2".to_owned()));
        triggers.add(Event::new("SOME_EVENT_KEY".to_owned(), FakeData{name: "SOME_EVENT_DATA".to_owned()}));
        
        println!("{}", serde_json::to_string(&triggers).unwrap());
        // assert_eq!(serde_json::to_string(&triggers).unwrap(), "{\"SOME_EVENT_KEY\":[null,{\"name\":\"SOME_EVENT_DATA\"}]}");
    }
}
