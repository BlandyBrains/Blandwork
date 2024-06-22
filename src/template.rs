use maud::{html, Markup, DOCTYPE};

use crate::{Component, Context};

/// Defines the root frame for rendering components
pub trait Template : Clone + Send {
    fn page(&self, context: &Context, body: Markup) -> Markup;
}

#[derive(Clone)]
pub struct VanillaTemplate {}

impl VanillaTemplate {
    pub fn new() -> Self {
        Self{}
    }
    fn head(&self, context: &Context) -> Markup {
        html! {
            head {
                meta charset="utf-8" name="viewport" content="width=device-width, initial-scale=1.0";

                // For now use the CDN and load everything. 
                // Optimize for performance later..
                script src="https://cdn.tailwindcss.com" { }          
                script src="https://unpkg.com/htmx.org@1.9.9" {}
                
                title {
                    (context.title())
                }
            }
        }
    }
}

impl Template for VanillaTemplate {
    fn page(&self, context: &Context, body: Markup) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" {
                // <head>
                (self.head(context))

                // <body>
                body 
                    class="w-full h-full m-0 p-0 font-sans" {
                    
                    div #root class="flex h-lvh max-w-7xl mx-auto" {

                        div #navigator
                            class="flex flex-col items-center justify-start p-2 bg-gray-800"
                            hx-boost="true"
                            hx-target="#content"
                            hx-swap="innerHTML" {
                                (context.navigator.render(context))
                            }

                        div #content 
                            hx-boost="true"
                            class="flex flex-row justify-start w-full bg-white" {
                            (body)
                        }
                    }
                }

                script src="/web/htmx_integration.js" {}
            }
        }
    }
}