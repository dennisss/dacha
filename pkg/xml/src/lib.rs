pub mod spec;
mod syntax;

use common::errors::*;
pub use spec::*;

pub fn parse(input: &str) -> Result<Document> {
    let (doc, _) = parsing::complete(syntax::parse_document)(input)?;
    Ok(doc)
}