

/*
Ordered list of name-value paris (can have duplicates)

NOTE: Separate state for request and resposne


A change in the maximum size of the dynamic table is signaled via a
   dynamic table size update (see Section 6.3).  This dynamic table size
   update MUST occur at the beginning of the first header block
   following the change to the dynamic table size.  In HTTP/2, this
   follows a settings acknowledgment (see Section 6.5.3 of [HTTP2]).

Dynamic Table is FIFO (oldest has higher index)
- May contain duplicate entries.
*/

/*
    What's the minium full buffer size?
    - N 
*/

// Max size is obtained from SETTINGS_HEADER_TABLE_SIZE

// Read many HeaderFieldRepresentations from the input data.
// Occasionally we may get changes to the max size of the dynamic table (usually )



use common::errors::*;

use crate::hpack::dynamic_table::DynamicTable;
use crate::hpack::indexing_tables::lookup_header_by_index;
use crate::hpack::header_field::*;


pub struct Decoder {
    dynamic_table: DynamicTable,
    protocol_max_size: usize
}

impl Decoder {
    pub fn new(protocol_max_size: usize) -> Self {
        Self {
            dynamic_table: DynamicTable::new(protocol_max_size),
            protocol_max_size
        }
    }

    // TODO: Dedup with the encoder implementation of this function.
    pub fn set_protocol_max_size(&mut self, protocol_max_size: usize) {
        // If the protocol specifies a new max size that is smaller than the current size of the dynamic table
        // we'll shunk it.
        self.dynamic_table.resize(std::cmp::min(protocol_max_size, self.dynamic_table.max_size()));
    }

    pub fn iter<'a, 'b>(&'a mut self, data: &'b [u8]) -> DecoderIterator<'a, 'b> {
        DecoderIterator { decoder: self, input: data }
    }

    pub fn parse_all(&mut self, data: &[u8]) -> Result<Vec<HeaderField>> {
        let mut out = vec![];
        for h in self.iter(data) {
            out.push(h?);
        }

        Ok(out)
    }

    fn resolve_string_ref(&self, r: StringReference, is_value: bool) -> Option<Vec<u8>> {
        match r {
            StringReference::Indexed(i) => {
                // TODO: When retrieving then name, use the same lookup method for the 
                lookup_header_by_index(i, &self.dynamic_table).map(|h| {
                    if is_value { h.value.to_owned() } else { h.name.to_owned() }
                })
            }
            StringReference::Literal(value) => {
                Some(value)
            }
            StringReference::LiteralRef(value) => {
                Some(value.to_owned())
            }
        }

    }
}

pub struct DecoderIterator<'a, 'b> {
    decoder: &'a mut Decoder,
    input: &'b [u8]
}

impl DecoderIterator<'_, '_> {
    fn decode_next(&mut self) -> Result<Option<HeaderField>> {
        while !self.input.is_empty() {
            let (repr, rest) = HeaderFieldRepresentation::parse(self.input)?;
            self.input = rest;

            match repr {
                HeaderFieldRepresentation::HeaderField { name, value, indexed } => {
                    let name_data = self.decoder.resolve_string_ref(name, false)
                        .ok_or_else(|| err_msg("Failed to find name"))?;
                    let value_data = self.decoder.resolve_string_ref(value, true)
                        .ok_or_else(|| err_msg("Failed to find value data"))?;
                    
                    let header = HeaderField { name: name_data, value: value_data };

                    if indexed == IndexingMode::Yes {
                        self.decoder.dynamic_table.insert(header.clone());
                    }
                    
                    return Ok(Some(header));
                }
                HeaderFieldRepresentation::DynamicTableSizeUpdate { max_size } => {
                    if max_size > self.decoder.protocol_max_size {
                        return Err(err_msg("Dynamic size update > protocol max size"));
                    }

                    self.decoder.dynamic_table.resize(max_size);
                }
            }
        }

        Ok(None)
    }
}

impl<'a, 'b> std::iter::Iterator for DecoderIterator<'a, 'b> {
    // TODO: We should also propagate out the 'indexed' field in case this is being forwarded.
    // TODO: Instead return referenced values?
    type Item = Result<HeaderField>;

    fn next(&mut self) -> Option<Self::Item> {
        // Result<Option<..>>  =>  Option<Result<..>>
        match self.decode_next() {
            Ok(v) => v.map(|h| Ok(h)),
            Err(e) => Some(Err(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::hex;

    #[test]
    fn hpack_decoder_example1() -> Result<()> {
        // RFC 7541: Appendix C.2.1
        let example1 = hex::decode(
            "400a637573746f6d2d6b65790d637573746f6d2d686561646572").unwrap();
        
        let mut decoder = Decoder::new(512);

        let mut headers = decoder.parse_all(&example1)?;

        assert_eq!(&headers, &[
            HeaderField { name: b"custom-key".to_vec(), value: b"custom-header".to_vec() }
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 55);

        Ok(())
    }

    #[test]
    fn hpack_decoder_example2() -> Result<()> {
        // RFC 7541: Appendix C.2.2
        let example1 = hex::decode(
            "040c2f73616d706c652f70617468").unwrap();
        
        let mut decoder = Decoder::new(512);

        let mut headers = decoder.parse_all(&example1)?;

        assert_eq!(&headers, &[
            HeaderField { name: b":path".to_vec(), value: b"/sample/path".to_vec() }
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 0);

        Ok(())
    }

    #[test]
    fn hpack_decoder_example3() -> Result<()> {
        // RFC 7541: Appendix C.2.3
        let example1 = hex::decode(
            "100870617373776f726406736563726574").unwrap();
        
        let mut decoder = Decoder::new(512);

        let headers = decoder.parse_all(&example1)?;
        assert_eq!(&headers, &[
            HeaderField { name: b"password".to_vec(), value: b"secret".to_vec() }
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 0);

        Ok(())
    }

    #[test]
    fn hpack_decoder_example4() -> Result<()> {
        // RFC 7541: Appendix C.2.4
        let example1 = hex::decode(
            "82").unwrap();
        
        let mut decoder = Decoder::new(512);

        let headers = decoder.parse_all(&example1)?;
        assert_eq!(&headers, &[
            HeaderField { name: b":method".to_vec(), value: b"GET".to_vec() }
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 0);

        Ok(())
    }

    #[test]
    fn hpack_decoder_example5() -> Result<()> {
        // RFC 7541: Appendix C.3
        let example1 = hex::decode(
            "828684410f7777772e6578616d706c652e636f6d").unwrap();
        let example2 = hex::decode(
            "828684be58086e6f2d6361636865").unwrap();
        let example3 = hex::decode(
            "828785bf400a637573746f6d2d6b65790c637573746f6d2d76616c7565").unwrap();

        let mut decoder = Decoder::new(512);

        let headers = decoder.parse_all(&example1)?;
        assert_eq!(&headers, &[
            HeaderField { name: b":method".to_vec(), value: b"GET".to_vec() },
            HeaderField { name: b":scheme".to_vec(), value: b"http".to_vec() },
            HeaderField { name: b":path".to_vec(), value: b"/".to_vec() },
            HeaderField { name: b":authority".to_vec(), value: b"www.example.com".to_vec() },
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 57);

        let headers = decoder.parse_all(&example2)?;
        assert_eq!(&headers, &[
            HeaderField { name: b":method".to_vec(), value: b"GET".to_vec() },
            HeaderField { name: b":scheme".to_vec(), value: b"http".to_vec() },
            HeaderField { name: b":path".to_vec(), value: b"/".to_vec() },
            HeaderField { name: b":authority".to_vec(), value: b"www.example.com".to_vec() },
            HeaderField { name: b"cache-control".to_vec(), value: b"no-cache".to_vec() },
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 110);

        let headers = decoder.parse_all(&example3)?;
        assert_eq!(&headers, &[
            HeaderField { name: b":method".to_vec(), value: b"GET".to_vec() },
            HeaderField { name: b":scheme".to_vec(), value: b"https".to_vec() },
            HeaderField { name: b":path".to_vec(), value: b"/index.html".to_vec() },
            HeaderField { name: b":authority".to_vec(), value: b"www.example.com".to_vec() },
            HeaderField { name: b"custom-key".to_vec(), value: b"custom-value".to_vec() },
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 164);

        Ok(())
    }

    #[test]
    fn hpack_decoder_example6() -> Result<()> {
        // RFC 7541: Appendix C.4
        let example1 = hex::decode(
            "828684418cf1e3c2e5f23a6ba0ab90f4ff").unwrap();
        let example2 = hex::decode(
            "828684be5886a8eb10649cbf").unwrap();
        let example3 = hex::decode(
            "828785bf408825a849e95ba97d7f8925a849e95bb8e8b4bf").unwrap();
        
        let mut decoder = Decoder::new(512);

        let headers = decoder.parse_all(&example1)?;
        assert_eq!(&headers, &[
            HeaderField { name: b":method".to_vec(), value: b"GET".to_vec() },
            HeaderField { name: b":scheme".to_vec(), value: b"http".to_vec() },
            HeaderField { name: b":path".to_vec(), value: b"/".to_vec() },
            HeaderField { name: b":authority".to_vec(), value: b"www.example.com".to_vec() },
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 57);

        let headers = decoder.parse_all(&example2)?;
        assert_eq!(&headers, &[
            HeaderField { name: b":method".to_vec(), value: b"GET".to_vec() },
            HeaderField { name: b":scheme".to_vec(), value: b"http".to_vec() },
            HeaderField { name: b":path".to_vec(), value: b"/".to_vec() },
            HeaderField { name: b":authority".to_vec(), value: b"www.example.com".to_vec() },
            HeaderField { name: b"cache-control".to_vec(), value: b"no-cache".to_vec() },
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 110);

        let headers = decoder.parse_all(&example3)?;
        assert_eq!(&headers, &[
            HeaderField { name: b":method".to_vec(), value: b"GET".to_vec() },
            HeaderField { name: b":scheme".to_vec(), value: b"https".to_vec() },
            HeaderField { name: b":path".to_vec(), value: b"/index.html".to_vec() },
            HeaderField { name: b":authority".to_vec(), value: b"www.example.com".to_vec() },
            HeaderField { name: b"custom-key".to_vec(), value: b"custom-value".to_vec() },
        ]);
        assert_eq!(decoder.dynamic_table.current_size(), 164);

        Ok(())
    }

    #[test]
    fn hpack_decoder_example7() -> Result<()> {
        // RFC 7541: Appendix C.5
        let example1 = hex::decode(
            "4803333032580770726976617465611d4d6f6e2c203231204f637420323031332032303a31333a323120474d546e1768747470733a2f2f7777772e6578616d706c652e636f6d").unwrap();
        let example2 = hex::decode(
            "4803333037c1c0bf").unwrap();
        let example3 = hex::decode(
            "88c1611d4d6f6e2c203231204f637420323031332032303a31333a323220474d54c05a04677a69707738666f6f3d4153444a4b48514b425a584f5157454f50495541585157454f49553b206d61782d6167653d333630303b2076657273696f6e3d31").unwrap();
        
        let mut decoder = Decoder::new(256);

        let headers = decoder.parse_all(&example1)?;
        assert_eq!(decoder.dynamic_table.current_size(), 222);
        assert_eq!(&headers, &[
            HeaderField { name: b":status".to_vec(), value: b"302".to_vec() },
            HeaderField { name: b"cache-control".to_vec(), value: b"private".to_vec() },
            HeaderField { name: b"date".to_vec(), value: b"Mon, 21 Oct 2013 20:13:21 GMT".to_vec() },
            HeaderField { name: b"location".to_vec(), value: b"https://www.example.com".to_vec() },
        ]);
        

        let headers = decoder.parse_all(&example2)?;
        assert_eq!(decoder.dynamic_table.current_size(), 222);
        assert_eq!(&headers, &[
            HeaderField { name: b":status".to_vec(), value: b"307".to_vec() },
            HeaderField { name: b"cache-control".to_vec(), value: b"private".to_vec() },
            HeaderField { name: b"date".to_vec(), value: b"Mon, 21 Oct 2013 20:13:21 GMT".to_vec() },
            HeaderField { name: b"location".to_vec(), value: b"https://www.example.com".to_vec() },
        ]);
        

        let headers = decoder.parse_all(&example3)?;
        assert_eq!(decoder.dynamic_table.current_size(), 215);
        assert_eq!(&headers, &[
            HeaderField { name: b":status".to_vec(), value: b"200".to_vec() },
            HeaderField { name: b"cache-control".to_vec(), value: b"private".to_vec() },
            HeaderField { name: b"date".to_vec(), value: b"Mon, 21 Oct 2013 20:13:22 GMT".to_vec() },
            HeaderField { name: b"location".to_vec(), value: b"https://www.example.com".to_vec() },
            HeaderField { name: b"content-encoding".to_vec(), value: b"gzip".to_vec() },
            HeaderField { name: b"set-cookie".to_vec(), value: b"foo=ASDJKHQKBZXOQWEOPIUAXQWEOIU; max-age=3600; version=1".to_vec() },
        ]);
        
        Ok(())
    }

    #[test]
    fn hpack_decoder_example8() -> Result<()> {
        // RFC 7541: Appendix C.6
        let example1 = hex::decode(
            "488264025885aec3771a4b6196d07abe941054d444a8200595040b8166e082a62d1bff6e919d29ad171863c78f0b97c8e9ae82ae43d3").unwrap();
        let example2 = hex::decode(
            "4883640effc1c0bf").unwrap();
        let example3 = hex::decode(
            "88c16196d07abe941054d444a8200595040b8166e084a62d1bffc05a839bd9ab77ad94e7821dd7f2e6c7b335dfdfcd5b3960d5af27087f3672c1ab270fb5291f9587316065c003ed4ee5b1063d5007").unwrap();
        
        let mut decoder = Decoder::new(256);

        let headers = decoder.parse_all(&example1)?;
        assert_eq!(decoder.dynamic_table.current_size(), 222);
        assert_eq!(&headers, &[
            HeaderField { name: b":status".to_vec(), value: b"302".to_vec() },
            HeaderField { name: b"cache-control".to_vec(), value: b"private".to_vec() },
            HeaderField { name: b"date".to_vec(), value: b"Mon, 21 Oct 2013 20:13:21 GMT".to_vec() },
            HeaderField { name: b"location".to_vec(), value: b"https://www.example.com".to_vec() },
        ]);
        

        let headers = decoder.parse_all(&example2)?;
        assert_eq!(decoder.dynamic_table.current_size(), 222);
        assert_eq!(&headers, &[
            HeaderField { name: b":status".to_vec(), value: b"307".to_vec() },
            HeaderField { name: b"cache-control".to_vec(), value: b"private".to_vec() },
            HeaderField { name: b"date".to_vec(), value: b"Mon, 21 Oct 2013 20:13:21 GMT".to_vec() },
            HeaderField { name: b"location".to_vec(), value: b"https://www.example.com".to_vec() },
        ]);
        

        let headers = decoder.parse_all(&example3)?;
        assert_eq!(decoder.dynamic_table.current_size(), 215);
        assert_eq!(&headers, &[
            HeaderField { name: b":status".to_vec(), value: b"200".to_vec() },
            HeaderField { name: b"cache-control".to_vec(), value: b"private".to_vec() },
            HeaderField { name: b"date".to_vec(), value: b"Mon, 21 Oct 2013 20:13:22 GMT".to_vec() },
            HeaderField { name: b"location".to_vec(), value: b"https://www.example.com".to_vec() },
            HeaderField { name: b"content-encoding".to_vec(), value: b"gzip".to_vec() },
            HeaderField { name: b"set-cookie".to_vec(), value: b"foo=ASDJKHQKBZXOQWEOPIUAXQWEOIU; max-age=3600; version=1".to_vec() },
        ]);
        
        Ok(())
    }
}

