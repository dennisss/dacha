use common::errors::*;
use crypto::hasher::Hasher;

use crate::object::*;
use crate::value::*;
use crate::value_attributes;

pub struct TupleValue {
    elements: Vec<ObjectWeak<dyn Value>>,
}

impl TupleValue {
    pub fn new(elements: Vec<ObjectWeak<dyn Value>>) -> Self {
        Self { elements }
    }

    pub(crate) fn call_eq_impl(
        elements: &[ObjectWeak<dyn Value>],
        other_elements: &[ObjectWeak<dyn Value>],
        frame: &mut ValueCallFrame,
    ) -> Result<bool> {
        if elements.len() != other_elements.len() {
            return Ok(false);
        }

        for (cur, other) in elements.iter().zip(other_elements.iter()) {
            let value = cur.upgrade_or_error()?;
            let mut inner_context = frame.child(&*value)?;

            let other_value = other.upgrade_or_error()?;

            if !value.call_eq(&*other_value, &mut inner_context)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub(crate) fn call_repr_impl(
        start_bracket: &str,
        elements: &[ObjectWeak<dyn Value>],
        end_bracket: &str,
        frame: &mut ValueCallFrame,
    ) -> Result<String> {
        let mut out = String::new();
        out.push_str(start_bracket);

        for (i, el) in elements.iter().enumerate() {
            if i > 0 {
                out.push_str(" ");
            }

            let value = el.upgrade_or_error()?;
            let mut inner_context = frame.child(&*value)?;

            let s = value.call_repr(&mut inner_context)?;

            out.push_str(&s);
            out.push_str(",");
        }

        out.push_str(end_bracket);
        Ok(out)
    }
}

impl Value for TupleValue {
    value_attributes!(Immutable | ReprAsStr);

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        out.extend_from_slice(&self.elements);
    }

    fn call_bool(&self) -> bool {
        !self.elements.is_empty()
    }

    fn call_repr(&self, mut frame: &mut ValueCallFrame) -> Result<String> {
        Self::call_repr_impl("(", &self.elements, ")", frame)
    }

    fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()> {
        for el in &self.elements {
            let value = el.upgrade_or_error()?;
            let mut inner_context = frame.child(&*value)?;
            value.call_hash(hasher, &mut inner_context)?;
        }

        Ok(())
    }

    fn call_eq(&self, other: &dyn Value, frame: &mut ValueCallFrame) -> Result<bool> {
        if core::ptr::eq::<dyn Value>(self, other) {
            return Ok(true);
        }

        let other = match other.as_any().downcast_ref::<Self>() {
            Some(v) => v,
            None => return Ok(false),
        };

        Self::call_eq_impl(&self.elements, &other.elements, frame)
    }

    fn call_len(&self, frame: &mut ValueCallFrame) -> Result<usize> {
        Ok(self.elements.len())
    }
}
