use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub struct Document {
    pub encoding: String,
    pub standalone: bool,
    pub root_element: Element
}

#[derive(Debug, PartialEq)]
pub struct Element {
    pub name: String,
    pub attributes: HashMap<String, String>,
    pub content: Vec<Node>
}

#[derive(Debug, PartialEq)]
pub enum Node {
    Text(String),
    Element(Element),
    Comment(String)
}