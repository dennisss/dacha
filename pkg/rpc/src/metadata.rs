use std::collections::HashMap;
use std::iter::Iterator;

use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::AsciiString;

// Comma separation pattern used for splitting received metadata values.
regexp!(COMMA_SEPARATOR => "(?: \t)*,(?: \t)");

#[derive(Debug, Default, Clone)]
pub struct Metadata {
    // Raw values for the metadata as encoded in the headers.
    raw_data: HashMap<AsciiString, Vec<AsciiString>>,
}

impl Metadata {
    pub fn new() -> Self {
        Self {
            raw_data: HashMap::new(),
        }
    }

    fn is_reserved_header(header: &http::Header) -> bool {
        /*
        The list of reserved system headers can be found in https://github.com/grpc/grpc-go/blob/ba41bbac225e6e1a9b822fe636c40c3b7d977894/internal/transport/http_util.go#L101
        isReservedHeader

        "content-type",
        "user-agent",
        "grpc-message-type",
        "grpc-encoding",
        "grpc-message",
        "grpc-status",
        "grpc-timeout",
        "grpc-status-details-bin",
        "te"
        */

        header.is_transport_level()
            || header.is_transport_level()
            || header.name.as_str().starts_with("grpc-")
    }

    pub fn from_headers(headers: &http::Headers) -> Result<Self> {
        let mut out = Self::new();

        // NOTE: In gRPC the http header values will always be ASCII strings.

        for header in &headers.raw_headers {
            if Self::is_reserved_header(header) {
                continue;
            }

            let name = header.name.clone();
            let value = AsciiString::from(header.value.to_bytes())?;

            // NOTE: HTTP2 gurantees that all header names are lowercase so we don't have to
            // worry about normalizing the keys.
            let values_entry = out.get_values_mut(name);

            for value_part in COMMA_SEPARATOR.split(value.as_str()) {
                // TODO: Optimize this given that this should never fail.
                values_entry.push(AsciiString::from(value_part)?);
            }
        }

        Ok(out)
    }

    pub fn append_to_headers(&self, headers: &mut http::Headers) -> Result<()> {
        // TODO: Ideally make this use a deterministic order.
        for (name, values) in self.raw_data.iter() {
            for value in values {
                let header = http::Header {
                    name: name.clone(),
                    value: value.to_bytes().into(),
                };

                if Self::is_reserved_header(&header) {
                    return Err(err_msg("Metadata specified with reserved name"));
                }

                headers.raw_headers.push(header);
            }
        }

        Ok(())
    }

    fn get_values<'a>(&'a self, name: &AsciiString) -> &'a [AsciiString] {
        self.raw_data
            .get(name)
            .map(|v| &v[..])
            .unwrap_or_else(|| &[])
    }

    fn get_values_mut(&mut self, name: AsciiString) -> &mut Vec<AsciiString> {
        self.raw_data.entry(name).or_insert_with(|| vec![])
    }

    pub fn add_text(&mut self, name: &str, value: &str) -> Result<()> {
        if name.ends_with("-bin") {
            return Err(err_msg("Text metadata must not end with -bin"));
        }

        let name = AsciiString::from(name)?;
        self.get_values_mut(name).push(AsciiString::from(value)?);
        Ok(())
    }

    pub fn add_binary(&mut self, name: &str, value: &[u8]) -> Result<()> {
        if !name.ends_with("-bin") {
            return Err(err_msg("Binary metadata must end with -bin"));
        }

        let name = AsciiString::from(name)?;
        let value = common::base64::encode_config(value, common::base64::STANDARD_NO_PAD);

        self.get_values_mut(name).push(AsciiString::from(value)?);
        Ok(())
    }

    pub fn get_text(&self, name: &str) -> Result<Option<&str>> {
        let mut iter = self.iter_text(name)?;
        let value = iter.next();
        if iter.next().is_some() {
            return Err(format_err!("More than one value named: {}", name));
        }

        Ok(value)
    }

    pub fn iter_text(&self, name: &str) -> Result<impl Iterator<Item = &str>> {
        if name.ends_with("-bin") {
            return Err(err_msg("Text metadata must not end with -bin"));
        }

        let name = AsciiString::from(name)?;

        let values = self.get_values(&name);

        Ok(values.iter().map(|v| v.as_str()))
    }

    pub fn get_binary(&self, name: &str) -> Result<Vec<u8>> {
        let mut iter = self.iter_binary(name)?;
        let value = iter
            .next()
            .ok_or_else(|| format_err!("No metadata named: {}", name))??;
        if iter.next().is_some() {
            return Err(format_err!("More than one value named: {}", name));
        }

        Ok(value)
    }

    pub fn iter_binary<'a>(
        &'a self,
        name: &str,
    ) -> Result<impl Iterator<Item = Result<Vec<u8>>> + 'a> {
        // TODO: Verify this accepts both padded and non-padded base64

        if !name.ends_with("-bin") {
            return Err(err_msg("Binary metadata must end with -bin"));
        }

        let name = AsciiString::from(name)?;

        let values = self.get_values(&name);

        Ok(values.iter().map(|v| {
            common::base64::decode_config(v.as_str().as_bytes(), common::base64::STANDARD_NO_PAD)
                .map_err(|e| Error::from(e))
        }))
    }
}

///
#[derive(Default)]
pub struct ResponseMetadata {
    pub head_metadata: Metadata,
    pub trailer_metadata: Metadata,
}
