use std::cmp::Reverse;

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

#[derive(Debug, Clone)]
pub struct Navigator {
    links: Vec<Link>
}

impl Navigator {
    pub fn new() -> Self {
        return Self::default();
    }

    pub fn size(&self) -> usize {
        return self.links.len();
    }

    pub fn set_current(&mut self, path: &str) {
        // let mut links: Vec<_> = self.links.clone().into_iter().collect();
        self.links.sort_by_key(|link| Reverse(link.route.clone()));

        self.links.iter_mut().for_each(|x| {
            x.active = false;
        });

        for link in self.links.iter_mut() {
            tracing::info!("checking link {:#?} with {:#?}", link, path);
            if link.route.starts_with(path) {
                link.active = true;
                break;
            }
        }
    }

    pub fn add_link(&mut self, link: Link) {
        self.links.push(link)
    }

    pub fn current_link(&self) -> Option<&Link> {
        self.links.iter().find(|&x| x.active)
    }

}

impl Component for Navigator {
    fn render(&self, context: &Context) -> Markup {
        html!{
            @for link in &self.links {
                (link.render(context))
            }
        }
    }
}
impl Default for Navigator {
    fn default() -> Self {
        Self { links: vec![] }
    }
}