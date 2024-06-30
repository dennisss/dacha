extern crate alloc;
extern crate core;

extern crate common;
extern crate json;
extern crate protobuf;
#[macro_use]
extern crate macros;

mod parser;
mod serializer;

pub use parser::*;
pub use serializer::*;

// TODO: Move this to the protobuf_test crate so that we don't need to link to
// the compiler.
#[cfg(test)]
mod tests {
    use super::*;
    use common::errors::*;
    use protobuf_json_proto::TestMessage;

    #[test]
    fn empty_json_test() -> Result<()> {
        let mut m = TestMessage::default();

        let serialized = m.serialize_json(&SerializerOptions::default())?;

        assert_eq!(r#"{}"#, serialized);

        Ok(())
    }

    #[test]
    fn one_field_set_test() -> Result<()> {
        let mut m = TestMessage::default();
        m.set_integer(10);

        let serialized = m.serialize_json(&SerializerOptions::default())?;

        assert_eq!(r#"{"integer":10}"#, serialized);

        Ok(())
    }

    #[test]
    fn json_ser_deser_test() -> Result<()> {
        let mut m = TestMessage::default();
        m.set_integer(123);
        m.set_flag(true);
        m.add_ids(10);
        m.add_ids(20);
        m.add_ids(30);
        m.set_s("Hello world");
        m.set_data(vec![0, 0, 1, 0, 0]);

        let serialized = m.serialize_json(&SerializerOptions::default())?;

        assert_eq!(
            r#"{"integer":123,"flag":true,"data":"AAABAAA","ids":[10,20,30],"s":"Hello world"}"#,
            serialized
        );

        let decoded = TestMessage::parse_json(&serialized, &ParserOptions::default())?;

        assert_eq!(decoded.integer(), 123);
        assert_eq!(decoded.flag(), true);
        assert_eq!(decoded.data().as_ref(), &[0, 0, 1, 0, 0]);
        assert_eq!(decoded.ids(), &[10, 20, 30]);
        assert_eq!(decoded.s(), "Hello world");

        Ok(())
    }
}
