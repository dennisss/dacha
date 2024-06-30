// AUTOGENERATED BY THE PROTOBUF COMPILER

#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use common::collections::FixedString;
use common::const_default::{ConstDefault, StaticDefault};
use common::errors::*;
use common::fixed::vec::FixedVec;
use common::list::Appendable;
use protobuf_core::*;

use protobuf_core::codecs::*;

use protobuf_core::wire::*;

#[cfg(feature = "alloc")]
use protobuf_core::reflection::*;

#[cfg(feature = "std")]
pub static FILE_DESCRIPTOR_635CB7D60B6984D8: protobuf_core::StaticFileDescriptor = protobuf_core::StaticFileDescriptor {
                proto: b"\x0a\x2cpkg\x2fprotobuf\x2fcompiler\x2fproto\x2fextensions\x2eproto\x12\x05dacha\x1a\x20google\x2fprotobuf\x2fdescriptor\x2eproto\x3a2\x0a\x09max\x5fcount\x18\xc1\x3e\x20\x01\x28\x0d\x12\x1cgoogle\x2eprotobuf\x2eFieldOptionsB\x00\x3a3\x0a\x0amax\x5flength\x18\xc2\x3e\x20\x01\x28\x0d\x12\x1cgoogle\x2eprotobuf\x2eFieldOptionsB\x00\x3a6\x0a\x0dunordered\x5fset\x18\xc3\x3e\x20\x01\x28\x08\x12\x1cgoogle\x2eprotobuf\x2eFieldOptionsB\x00\x3a\x2c\x0a\x03key\x18\xc4\x3e\x20\x01\x28\x09\x12\x1cgoogle\x2eprotobuf\x2eFieldOptionsB\x00\x3a4\x0a\x09typed\x5fnum\x18\xc4\x3e\x20\x01\x28\x08\x12\x1egoogle\x2eprotobuf\x2eMessageOptionsB\x00B\x00b\x06proto2",
                dependencies: &[// google/protobuf/descriptor.proto
&protobuf_descriptor::google::protobuf::FILE_DESCRIPTOR_0F2934D003718DD8,
]
            };

struct MAX_COUNT_EXTENSION_TAG {}

impl protobuf_core::ExtensionTag for MAX_COUNT_EXTENSION_TAG {
    fn extension_number(&self) -> protobuf_core::ExtensionNumberType {
        8001
    }

    fn extension_name(&self) -> protobuf_core::StringPtr {
        protobuf_core::StringPtr::Static("dacha.max_count")
    }

    fn default_extension_value(&self) -> protobuf_core::Value {
        use protobuf_core::SingularValue;
        protobuf_core::Value::new(SingularValue::UInt32(0), false)
    }
}

pub trait MaxCountExtension {
    // TODO: Add has_ accessor and clear_accessors

    fn max_count(&self) -> protobuf_core::WireResult<ExtensionRef<u32>>;
    fn max_count_mut(&mut self) -> protobuf_core::WireResult<&mut u32>;
}

impl MaxCountExtension for protobuf_descriptor::google::protobuf::FieldOptions {
    fn max_count(&self) -> protobuf_core::WireResult<ExtensionRef<u32>> {
        use common::any::AsAny;
        use protobuf_core::ExtensionRef;

        let v = self
            .extensions()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic(&MAX_COUNT_EXTENSION_TAG {})?;

        Ok(match v {
            ExtensionRef::Pointer(v) => match v {
                Value::Singular(SingularValue::UInt32(v)) => ExtensionRef::Pointer(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            ExtensionRef::Owned(v) => match v {
                Value::Singular(SingularValue::UInt32(v)) => ExtensionRef::Owned(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            // Should never be returned by get_dynamic().
            ExtensionRef::Boxed(v) => todo!(),
        })
    }

    fn max_count_mut(&mut self) -> protobuf_core::WireResult<&mut u32> {
        use common::any::AsAny;
        use protobuf_core::{RepeatedValues, SingularValue, Value};

        let v = self
            .extensions_mut()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic_mut(&MAX_COUNT_EXTENSION_TAG {})?;

        Ok(match v {
            Value::Singular(SingularValue::UInt32(v)) => v,
            _ => return Err(protobuf_core::WireError::BadDescriptor),
        })
    }
}

struct MAX_LENGTH_EXTENSION_TAG {}

impl protobuf_core::ExtensionTag for MAX_LENGTH_EXTENSION_TAG {
    fn extension_number(&self) -> protobuf_core::ExtensionNumberType {
        8002
    }

    fn extension_name(&self) -> protobuf_core::StringPtr {
        protobuf_core::StringPtr::Static("dacha.max_length")
    }

    fn default_extension_value(&self) -> protobuf_core::Value {
        use protobuf_core::SingularValue;
        protobuf_core::Value::new(SingularValue::UInt32(0), false)
    }
}

pub trait MaxLengthExtension {
    // TODO: Add has_ accessor and clear_accessors

    fn max_length(&self) -> protobuf_core::WireResult<ExtensionRef<u32>>;
    fn max_length_mut(&mut self) -> protobuf_core::WireResult<&mut u32>;
}

impl MaxLengthExtension for protobuf_descriptor::google::protobuf::FieldOptions {
    fn max_length(&self) -> protobuf_core::WireResult<ExtensionRef<u32>> {
        use common::any::AsAny;
        use protobuf_core::ExtensionRef;

        let v = self
            .extensions()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic(&MAX_LENGTH_EXTENSION_TAG {})?;

        Ok(match v {
            ExtensionRef::Pointer(v) => match v {
                Value::Singular(SingularValue::UInt32(v)) => ExtensionRef::Pointer(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            ExtensionRef::Owned(v) => match v {
                Value::Singular(SingularValue::UInt32(v)) => ExtensionRef::Owned(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            // Should never be returned by get_dynamic().
            ExtensionRef::Boxed(v) => todo!(),
        })
    }

    fn max_length_mut(&mut self) -> protobuf_core::WireResult<&mut u32> {
        use common::any::AsAny;
        use protobuf_core::{RepeatedValues, SingularValue, Value};

        let v = self
            .extensions_mut()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic_mut(&MAX_LENGTH_EXTENSION_TAG {})?;

        Ok(match v {
            Value::Singular(SingularValue::UInt32(v)) => v,
            _ => return Err(protobuf_core::WireError::BadDescriptor),
        })
    }
}

struct UNORDERED_SET_EXTENSION_TAG {}

impl protobuf_core::ExtensionTag for UNORDERED_SET_EXTENSION_TAG {
    fn extension_number(&self) -> protobuf_core::ExtensionNumberType {
        8003
    }

    fn extension_name(&self) -> protobuf_core::StringPtr {
        protobuf_core::StringPtr::Static("dacha.unordered_set")
    }

    fn default_extension_value(&self) -> protobuf_core::Value {
        use protobuf_core::SingularValue;
        protobuf_core::Value::new(SingularValue::Bool(false), false)
    }
}

pub trait UnorderedSetExtension {
    // TODO: Add has_ accessor and clear_accessors

    fn unordered_set(&self) -> protobuf_core::WireResult<ExtensionRef<bool>>;
    fn unordered_set_mut(&mut self) -> protobuf_core::WireResult<&mut bool>;
}

impl UnorderedSetExtension for protobuf_descriptor::google::protobuf::FieldOptions {
    fn unordered_set(&self) -> protobuf_core::WireResult<ExtensionRef<bool>> {
        use common::any::AsAny;
        use protobuf_core::ExtensionRef;

        let v = self
            .extensions()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic(&UNORDERED_SET_EXTENSION_TAG {})?;

        Ok(match v {
            ExtensionRef::Pointer(v) => match v {
                Value::Singular(SingularValue::Bool(v)) => ExtensionRef::Pointer(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            ExtensionRef::Owned(v) => match v {
                Value::Singular(SingularValue::Bool(v)) => ExtensionRef::Owned(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            // Should never be returned by get_dynamic().
            ExtensionRef::Boxed(v) => todo!(),
        })
    }

    fn unordered_set_mut(&mut self) -> protobuf_core::WireResult<&mut bool> {
        use common::any::AsAny;
        use protobuf_core::{RepeatedValues, SingularValue, Value};

        let v = self
            .extensions_mut()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic_mut(&UNORDERED_SET_EXTENSION_TAG {})?;

        Ok(match v {
            Value::Singular(SingularValue::Bool(v)) => v,
            _ => return Err(protobuf_core::WireError::BadDescriptor),
        })
    }
}

struct KEY_EXTENSION_TAG {}

impl protobuf_core::ExtensionTag for KEY_EXTENSION_TAG {
    fn extension_number(&self) -> protobuf_core::ExtensionNumberType {
        8004
    }

    fn extension_name(&self) -> protobuf_core::StringPtr {
        protobuf_core::StringPtr::Static("dacha.key")
    }

    fn default_extension_value(&self) -> protobuf_core::Value {
        use protobuf_core::SingularValue;
        protobuf_core::Value::new(SingularValue::String(String::new()), false)
    }
}

pub trait KeyExtension {
    // TODO: Add has_ accessor and clear_accessors

    fn key(&self) -> protobuf_core::WireResult<ExtensionRef<String>>;
    fn key_mut(&mut self) -> protobuf_core::WireResult<&mut String>;
}

impl KeyExtension for protobuf_descriptor::google::protobuf::FieldOptions {
    fn key(&self) -> protobuf_core::WireResult<ExtensionRef<String>> {
        use common::any::AsAny;
        use protobuf_core::ExtensionRef;

        let v = self
            .extensions()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic(&KEY_EXTENSION_TAG {})?;

        Ok(match v {
            ExtensionRef::Pointer(v) => match v {
                Value::Singular(SingularValue::String(v)) => ExtensionRef::Pointer(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            ExtensionRef::Owned(v) => match v {
                Value::Singular(SingularValue::String(v)) => ExtensionRef::Owned(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            // Should never be returned by get_dynamic().
            ExtensionRef::Boxed(v) => todo!(),
        })
    }

    fn key_mut(&mut self) -> protobuf_core::WireResult<&mut String> {
        use common::any::AsAny;
        use protobuf_core::{RepeatedValues, SingularValue, Value};

        let v = self
            .extensions_mut()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic_mut(&KEY_EXTENSION_TAG {})?;

        Ok(match v {
            Value::Singular(SingularValue::String(v)) => v,
            _ => return Err(protobuf_core::WireError::BadDescriptor),
        })
    }
}

struct TYPED_NUM_EXTENSION_TAG {}

impl protobuf_core::ExtensionTag for TYPED_NUM_EXTENSION_TAG {
    fn extension_number(&self) -> protobuf_core::ExtensionNumberType {
        8004
    }

    fn extension_name(&self) -> protobuf_core::StringPtr {
        protobuf_core::StringPtr::Static("dacha.typed_num")
    }

    fn default_extension_value(&self) -> protobuf_core::Value {
        use protobuf_core::SingularValue;
        protobuf_core::Value::new(SingularValue::Bool(false), false)
    }
}

pub trait TypedNumExtension {
    // TODO: Add has_ accessor and clear_accessors

    fn typed_num(&self) -> protobuf_core::WireResult<ExtensionRef<bool>>;
    fn typed_num_mut(&mut self) -> protobuf_core::WireResult<&mut bool>;
}

impl TypedNumExtension for protobuf_descriptor::google::protobuf::MessageOptions {
    fn typed_num(&self) -> protobuf_core::WireResult<ExtensionRef<bool>> {
        use common::any::AsAny;
        use protobuf_core::ExtensionRef;

        let v = self
            .extensions()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic(&TYPED_NUM_EXTENSION_TAG {})?;

        Ok(match v {
            ExtensionRef::Pointer(v) => match v {
                Value::Singular(SingularValue::Bool(v)) => ExtensionRef::Pointer(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            ExtensionRef::Owned(v) => match v {
                Value::Singular(SingularValue::Bool(v)) => ExtensionRef::Owned(v),
                _ => return Err(protobuf_core::WireError::BadDescriptor),
            },
            // Should never be returned by get_dynamic().
            ExtensionRef::Boxed(v) => todo!(),
        })
    }

    fn typed_num_mut(&mut self) -> protobuf_core::WireResult<&mut bool> {
        use common::any::AsAny;
        use protobuf_core::{RepeatedValues, SingularValue, Value};

        let v = self
            .extensions_mut()
            .ok_or(protobuf_core::WireError::BadDescriptor)?
            .get_dynamic_mut(&TYPED_NUM_EXTENSION_TAG {})?;

        Ok(match v {
            Value::Singular(SingularValue::Bool(v)) => v,
            _ => return Err(protobuf_core::WireError::BadDescriptor),
        })
    }
}
