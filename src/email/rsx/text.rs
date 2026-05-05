pub struct TextBuilder {
    lines: Vec<String>,
}

impl TextBuilder {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn line(mut self, text: &str) -> Self {
        self.lines.push(text.to_string());
        self
    }

    pub fn blank(mut self) -> Self {
        self.lines.push(String::new());
        self
    }

    pub fn optional_line(mut self, label: &str, value: Option<&str>) -> Self {
        if let Some(v) = value {
            self.lines.push(format!("{}: {}", label, v));
        }
        self
    }

    pub fn optional_section(mut self, label: &str, value: Option<&str>) -> Self {
        if let Some(v) = value {
            self.lines.push(String::new());
            self.lines.push(format!("{}: {}", label, v));
        }
        self
    }

    pub fn link(mut self, label: &str, url: &str) -> Self {
        self.lines.push(format!("{}: {}", label, url));
        self
    }

    pub fn optional_unsubscribe(mut self, link: Option<&str>) -> Self {
        if let Some(url) = link {
            self.lines.push(String::new());
            self.lines.push(format!("Unsubscribe from this category: {}", url));
        }
        self
    }

    pub fn build(self) -> String {
        self.lines.join("\n")
    }
}

impl Default for TextBuilder {
    fn default() -> Self {
        Self::new()
    }
}
