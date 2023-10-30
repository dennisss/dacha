use std::collections::HashMap;

use common::errors::*;

pub trait ParseFrom<'data> {
    fn parse_from<Input: ValueReader<'data>>(input: Input) -> Result<Self>
    where
        Self: Sized;
}

impl<'data, T: ParseFromValue<'data> + Sized> ParseFrom<'data> for T {
    fn parse_from<Input: ValueReader<'data>>(input: Input) -> Result<Self> {
        input.parse()
    }
}

pub trait ParseFromValue<'data> {
    fn parse_merge<Input: ValueReader<'data>>(&mut self, input: Input) -> Result<()> {
        return Err(err_msg("Duplicate value for field"));
    }

    fn parse_from_primitive(value: PrimitiveValue<'data>) -> Result<Self>
    where
        Self: Sized,
    {
        Err(err_msg("Can't be parsed from a primitive value."))
    }

    fn parse_from_object<Input: ObjectIterator<'data>>(input: Input) -> Result<Self>
    where
        Self: Sized,
    {
        Err(format_err!(
            "Can't be parsed from an object: {}",
            std::any::type_name::<Self>()
        ))
    }

    fn parse_from_list<Input: ListIterator<'data>>(input: Input) -> Result<Self>
    where
        Self: Sized,
    {
        Err(err_msg("Can't be parsed from a list."))
    }

    fn parsing_typename() -> Option<&'static str> {
        None
    }

    fn parsing_hint() -> Option<ParsingTypeHint> {
        None
    }

    fn unwrap_parsed_result(name: &str, value: Option<Self>) -> Result<Self>
    where
        Self: Sized,
    {
        match value {
            Some(v) => Ok(v),
            None => Err(format_err!("Missing required value for field: {}", name)),
        }
    }
}

/// Input source of a single list|object|primitive value.
pub trait ValueReader<'data> {
    /// Reads the single value in the underlying stream as type T.
    /// Will consume all the remaining input data in the underylying stream.
    fn parse<T: ParseFromValue<'data>>(self) -> Result<T>;
}

pub enum PrimitiveValue<'data> {
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
    Str(&'data str),
    String(String),
}

#[derive(Debug)]
pub enum ParsingTypeHint {
    // Primitives
    Null,
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    ISize,
    USize,
    F32,
    F64,
    String,

    // Higher level
    Object,
    List,
}

pub trait ObjectIterator<'data> {
    type ValueReaderType: ValueReader<'data>;

    fn next_field(&mut self) -> Result<Option<(String, Self::ValueReaderType)>>;
}

pub trait ObjectBuilder<'data> {
    type ObjectType;

    fn add_field<V: ValueReader<'data>>(
        &mut self,
        key: String,
        value: V,
    ) -> Result<Option<(String, V)>>;

    fn build(self) -> Result<Self::ObjectType>;
}

pub trait ListIterator<'data> {
    type ValueReaderType: ValueReader<'data>;

    fn next(&mut self) -> Result<Option<Self::ValueReaderType>>;
}

///////////////////////////////////////////////////////////////////////////////

impl<'data> ParseFromValue<'data> for () {}

macro_rules! impl_numeric_parse_from {
    ($t:ty, $case:ident) => {
        impl<'data> ParseFromValue<'data> for $t {
            fn parse_from_primitive(value: PrimitiveValue<'data>) -> Result<Self> {
                Ok(match value {
                    // TODO: Block lossy conversions.
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
                })
            }

            fn parsing_hint() -> Option<ParsingTypeHint> {
                Some(ParsingTypeHint::$case)
            }
        }
    };
}

impl_numeric_parse_from!(i8, I8);
impl_numeric_parse_from!(u8, U8);
impl_numeric_parse_from!(i16, I16);
impl_numeric_parse_from!(u16, U16);
impl_numeric_parse_from!(i32, I32);
impl_numeric_parse_from!(u32, U32);
impl_numeric_parse_from!(i64, I64);
impl_numeric_parse_from!(u64, U64);
impl_numeric_parse_from!(f32, F32);
impl_numeric_parse_from!(f64, F64);
impl_numeric_parse_from!(isize, ISize);
impl_numeric_parse_from!(usize, USize);

impl<'data> ParseFromValue<'data> for bool {
    fn parse_from_primitive(value: PrimitiveValue<'data>) -> Result<Self> {
        Ok(match value {
            PrimitiveValue::Bool(v) => v,
            _ => {
                return Err(err_msg("Not a bool"));
            }
        })
    }

    fn parsing_hint() -> Option<ParsingTypeHint> {
        Some(ParsingTypeHint::Bool)
    }
}

// NOTE: This MUST implement all parse_from method.
impl<'data, T: ParseFromValue<'data> + Sized> ParseFromValue<'data> for Option<T> {
    fn parse_from_primitive(value: PrimitiveValue<'data>) -> Result<Self> {
        // TODO: Make it a user option to decide if we should tolerate null
        if let PrimitiveValue::Null = value {
            return Ok(None);
        }

        Ok(Some(T::parse_from_primitive(value)?))
    }

    fn parse_from_object<Input: ObjectIterator<'data>>(input: Input) -> Result<Self> {
        Ok(Some(T::parse_from_object(input)?))
    }

    fn parse_from_list<Input: ListIterator<'data>>(input: Input) -> Result<Self> {
        Ok(Some(T::parse_from_list(input)?))
    }

    // TODO: Have a special hint for optional values?
    fn parsing_hint() -> Option<ParsingTypeHint> {
        T::parsing_hint()
    }

    fn unwrap_parsed_result(name: &str, value: Option<Self>) -> Result<Self> {
        Ok(value.unwrap_or(None))
    }
}

impl<'data> ParseFromValue<'data> for String {
    fn parse_from_primitive(value: PrimitiveValue<'data>) -> Result<Self> {
        Ok(match value {
            PrimitiveValue::String(s) => s,
            PrimitiveValue::Str(s) => s.to_string(),
            _ => {
                return Err(err_msg("Not a string"));
            }
        })
    }

    fn parsing_hint() -> Option<ParsingTypeHint> {
        Some(ParsingTypeHint::String)
    }
}

impl<'data, T: ParseFromValue<'data> + Sized> ParseFromValue<'data> for Box<T> {
    fn parse_from_primitive(value: PrimitiveValue<'data>) -> Result<Self> {
        Ok(Box::new(T::parse_from_primitive(value)?))
    }

    fn parse_from_object<Input: ObjectIterator<'data>>(input: Input) -> Result<Self> {
        Ok(Box::new(T::parse_from_object(input)?))
    }

    fn parse_from_list<Input: ListIterator<'data>>(input: Input) -> Result<Self> {
        Ok(Box::new(T::parse_from_list(input)?))
    }

    fn parsing_hint() -> Option<ParsingTypeHint> {
        T::parsing_hint()
    }
}

impl<'data, T: ParseFromValue<'data> + Sized> ParseFromValue<'data> for Vec<T> {
    fn parse_from_list<Input: ListIterator<'data>>(mut input: Input) -> Result<Self> {
        let mut out = vec![];

        while let Some(i) = input.next()? {
            out.push(T::parse_from(i)?);
        }

        Ok(out)
    }

    fn parsing_hint() -> Option<ParsingTypeHint> {
        Some(ParsingTypeHint::List)
    }
}

impl<'data, T: ParseFromValue<'data> + Sized> ParseFromValue<'data> for HashMap<String, T> {
    fn parse_from_object<Input: ObjectIterator<'data>>(mut input: Input) -> Result<Self> {
        let mut out = HashMap::new();
        while let Some((key, value)) = input.next_field()? {
            if out.insert(key, T::parse_from(value)?).is_some() {
                return Err(err_msg("Duplicate field in map"));
            }
        }

        Ok(out)
    }

    fn parsing_hint() -> Option<ParsingTypeHint> {
        Some(ParsingTypeHint::Object)
    }
}
