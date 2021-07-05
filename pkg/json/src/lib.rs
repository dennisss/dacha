#[macro_use] extern crate common;
extern crate parsing;
extern crate automata;

#[macro_use] extern crate regexp_macros;

mod parser;
mod value;
mod stringifier;

use common::errors::*;

pub use value::Value;
pub use stringifier::*;

pub fn parse(input: &str) -> Result<Value> {
    let (v, _) = parsing::complete(parser::parse_json)(input)?;
    Ok(v)
}

pub fn stringify(value: &Value) -> String {
    Stringifier::run(value, StringifyOptions::default())
}

pub fn pretty_stringify(value: &Value) -> String {
    let options = StringifyOptions {
        indent: Some(String::from("    ")),
        space_after_colon: true
    };

    Stringifier::run(value, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_test() -> Result<()> {

        assert_eq!(parse("null")?, Value::Null);
        assert_eq!(parse("123")?, Value::Number(123.0));
        assert_eq!(parse("true")?, Value::Bool(true));

        assert_eq!(parse(" [1 , 2,3]")?, Value::Array(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ]));

        assert_eq!(parse("\"hello\"")?, Value::String("hello".into()));

        parse(r#"{"hello":"world"}"#)?;

        let v = parse(r#"
            {
                "hello": "world",
                "testing": 123,
                "list" : [
                    1, 2, true, false, null, "hi",
                    [11],
                    { }
                ]
            }
        "#)?;

        Ok(())
    }

    #[test]
    fn stringify_test() {
        assert_eq!(stringify(&Value::Null), "null");
        assert_eq!(stringify(&Value::Number(456.1)), "456.1");

        let obj1 = Value::Object(map!(
            "hello" => Value::Array(vec![ Value::Number(1.0), Value::Number(2.0), Value::String("3".into()) ])
        ));
        assert_eq!(stringify(&obj1), r#"{"hello":[1,2,"3"]}"#);
        assert_eq!(pretty_stringify(&obj1),
r#"{
    "hello": [
        1,
        2,
        "3"
    ]
}"#);
    }


}