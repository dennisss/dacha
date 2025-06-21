use common::errors::*;

#[derive(Debug, Clone, Parseable)]
pub struct LibraryTable {
    pub version: usize,
    pub lib: xml::List<Library>,
}

#[derive(Debug, Clone, Parseable)]
pub struct Library {
    pub name: String,
    #[parse(name = "type")]
    pub typ: String,
    pub uri: String,
    pub options: String,
    pub descr: String
}

