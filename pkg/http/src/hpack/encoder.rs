use crate::hpack::dynamic_table::DynamicTable;
use crate::hpack::header_field::*;
use crate::hpack::header_field::IndexingMode;
use crate::hpack::indexing_tables::search_for_header;


pub struct Encoder {

    /// Maximum size of the dynamic table as specified by the protocol (e.g. HTTP2)
    protocol_max_size: usize,

    dynamic_table: DynamicTable
}

impl Encoder {
    pub fn new(protocol_max_size: usize) -> Self {
        Self { protocol_max_size, dynamic_table: DynamicTable::new(protocol_max_size) }
    }

    pub fn set_protocol_max_size(&mut self, protocol_max_size: usize) {
        // If the protocol specifies a new max size that is smaller than the current size of the dynamic table
        // we'll shunk it.
        self.dynamic_table.resize(std::cmp::min(protocol_max_size, self.dynamic_table.max_size()));
    }

    /// NOTE: We assume that new_max_size is smaller than the protocol_max_size.
    pub fn append_table_size_update(&mut self, max_size: usize, out: &mut Vec<u8>) {
        self.dynamic_table.resize(max_size);
        
        HeaderFieldRepresentation::DynamicTableSizeUpdate {
            max_size
        }.serialize(out);
    }

    pub fn append<'a>(&mut self, header: HeaderFieldRef<'a>, out: &mut Vec<u8>) {
        let mut name = StringReference::LiteralRef(header.name);
        let mut value = StringReference::LiteralRef(header.value);
        let mut indexed ;

        // TODO: Instead take an indexing mode hint as an argument.
        let never_index = header.name.eq_ignore_ascii_case(b"Authentication")
            || header.name.eq_ignore_ascii_case(b"Cookie");
        
        if never_index {
            indexed = IndexingMode::Never;
        } else {
            indexed = IndexingMode::Yes;

            let search_result = search_for_header(header, &self.dynamic_table);
            if let Some(result) = search_result {
                name = StringReference::Indexed(result.index);
                if result.value_matches {
                    value = StringReference::Indexed(result.index);

                    // If both the name and value are already in the index, no need to re-index it.
                    // (also this isn't supported in the binary representation)
                    indexed = IndexingMode::No;
                }
            }
        }

        if indexed == IndexingMode::Yes {
            self.dynamic_table.insert(header.to_owned());
        }

        // TODO: Verify that this will never panic.
        HeaderFieldRepresentation::HeaderField {
            name,
            value,
            indexed
        }.serialize(out).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use common::errors::*;
    use super::*;

    #[test]
    fn hpack_encoder_test() -> Result<()> {
        let mut encoder = Encoder::new(512);

        // Lookup from static table.
        let mut out = vec![];
        encoder.append(HeaderFieldRef {
            name: b":status",
            value: b"200"
        }, &mut out);
        assert_eq!(&out, &[0x88]);

        // Literal name + literal value.
        // TODO: Verify the output value using the decoder
        let mut out = vec![];
        encoder.append(HeaderFieldRef {
            name: b"awesome-header",
            value: b"awesome-value"
        }, &mut out);
        assert_eq!(&out, &[64, 138, 31, 130, 160, 244, 149, 105, 202, 57, 11, 103, 138, 31, 130, 160, 244, 149, 110, 227, 162, 210, 255]);

        // Same thing as before. Should be completely indexed.
        let mut out = vec![];
        encoder.append(HeaderFieldRef {
            name: b"awesome-header",
            value: b"awesome-value"
        }, &mut out);
        assert_eq!(&out, &[190]);

        // Indexed name + literal value.
        let mut out = vec![];
        encoder.append(HeaderFieldRef {
            name: b"awesome-header",
            value: b"brand-new-value"
        }, &mut out);
        assert_eq!(&out, &[126, 139, 142, 193, 213, 34, 213, 23, 194, 221, 199, 69, 165]);


        let mut out = vec![];
        encoder.append(HeaderFieldRef {
            name: b"awesome-header",
            value: b"awesome-value"
        }, &mut out);
        assert_eq!(&out, &[191]);

        // println!("{:?}", crate::hpack::header_field::HeaderFieldRepresentation::parse(&out)?.0);

        Ok(())
    }

}