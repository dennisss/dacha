use common::errors::*;

use crate::dns::name::*;
use crate::dns::proto;

pub struct MessageBuilder {
    data: Vec<u8>,
    header: proto::Header,
    name_encoder: NameEncoder,
}

impl MessageBuilder {
    fn new(header: proto::Header) -> Self {
        MessageBuilder {
            data: vec![0; proto::Header::size_of()],
            header,
            name_encoder: NameEncoder::new(),
        }
    }

    fn append_name(&mut self, name: Name) {
        self.name_encoder.encode(name, &mut self.data);
    }

    fn append_question(&mut self, name: Name, trailer: proto::QuestionTrailer) {
        self.append_name(name);
        trailer.serialize(&mut self.data).unwrap();
    }

    fn build(mut self) -> Vec<u8> {
        let mut header = vec![];
        self.header.serialize(&mut header).unwrap();
        self.data[0..header.len()].copy_from_slice(&header);

        self.data
    }
}

pub struct QueryBuilder {
    message_builder: MessageBuilder,
}

impl QueryBuilder {
    pub fn new(id: u16) -> Self {
        Self {
            message_builder: MessageBuilder::new(proto::Header {
                id,
                flags: proto::Flags {
                    reply: false,
                    opcode: proto::OpCode::Query,
                    authoritive_answer: false,
                    truncated: false,
                    recursion_desired: false,
                    recursion_available: false,
                    zero: 0,
                    response_code: proto::ResponseCode::NoError,
                },
                num_questions: 0,
                num_answers: 0,
                num_authority_records: 0,
                num_additional_records: 0,
            }),
        }
    }

    /// NOTE: Generally you should only have 1 question per query message.
    pub fn add_question(
        &mut self,
        name: Name,
        typ: proto::RecordType,
        class: proto::Class,
        unicast_response: bool,
    ) {
        self.message_builder.header.num_questions += 1;
        self.message_builder.append_question(
            name,
            proto::QuestionTrailer {
                typ,
                unicast_response,
                class,
            },
        );
    }

    pub fn build(self) -> Vec<u8> {
        self.message_builder.build()
    }
}

pub struct ReplyBuilder {
    message_builder: MessageBuilder,
}

impl ReplyBuilder {}
