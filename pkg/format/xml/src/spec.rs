use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub struct Document {
    pub encoding: String,
    pub standalone: bool,
    pub root_element: Element,
}

#[derive(Debug, PartialEq)]
pub struct Element {
    pub name: String,
    /// TODO: Define the behavior for duplicate attributes.
    pub attributes: HashMap<String, String>,
    pub content: Vec<Node>,
}

impl Element {
    pub fn children(&self) -> impl Iterator<Item = &Element> {
        self.content.iter().filter_map(|n| {
            if let Node::Element(e) = n {
                Some(e)
            } else {
                None
            }
        })
    }

    pub fn inner_text(&self) -> String {
        let mut out = String::new();
        
        for n in &self.content {
            match n {
                Node::Element(e) => {
                    out.push_str(&e.inner_text());
                }
                Node::Text(s) => {
                    out.push_str(&s);
                }
                Node::Comment(_) => {}
            }
        }

        out
    }
}

#[derive(Debug, PartialEq)]
pub enum Node {
    Text(String),
    Element(Element),
    Comment(String),
}
