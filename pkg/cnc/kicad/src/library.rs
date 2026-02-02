use common::errors::*;

#[derive(Debug, Clone, Parseable)]
pub struct LibraryTable {
    // Only present in older versions of Kicad
    pub version: Option<usize>,
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

