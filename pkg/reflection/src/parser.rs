use common::errors::*;

pub trait ParseFrom<'a> {
    fn parse_from<Input: ValueParser<'a>>(input: Input) -> Result<Self>
    where
        Self: Sized;
}

// pub trait ParserTypes {
//     type ValueParserType;
//     type ObjectParserType;
//     type ListParserType;
// }

/// TODO: All parsers should check that all bytes are consumed?
pub trait ValueParser<'a> {
    type ObjectParserType: ObjectParser<'a>;
    type ListParserType: ListParser<'a>;

    // /// Returns whether or not there are any remaining readable values in this
    // /// input stream.
    // fn is_empty(&self) -> bool;

    /// Reads a single value from the underlying stream and advances forward the
    /// stream by one position.
    ///
    /// TODO: We want to support a value 'hint'
    /// - For objects, may need to support a 'name'
    fn parse(self) -> Result<Value<'a, Self::ObjectParserType, Self::ListParserType>>;
}

pub enum PrimitiveValue<'a> {
    Null,
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    ISize(isize),
    USize(usize),
    F32(f32),
    F64(f64),
    Str(&'a str),
    String(String),
}

pub enum Value<'a, ObjectParserType, ListParserType> {
    Primitive(PrimitiveValue<'a>),
    Object(ObjectParserType),
    List(ListParserType),
}

impl<'a, ObjectParserType, ListParserType> Value<'a, ObjectParserType, ListParserType> {
    pub fn into_object(self) -> Result<ObjectParserType> {
        match self {
            Value::Object(v) => Ok(v),
            _ => Err(err_msg("Not an object")),
        }
    }
}

impl<'a, ObjectParserType: ObjectParser<'a>, ListParserType: ListParser<'a>> ValueParser<'a>
    for Value<'a, ObjectParserType, ListParserType>
{
    type ObjectParserType = ObjectParserType;
    type ListParserType = ListParserType;

    fn parse(self) -> Result<Value<'a, Self::ObjectParserType, Self::ListParserType>> {
        Ok(self)
    }
}

pub trait ObjectParser<'a> {
    type Key: AsRef<str> + 'a;
    type ValueParserType: ValueParser<'a>;

    fn next_field(&mut self) -> Result<Option<(Self::Key, Self::ValueParserType)>>;
}

pub trait ListParser<'a> {
    type ValueParserType: ValueParser<'a>;

    fn next(&mut self) -> Option<Self::ValueParserType>;
}

macro_rules! impl_numeric_parse_from {
    ($t:ty) => {
        impl<'a> ParseFrom<'a> for $t {
            fn parse_from<Input: ValueParser<'a>>(input: Input) -> Result<Self> {
                Ok(match input.parse()? {
                    Value::Primitive(v) => match v {
                        PrimitiveValue::I8(v) => v as $t,
                        PrimitiveValue::U8(v) => v as $t,
                        PrimitiveValue::I16(v) => v as $t,
                        PrimitiveValue::U16(v) => v as $t,
                        PrimitiveValue::I32(v) => v as $t,
                        PrimitiveValue::U32(v) => v as $t,
                        PrimitiveValue::I64(v) => v as $t,
                        PrimitiveValue::U64(v) => v as $t,
                        PrimitiveValue::ISize(v) => v as $t,
                        PrimitiveValue::USize(v) => v as $t,
                        PrimitiveValue::F32(v) => v as $t,
                        PrimitiveValue::F64(v) => v as $t,
                        PrimitiveValue::Null
                        | PrimitiveValue::Bool(_)
                        | PrimitiveValue::Str(_)
                        | PrimitiveValue::String(_) => {
                            return Err(err_msg("Type mismatch"));
                        }
                    },
                    Value::Object(_) | Value::List(_) => {
                        return Err(err_msg("Expected primitive value"));
                    }
                })
            }
        }
    };
}

impl_numeric_parse_from!(i8);
impl_numeric_parse_from!(u8);
impl_numeric_parse_from!(i16);
impl_numeric_parse_from!(u16);
impl_numeric_parse_from!(i32);
impl_numeric_parse_from!(u32);
impl_numeric_parse_from!(i64);
impl_numeric_parse_from!(u64);
impl_numeric_parse_from!(isize);
impl_numeric_parse_from!(usize);

// TODO: The main issue with this is that we can't use it to implement parser
// hints.
impl<'a, T: ParseFrom<'a> + Sized> ParseFrom<'a> for Option<T> {
    fn parse_from<Input: ValueParser<'a>>(input: Input) -> Result<Self> {
        let v = input.parse()?;
        if let Value::Primitive(PrimitiveValue::Null) = v {
            return Ok(None);
        }

        let inner = T::parse_from(v)?;
        Ok(Some(inner))
    }
}

impl<'a> ParseFrom<'a> for String {
    fn parse_from<Input: ValueParser<'a>>(input: Input) -> Result<Self> {
        Ok(match input.parse()? {
            Value::Primitive(PrimitiveValue::String(s)) => s,
            _ => {
                return Err(err_msg("Not a string"));
            }
        })
    }
}

impl<'a, T: ParseFrom<'a> + Sized> ParseFrom<'a> for Vec<T> {
    fn parse_from<Input: ValueParser<'a>>(input: Input) -> Result<Self> {
        let mut out = vec![];
        let mut list = match input.parse()? {
            Value::List(v) => v,
            _ => {
                return Err(err_msg("Not a list"));
            }
        };

        while let Some(i) = list.next() {
            out.push(T::parse_from(i)?);
        }

        Ok(out)
    }
}
