use blandwork::{Feature, Layout, Template};

use crate::template::VanillaTemplate;

#[derive(Clone)]
pub struct VanillaLayout {
    template: VanillaTemplate
}

impl Layout for VanillaLayout {

    fn template(&self) -> impl Template {
        return self.template.clone();
    }
    
    fn register(&mut self, feature: &Box<dyn Feature + 'static>) {
        match feature.link() {
            Some(x) => {
                self.template.navigator.add_link(x)
            },
            None => {}
        }
    }
}

impl Default for VanillaLayout {
    fn default() -> Self {
        Self { 
            template: VanillaTemplate::default()
        }
    }
}

