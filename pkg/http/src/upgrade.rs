// Code related to the 'Upgrade' HTTP header.

use parsing::ascii::AsciiString;

#[derive(Debug)]
pub struct Protocol {
    pub name: AsciiString,
    pub version: Option<AsciiString>,
}