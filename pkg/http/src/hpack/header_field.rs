use parsing::binary::be_u8;

pub struct HeaderField {
    pub name: Vec<u8>,
    pub value: Vec<u8>
}

enum HeaderFieldRepresentation {
    /// RFC 7541: Section 6.1
    HeaderField {
        name: StringReference,
        value: StringReference,
        indexed: bool
    },

    DynamicTableSizeUpdate {
        size: u64
    }
}

impl HeaderFieldRepresentation {
    // pub fn parse(mut input: &[u8]) -> Result<(Self, &[u8])> {
    //     let (first_byte, _) = be_u8(input)?;

    //     match first_byte.leading_zeros() {
    //         // Starts with '1'
    //         // RFC 7531: Section 6.1
    //         0 => {
    //             // let index = 
    //         }
    //     }

    //     // Read first byte.
    //     // Count leading zeros.

    // }
}


enum StringReference {
    Indexed(u64),
    Literal(Vec<u8>)
}

// '1'    : Indexed Header Field
// '01'   : Index Name + Lit Value
// '01000000' : Lit Name + Lit Value
// '0000' : Indexed Name + Lit Name (non-indexed)
// '00000000' : Lit Name + Lit Value (non-indexed)
// '0001'     : Index Name + Lit Value (never-indexed)
// '00010000' : Lit Name + Lit Value (never-indexed)
// '001'