use blandwork::{Context, Template};
use maud::{html, Markup, DOCTYPE};

use crate::navigator::Navigator;

/// Defines the root frame for rendering components
#[derive(Clone)]
pub struct VanillaTemplate {
    pub navigator: Navigator
}

impl Default for VanillaTemplate {
    fn default() -> Self {
        Self { navigator: Navigator::new() }
    }
}

impl VanillaTemplate{
    fn head(&self, context: &Context) -> Markup {
        html! {
            head {
                meta charset="utf-8" name="viewport" content="width=device-width, initial-scale=1.0";

                link
                    rel="stylesheet"
                    href="/web/dist/output.css" {}

                // For now use the CDN and load everything. 
                // Optimize for performance later..
                // script src="https://cdn.tailwindcss.com" { }          
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
                body {
                    
                    div #root class="h-lvh bg-pink-500 lg:bg-green-500 md:bg-red-500 p-4" {

                        div #navigator
                            class="flex flex-col items-center justify-start p-2"
                            hx-boost="true"
                            hx-target="#content"
                            hx-swap="innerHTML" {
                            (self.navigator.render(context))
                        }

                        div #content 
                            class="flex flex-col justify-start w-full" 
                            hx-boost="true" {

                            header 
                                class="flex flex-col items-center" {
                                h1 {
                                    "HTML 5 Header with h1"
                                }
                                p {
                                    "and a paragraph of sorts"
                                }
                            }
    
                            nav 
                                class="flex flex-col items-center" {
                                h2 {
                                    "this is an h2"
                                }
                                h3 {
                                    "this is an h3"
                                }
                                h4 {
                                    "this is an h4"
                                }
                                h5 {
                                    "this is an h5"
                                }
                                ul {
                                    li {
                                        a {
                                            "deep nested link item 1"
                                        }
                                    }
                                    li {
                                        a {
                                            "deep nested link item 2"
                                        }
                                    }
                                }
                            }
                            section class="flex flex-col items-center" {
                                button {
                                    "standard button"
                                }

                                button class="btn-primary" {
                                    "primary button"
                                }

                                button class="btn-secondary" {
                                    "secondary button"
                                }
                            }

                            (body)
                        }
                    }
                }

                script src="/web/htmx_integration.js" {}
            }
        }
    }
}