use std::cmp::Reverse;
use blandwork::Link;
use maud::{html, Markup};

use crate::Context;

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

impl Navigator {
    pub fn render(&self, context: &Context) -> Markup {
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