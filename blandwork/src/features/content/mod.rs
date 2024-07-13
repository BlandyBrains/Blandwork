use axum::Router;
use serde::Deserialize;
use tower_http::services::ServeDir;
use crate::{Config, Feature};


#[derive(Clone, Deserialize)]
pub struct ContentPath {
    key: String,
    mountpoint: String
}

impl ContentPath {
    pub fn new(key: &str, mountpoint: &str) -> Self {
        Self {
            key: key.to_string(), 
            mountpoint: mountpoint.to_string(),
        }
    }

    pub fn path(&self) -> String {
        return format!("/static/{0}", self.key);
    }
}

pub struct ContentFeature {
    roots: Vec<ContentPath>
}

impl Default for ContentFeature {
    fn default() -> Self {
        Self { roots: Default::default() }
    }
}

impl From<Config> for ContentFeature {
    fn from(config: Config) -> Self {
        let mut feature = ContentFeature::default();
        config.server.content_paths.iter().for_each(|f| feature.add_path(f.clone()));
        return feature;
    }
}

impl ContentFeature {
    pub fn add_path(&mut self, content_path: ContentPath) {
        self.roots.push(content_path);
    }
}

impl Feature for ContentFeature {

    fn api(&self) -> Option<axum::Router> {
        let mut app: Router = Router::new();
    
        for static_path in self.roots.iter() {
            app = app.nest_service(&static_path.path(), ServeDir::new(static_path.mountpoint.clone()));
        };

        return Some(app);
    }
}
