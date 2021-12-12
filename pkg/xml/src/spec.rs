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
}

#[derive(Debug, PartialEq)]
pub enum Node {
    Text(String),
    Element(Element),
    Comment(String),
}
