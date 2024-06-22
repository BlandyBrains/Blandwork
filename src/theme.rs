pub struct Color {
    // maps to css class name
    name: String
}

impl Color {
    pub fn new(name: &str) -> Self{
        Self { name: name.to_string() }
    }
    pub fn light(&self) -> String {
        format!("{}-{}", self.name, "light")
    }
    pub fn default(&self) -> String {
        format!("{}-{}", self.name, "light")
    }
    pub fn dark(&self) -> String {
        format!("{}-{}", self.name, "light")
    }
}

pub trait Theme {
    fn primary(&self) -> Color {
        Color::new("primary")
    }
    fn secondary(&self) -> Color {
        Color::new("seconary")
    }
    fn accent(&self) -> Color {
        Color::new("accent")
    }
    fn shade(&self) -> Color {
        Color::new("shade")
    }
    fn success(&self) -> Color {
        Color::new("success")
    }
    fn warning(&self) -> Color {
        Color::new("warning")
    }
    fn error(&self) -> Color {
        Color::new("error")
    }
    fn highlight(&self) -> Color {
        Color::new("highlight")
    }
    fn active(&self) -> Color {
        Color::new("active")
    }
    fn background(&self) -> Color {
        Color::new("background")
    }
}
