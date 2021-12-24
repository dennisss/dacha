extern crate alloc;
extern crate core;

extern crate common;
extern crate json;
extern crate protobuf;
#[macro_use]
extern crate macros;

mod parser;
mod proto;
mod serializer;

pub use parser::*;
pub use serializer::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::test::TestMessage;
    use common::errors::*;

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

        let serialized = m.serialize_json();

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
