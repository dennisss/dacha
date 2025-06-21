use alloc::vec::Vec;
use core::fmt::Debug;

use common::errors::*;
use parsing::take_exact;

use crate::dns::name::*;
use crate::dns::proto::{self, RecordType};
use crate::ip::IPAddress;

#[derive(PartialEq, Debug)]
pub struct Message<'a> {
    header: proto::Header,
    questions: Vec<Question<'a>>,

    /// Concatenated answer, authority, and additional records.
    records: Vec<ResourceRecord<'a>>,
}

impl<'a> Message<'a> {
    pub fn parse(mut input: &'a [u8]) -> Result<(Self, &'a [u8])> {
        let message = input;
        let header = parse_next!(input, proto::Header::parse);

        let mut questions = vec![];
        for i in 0..header.num_questions {
            questions.push(parse_next!(input, Question::parse, message));
        }

        let mut records = vec![];
        for i in
            0..(header.num_answers + header.num_authority_records + header.num_additional_records)
        {
            records.push(parse_next!(input, ResourceRecord::parse, message));
        }

        Ok((
            Self {
                header,
                questions,
                records,
            },
            input,
        ))
    }

    pub fn parse_complete(input: &'a [u8]) -> Result<Self> {
        let (msg, _) = parsing::complete(Self::parse)(input)?;
        Ok(msg)
    }

    pub fn id(&self) -> u16 {
        self.header.id
    }

    pub fn is_reply(&self) -> bool {
        self.header.flags.reply
    }

    pub fn opcode(&self) -> proto::OpCode {
        self.header.flags.opcode
    }

    pub fn response_code(&self) -> proto::ResponseCode {
        self.header.flags.response_code
    }

    pub fn questions(&self) -> &[Question] {
        &self.questions
    }

    pub fn records(&self) -> &[ResourceRecord] {
        &self.records
    }
}

#[derive(PartialEq, Debug)]
pub struct Question<'a> {
    name: Name<'a>,
    pub(super) trailer: proto::QuestionTrailer,
}

impl<'a> Question<'a> {
    pub fn parse(mut input: &'a [u8], message: &'a [u8]) -> Result<(Self, &'a [u8])> {
        let name = parse_next!(input, Name::parse, message);
        let trailer = parse_next!(input, proto::QuestionTrailer::parse);
        Ok((Self { name, trailer }, input))
    }

    pub fn name(&self) -> &Name<'a> {
        &self.name
    }

    pub fn typ(&self) -> RecordType {
        self.trailer.typ
    }

    pub fn class(&self) -> proto::Class {
        self.trailer.class
    }
}

// TODO: PartialEq will not work due to the 'message' reference.
#[derive(PartialEq)]
pub struct ResourceRecord<'a> {
    name: Name<'a>,

    // TODO: Use a reference for the data in this.
    trailer: proto::ResourceRecordTrailer,

    /// The 'rdata' part of the record. 
    data: &'a [u8],

    /// Reference to the complete message. This is just used for resolving compressed name references.
    message: &'a [u8],
}

impl<'a> Debug for ResourceRecord<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResourceRecord")
        .field("name", &self.name)
        .field("type", &self.trailer.typ)
        .field("cache_flush", &self.trailer.cache_flush)
        .field("class", &self.trailer.class)
        .field("ttl", &self.trailer.ttl)
        .field("rdata", &self.data())
        .finish()
    }
}

impl<'a> ResourceRecord<'a> {
    pub fn parse(mut input: &'a [u8], message: &'a [u8]) -> Result<(Self, &'a [u8])> {
        let name = parse_next!(input, Name::parse, message);
        let trailer = parse_next!(input, proto::ResourceRecordTrailer::parse);
        let data = parse_next!(input, take_exact(trailer.data_len as usize));

        Ok((
            Self {
                name,
                trailer,
                data,
                message,
            },
            input,
        ))
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn typ(&self) -> RecordType {
        self.trailer.typ
    }

    pub fn data(&self) -> Result<ResourceRecordData> {
        match self.trailer.typ {
            RecordType::A => {
                if self.data.len() != 4 {
                    return Err(err_msg("IpV4 must be 4 bytes"));
                }

                Ok(ResourceRecordData::Address(IPAddress::V4(*array_ref![
                    self.data,
                    0,
                    4
                ])))
            }
            RecordType::AAAA => {
                if self.data.len() != 16 {
                    return Err(err_msg("IpV6 must be 16 bytes"));
                }

                Ok(ResourceRecordData::Address(IPAddress::V6(*array_ref![
                    self.data,
                    0,
                    16
                ])))
            }
            // _Service._Proto.Name TTL Class SRV Priority Weight Port Target
            // Defined in https://datatracker.ietf.org/doc/html/rfc2782
            RecordType::SRV => {
                let mut input = &self.data[..];
                let header = parse_next!(input, proto::SRVDataHeader::parse);
                let target = parse_next!(input, Name::parse, self.message);
                Ok(ResourceRecordData::Service(SRVRecordData {
                    header,
                    target,
                }))
            }
            RecordType::PTR => {
                let mut input = &self.data[..];
                let name = parse_next!(input, Name::parse, self.message);
                if input.len() != 0 {
                    return Err(err_msg("Extra bytes in PTR record"));
                }

                Ok(ResourceRecordData::Pointer(name))
            }
            RecordType::TXT => {
                let mut input = &self.data[..];

                let mut items = vec![];

                while !input.is_empty() {
                    let len = input[0] as usize;
                    input = &input[1..];

                    if input.len() < len {
                        return Err(err_msg("Invalid TXT record"));
                    }

                    items.push(&input[0..len]);
                    input = &input[len..];
                }

                Ok(ResourceRecordData::Text(items))
            }

            // Want something for
            _ => Ok(ResourceRecordData::Unknown(&self.data)),
        }
    }
}

#[derive(Debug)]
pub enum ResourceRecordData<'a> {
    /// On A and AAAA records
    Address(IPAddress),

    Pointer(Name<'a>),

    Service(SRVRecordData<'a>),

    Text(Vec<&'a [u8]>),

    Unknown(&'a [u8]),
}

impl<'a> ResourceRecordData<'a> {
    pub fn serialize(&self, name_encoder: &mut NameEncoder, out: &mut Vec<u8>) {
        match self {
            Self::Address(addr) => {
                out.extend_from_slice(addr.as_bytes());
            }
            Self::Pointer(name) => {
                name_encoder.encode(name, out);
            }
            Self::Service(_) => {
                // TODO
            }
            Self::Text(_) => {
                // TODO
            }
            Self::Unknown(data) => {
                out.extend_from_slice(data);
            }
        }
    }
}


#[derive(Debug)]
pub struct SRVRecordData<'a> {
    // TODO: Make this private?
    pub header: proto::SRVDataHeader,

    pub target: Name<'a>,
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn parse_reply_test() {
        let data = &[
            0x00, 0x01, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let reply = Message::parse_complete(data).unwrap();

        assert_eq!(
            reply,
            Message {
                header: proto::Header {
                    id: 1,
                    flags: proto::Flags {
                        reply: true,
                        opcode: proto::OpCode::Query,
                        authoritive_answer: false,
                        truncated: false,
                        recursion_desired: false,
                        recursion_available: false,
                        zero: 0,
                        response_code: proto::ResponseCode::FormatError,
                    },
                    num_questions: 0,
                    num_answers: 0,
                    num_authority_records: 0,
                    num_additional_records: 0
                },
                questions: vec![],
                records: vec![]
            }
        );
    }

    #[test]
    fn parse_reply2_test() {
        let data = &[
            0x00, 0x01, 0x80, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x06, 0x67,
            0x6f, 0x6f, 0x67, 0x6c, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x01, 0x00, 0x01,
            0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x29, 0x00, 0x04, 0x8e, 0xfa,
            0xbd, 0xae,
        ];

        let reply = Message::parse(data).unwrap();
        println!("{:?}", reply)
    }
}
