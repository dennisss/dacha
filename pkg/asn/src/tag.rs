/// NOTE: Tags have a canonical ordering of Universal, Application, Context,
/// Private, and then in each class it is in order of ascending number.
#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash, Clone)]
pub struct Tag {
    pub class: TagClass,
    pub number: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum TagClass {
    Universal = 0,
    Application = 1,
    ContextSpecific = 2,
    Private = 3,
}

impl TagClass {
    pub fn from(v: u8) -> Self {
        match v {
            0 => TagClass::Universal,
            1 => TagClass::Application,
            2 => TagClass::ContextSpecific,
            3 => TagClass::Private,
            _ => panic!("Value larger than 2 bits"),
        }
    }
}

// Tag numbers in the Universal class.
pub const TAG_NUMBER_BOOLEAN: usize = 1;
pub const TAG_NUMBER_INTEGER: usize = 2;
pub const TAG_NUMBER_BIT_STRING: usize = 3;
pub const TAG_NUMBER_OCTET_STRING: usize = 4;
pub const TAG_NUMBER_NULL: usize = 5;
pub const TAG_NUMBER_OBJECT_IDENTIFIER: usize = 6;
pub const TAG_NUMBER_OBJECT_DESCRIPTOR: usize = 7;
pub const TAG_NUMBER_EXTERNAL: usize = 8;
pub const TAG_NUMBER_REAL: usize = 9;
pub const TAG_NUMBER_ENUMERATED: usize = 10;
pub const TAG_NUMBER_EMBEDDED_PDV: usize = 11;
pub const TAG_NUMBER_UTF8STRING: usize = 12;
pub const TAG_NUMBER_RELATIVE_OID: usize = 13;
pub const TAG_NUMBER_TIME: usize = 14;
pub const TAG_NUMBER_SEQUENCE: usize = 16;
pub const TAG_NUMBER_SET: usize = 17;
pub const TAG_NUMBER_NUMERIC_STRING: usize = 18;
pub const TAG_NUMBER_PRINTABLE_STRING: usize = 19;
pub const TAG_NUMBER_T61STRING: usize = 20;
pub const TAG_NUMBER_VIDEOTEXSTRING: usize = 21;
pub const TAG_NUMBER_IA5STRING: usize = 22;
pub const TAG_NUMBER_UTCTIME: usize = 23;
pub const TAG_NUMBER_GENERALIZEDTIME: usize = 24;
pub const TAG_NUMBER_GRAPHICSTRING: usize = 25;
pub const TAG_NUMBER_VISIBLESTRING: usize = 26;
pub const TAG_NUMBER_GENERALSTRING: usize = 27;
pub const TAG_NUMBER_UNIVERSALSTRING: usize = 28;
pub const TAG_NUMBER_CHARACTER_STRING: usize = 29;
pub const TAG_NUMBER_BMPSTRING: usize = 30;
pub const TAG_NUMBER_DATE: usize = 31;
pub const TAG_NUMBER_TIME_OF_DAY: usize = 32;
pub const TAG_NUMBER_DATE_TIME: usize = 33;
pub const TAG_NUMBER_DURATION: usize = 34;
pub const TAG_NUMBER_OID_IRI: usize = 35;
pub const TAG_NUMBER_RELATIVE_OID_IRI: usize = 36;
