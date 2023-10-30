use core::ops::{Deref, DerefMut};

use common::errors::*;
use reflection::{ParseFrom, ParseFromValue};

#[derive(Default, Clone, Debug)]
pub struct List<T> {
    inner: Vec<T>,
}

impl<T> Deref for List<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for List<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'data, T: reflection::ParseFromValue<'data> + Sized> reflection::ParseFromValue<'data>
    for List<T>
{
    fn parse_merge<Input: reflection::ValueReader<'data>>(&mut self, input: Input) -> Result<()> {
        let items = Self::parse_from(input)?;
        self.inner.extend(items.inner);
        Ok(())
    }

    fn parse_from_primitive(value: reflection::PrimitiveValue<'data>) -> Result<Self> {
        Ok(Self {
            inner: vec![T::parse_from_primitive(value)?],
        })
    }

    fn parse_from_object<Input: reflection::ObjectIterator<'data>>(input: Input) -> Result<Self> {
        Ok(Self {
            inner: vec![T::parse_from_object(input)?],
        })
    }

    fn parse_from_list<Input: reflection::ListIterator<'data>>(mut input: Input) -> Result<Self> {
        Ok(Self {
            inner: Vec::<T>::parse_from_list(input)?,
        })
    }

    fn parsing_hint() -> Option<reflection::ParsingTypeHint> {
        Vec::<T>::parsing_hint()
    }

    fn unwrap_parsed_result(name: &str, value: Option<Self>) -> Result<Self>
    where
        Self: Sized,
    {
        match value {
            Some(v) => Ok(v),
            // XML has no concept of lists so having no elements is the same as having an empty
            // list.
            None => Ok(Self { inner: vec![] }),
        }
    }
}

impl<T: reflection::SerializeTo> reflection::SerializeTo for List<T> {
    fn serialize_to<Output: reflection::ValueSerializer>(&self, out: Output) -> Result<()> {
        self.inner.serialize_to(out)
    }

    fn serialize_sparse_as_empty_value(&self) -> bool {
        self.inner.serialize_sparse_as_empty_value()
    }
}
