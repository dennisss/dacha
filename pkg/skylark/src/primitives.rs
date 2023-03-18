use common::errors::*;
use crypto::hasher::Hasher;

use crate::object::*;
use crate::value::*;
use crate::value_attributes;

pub trait ValueExt {
    fn downcast_int(&self) -> Option<i64>;

    fn downcast_string(&self) -> Option<&str>;

    fn downcast_float(&self) -> Option<f64>;

    fn downcast_bool(&self) -> Option<bool>;
}

impl ValueExt for dyn Value {
    fn downcast_int(&self) -> Option<i64> {
        self.as_any().downcast_ref::<IntValue>().map(|v| v.value)
    }

    fn downcast_string(&self) -> Option<&str> {
        self.as_any()
            .downcast_ref::<StringValue>()
            .map(|v| v.value.as_str())
    }

    fn downcast_float(&self) -> Option<f64> {
        self.as_any().downcast_ref::<FloatValue>().map(|v| v.value)
    }

    fn downcast_bool(&self) -> Option<bool> {
        self.as_any().downcast_ref::<BoolValue>().map(|v| v.value)
    }
}

pub struct NoneValue {
    hidden: (),
}

impl NoneValue {
    pub fn new() -> Self {
        Self { hidden: () }
    }
}

impl Value for NoneValue {
    value_attributes!(Immutable | NoChildren | ReprAsStr);

    fn call_bool(&self) -> bool {
        false
    }

    fn call_repr(&self, frame: &mut ValueCallFrame) -> Result<String> {
        Ok("None".to_string())
    }

    fn call_eq(&self, other: &dyn Value, frame: &mut ValueCallFrame) -> Result<bool> {
        Ok(other
            .as_any()
            .downcast_ref::<Self>()
            .map(|v| true)
            .unwrap_or(false))
    }

    fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()> {
        Ok(())
    }
}

pub struct NotImplementedValue {
    hidden: (),
}

impl NotImplementedValue {
    pub fn new() -> Self {
        Self { hidden: () }
    }
}

impl Value for NotImplementedValue {
    value_attributes!(Immutable | NoChildren | ReprAsStr);

    fn call_bool(&self) -> bool {
        false
    }

    fn call_repr(&self, frame: &mut ValueCallFrame) -> Result<String> {
        Ok("NotImplemented".to_string())
    }

    fn call_eq(&self, other: &dyn Value, frame: &mut ValueCallFrame) -> Result<bool> {
        Ok(other
            .as_any()
            .downcast_ref::<Self>()
            .map(|v| true)
            .unwrap_or(false))
    }

    fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()> {
        Ok(())
    }
}

pub struct BoolValue {
    value: bool,
}

impl BoolValue {
    pub fn new(value: bool) -> Self {
        Self { value }
    }
}

impl Value for BoolValue {
    value_attributes!(Immutable | NoChildren | ReprAsStr);

    fn call_bool(&self) -> bool {
        self.value
    }

    fn call_repr(&self, frame: &mut ValueCallFrame) -> Result<String> {
        Ok(if self.value { "True" } else { "False" }.to_string())
    }

    fn call_eq(&self, other: &dyn Value, frame: &mut ValueCallFrame) -> Result<bool> {
        Ok(other
            .as_any()
            .downcast_ref::<Self>()
            .map(|other| self.value == other.value)
            .unwrap_or(false))
    }

    fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()> {
        hasher.update(if self.value { &[1] } else { &[0] });
        Ok(())
    }
}

pub struct IntValue {
    value: i64,
}

impl IntValue {
    pub fn new(value: i64) -> Self {
        Self { value }
    }
}

impl Value for IntValue {
    value_attributes!(Immutable | NoChildren | ReprAsStr);

    fn call_bool(&self) -> bool {
        self.value != 0
    }

    fn call_repr(&self, frame: &mut ValueCallFrame) -> Result<String> {
        Ok(self.value.to_string())
    }

    fn call_eq(&self, other: &dyn Value, frame: &mut ValueCallFrame) -> Result<bool> {
        Ok(other
            .as_any()
            .downcast_ref::<Self>()
            .map(|other| self.value == other.value)
            .unwrap_or(false))
    }

    fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()> {
        hasher.update(&self.value.to_le_bytes());
        Ok(())
    }
}

pub struct FloatValue {
    value: f64,
}

impl FloatValue {
    pub fn new(value: f64) -> Self {
        Self { value }
    }
}

impl Value for FloatValue {
    value_attributes!(Immutable | NoChildren | ReprAsStr);

    fn call_bool(&self) -> bool {
        self.value != 0.
    }

    fn call_repr(&self, frame: &mut ValueCallFrame) -> Result<String> {
        Ok(self.value.to_string())
    }

    fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()> {
        hasher.update(&self.value.to_le_bytes());
        Ok(())
    }

    fn call_eq(&self, other: &dyn Value, frame: &mut ValueCallFrame) -> Result<bool> {
        Ok(other
            .as_any()
            .downcast_ref::<Self>()
            .map(|other| self.value == other.value)
            .unwrap_or(false))
    }
}

pub struct StringValue {
    value: String,
}

impl StringValue {
    pub fn new(value: String) -> Self {
        Self { value }
    }
}

impl Value for StringValue {
    value_attributes!(Immutable | NoChildren);

    fn call_bool(&self) -> bool {
        !self.value.is_empty()
    }

    fn call_repr(&self, frame: &mut ValueCallFrame) -> Result<String> {
        Ok(format!("\"{}\"", self.value))
    }

    fn call_str(&self, frame: &mut ValueCallFrame) -> Result<String> {
        Ok(self.value.clone())
    }

    fn call_eq(&self, other: &dyn Value, frame: &mut ValueCallFrame) -> Result<bool> {
        Ok(other
            .as_any()
            .downcast_ref::<Self>()
            .map(|other| self.value == other.value)
            .unwrap_or(false))
    }

    fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()> {
        hasher.update(self.value.as_bytes());
        Ok(())
    }
}
