use maud::Markup;

use crate::Context;

/// Defines the root frame for rendering components
pub trait Template {
    fn page(&self, context: &Context, body: Markup) -> Markup;
}
