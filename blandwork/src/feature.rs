use axum::Router;
use maud::{html, Markup};
use serde::Serialize;

use crate::{Component, Context};

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub active: bool,
    pub title: String,
    pub label: String,
    pub route: String,
    pub icon: Option<String>,
    pub css: Option<String>
}
impl Component for Link {
    fn render(&self, _: &Context) -> Markup {
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


pub trait Feature {
    fn name(&self) -> String;

    fn link(&self) -> Option<Link> {
        return None;
    }

    fn api(&self) -> Option<Router> {
        return None;
    }

    fn web(&self) -> Option<Router> {
        return None;
    }
}


pub type FeatureError = Box<dyn std::error::Error>;