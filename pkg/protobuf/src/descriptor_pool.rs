use std::collections::HashSet;
use std::ops::DerefMut;
use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};

use common::errors::*;
use protobuf_core::{FieldDescriptorShort, FieldNumber, Message};
use protobuf_descriptor::{
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, MethodDescriptorProto,
};

#[derive(Clone)]
pub struct DescriptorPool {
    state: Arc<Mutex<DescriptorPoolState>>,
}

struct DescriptorPoolState {
    /// Map from the fully qualified name of each symbol in this pool to it's
    /// descriptor object.
    types: HashMap<String, TypeDescriptorInner>,
    added_files: HashSet<String>,
}

impl DescriptorPool {
    /// Creates a new empty descriptor pool.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(DescriptorPoolState {
                types: HashMap::new(),
                added_files: HashSet::new(),
            })),
        }
    }

    pub fn add_file(&self, data: &[u8]) -> Result<()> {
        let proto = FileDescriptorProto::parse(data)?;

        let mut state = self.state.lock().unwrap();

        // Don't re-add files.
        if !state.added_files.insert(proto.name().to_string()) {
            return Ok(());
        }

        for m in proto.message_type() {
            let name = if proto.package().is_empty() {
                m.name().to_string()
            } else {
                format!("{}.{}", proto.package(), m.name())
            };

            self.insert_unique_symbol(
                &name,
                TypeDescriptorInner::Message(Arc::new(MessageDescriptorInner::new(m))),
                state.deref_mut(),
            )?;

            self.add_nested_types(&name, m, state.deref_mut())?;
        }

        for e in proto.enum_type() {
            let name = if proto.package().is_empty() {
                e.name().to_string()
            } else {
                format!("{}.{}", proto.package(), e.name())
            };

            self.insert_unique_symbol(
                &name,
                TypeDescriptorInner::Enum(Arc::new(EnumDescriptorInner { proto: e.clone() })),
                state.deref_mut(),
            )?;
        }

        for s in proto.service() {
            let name = if proto.package().is_empty() {
                s.name().to_string()
            } else {
                format!("{}.{}", proto.package(), s.name())
            };

            self.insert_unique_symbol(
                &name,
                TypeDescriptorInner::Service(Arc::new(ServiceDescriptorInner { proto: s.clone() })),
                state.deref_mut(),
            )?;
        }

        Ok(())
    }

    /// Adds all types inside a message descriptor (excluding the message
    /// itself).
    fn add_nested_types(
        &self,
        message_name: &str,
        message: &DescriptorProto,
        state: &mut DescriptorPoolState,
    ) -> Result<()> {
        for m in message.nested_type() {
            let name = format!("{}.{}", message_name, m.name());
            self.insert_unique_symbol(
                &name,
                TypeDescriptorInner::Message(Arc::new(MessageDescriptorInner::new(m))),
                state,
            )?;
            self.add_nested_types(&name, m, state)?;
        }

        for e in message.enum_type() {
            let name = format!("{}.{}", message_name, e.name());
            self.insert_unique_symbol(
                &name,
                TypeDescriptorInner::Enum(Arc::new(EnumDescriptorInner { proto: e.clone() })),
                state,
            )?;
        }

        Ok(())
    }

    fn insert_unique_symbol(
        &self,
        name: &str,
        value: TypeDescriptorInner,
        state: &mut DescriptorPoolState,
    ) -> Result<()> {
        if state.types.insert(name.to_string(), value).is_some() {
            return Err(format_err!("Duplicate type named {}", name));
        }

        Ok(())
    }

    pub fn find_relative_type(&self, scope: &str, relative_name: &str) -> Option<TypeDescriptor> {
        let state = self.state.lock().unwrap();

        // TODO: Trim any '.' from the start of relative_name?

        let mut scope_parts = scope.split('.').collect::<Vec<_>>();
        if scope.is_empty() {
            scope_parts.pop();
        }

        let mut current_prefix = &scope_parts[..];
        loop {
            let name = {
                if current_prefix.len() == 0 {
                    relative_name.to_string()
                } else {
                    // TODO: Make joining cheap given we have the original concatenated string
                    // present.
                    format!("{}.{}", current_prefix.join("."), relative_name)
                }
            };

            if let Some(desc) = state.types.get(&name) {
                return Some(match desc {
                    TypeDescriptorInner::Message(m) => TypeDescriptor::Message(MessageDescriptor {
                        name,
                        pool: self.clone(),
                        inner: m.clone(),
                    }),
                    TypeDescriptorInner::Enum(e) => {
                        TypeDescriptor::Enum(EnumDescriptor { inner: e.clone() })
                    }
                    TypeDescriptorInner::Service(s) => TypeDescriptor::Service(ServiceDescriptor {
                        name,
                        pool: self.clone(),
                        inner: s.clone(),
                    }),
                });
            }

            if current_prefix.len() > 0 {
                current_prefix = &current_prefix[0..(current_prefix.len() - 1)];
            } else {
                break;
            }
        }

        None
    }
}

pub enum TypeDescriptor {
    Message(MessageDescriptor),
    Enum(EnumDescriptor),
    Service(ServiceDescriptor),
}

enum TypeDescriptorInner {
    Message(Arc<MessageDescriptorInner>),
    Enum(Arc<EnumDescriptorInner>),
    Service(Arc<ServiceDescriptorInner>),
}

impl TypeDescriptor {
    pub fn to_message(self) -> Option<MessageDescriptor> {
        match self {
            TypeDescriptor::Message(v) => Some(v),
            _ => None,
        }
    }

    pub fn to_enum(self) -> Option<EnumDescriptor> {
        match self {
            TypeDescriptor::Enum(v) => Some(v),
            _ => None,
        }
    }

    pub fn to_service(self) -> Option<ServiceDescriptor> {
        match self {
            TypeDescriptor::Service(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct MessageDescriptor {
    name: String,
    pool: DescriptorPool,
    inner: Arc<MessageDescriptorInner>,
}

impl MessageDescriptor {
    pub fn fields(&self) -> &[FieldDescriptorShort] {
        &self.inner.fields_short
    }

    pub fn field_by_number(&self, num: FieldNumber) -> Option<FieldDescriptor> {
        for i in 0..self.inner.proto.field_len() {
            let field: &protobuf_descriptor::FieldDescriptorProto = &self.inner.proto.field()[i];
            if field.number() == num as i32 {
                return Some(FieldDescriptor {
                    message: self.clone(),
                    field_index: i,
                });
            }
        }

        None
    }

    pub fn field_number_by_name(&self, name: &str) -> Option<FieldNumber> {
        for i in 0..self.inner.proto.field_len() {
            let field: &protobuf_descriptor::FieldDescriptorProto = &self.inner.proto.field()[i];
            if field.name() == name {
                return Some(field.number() as FieldNumber);
            }
        }

        None
    }
}

struct MessageDescriptorInner {
    proto: protobuf_descriptor::DescriptorProto,
    fields_short: Vec<FieldDescriptorShort>,
}

impl MessageDescriptorInner {
    fn new(proto: &protobuf_descriptor::DescriptorProto) -> Self {
        let mut fields_short = vec![];
        for field in proto.field() {
            fields_short.push(FieldDescriptorShort::new(
                field.name().to_string(),
                field.number() as u32,
            ));
        }

        Self {
            proto: proto.clone(),
            fields_short,
        }
    }
}

#[derive(Clone)]
pub struct ServiceDescriptor {
    name: String,
    pool: DescriptorPool,
    inner: Arc<ServiceDescriptorInner>,
}

struct ServiceDescriptorInner {
    proto: protobuf_descriptor::ServiceDescriptorProto,
}

impl ServiceDescriptor {
    pub fn proto(&self) -> &protobuf_descriptor::ServiceDescriptorProto {
        &self.inner.proto
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn method(&self, index: usize) -> Option<MethodDescriptor> {
        if index >= self.proto().method_len() {
            return None;
        }

        Some(MethodDescriptor {
            service: self,
            method: &self.proto().method()[index],
        })
    }

    pub fn method_len(&self) -> usize {
        self.proto().method_len()
    }
}

pub struct MethodDescriptor<'a> {
    service: &'a ServiceDescriptor,
    method: &'a MethodDescriptorProto,
}

impl<'a> MethodDescriptor<'a> {
    pub fn proto(&self) -> &MethodDescriptorProto {
        &self.method
    }

    pub fn input_type(&self) -> Option<MessageDescriptor> {
        self.service
            .pool
            .find_relative_type(&self.service.name, self.method.input_type())
            .and_then(|t| t.to_message())
    }

    pub fn output_type(&self) -> Option<MessageDescriptor> {
        self.service
            .pool
            .find_relative_type(&self.service.name, self.method.output_type())
            .and_then(|t| t.to_message())
    }
}

#[derive(Clone)]
pub struct EnumDescriptor {
    inner: Arc<EnumDescriptorInner>,
}

struct EnumDescriptorInner {
    proto: protobuf_descriptor::EnumDescriptorProto,
}

impl EnumDescriptor {
    pub fn proto(&self) -> &protobuf_descriptor::EnumDescriptorProto {
        &self.inner.proto
    }
}

#[derive(Clone)]
pub struct FieldDescriptor {
    message: MessageDescriptor,
    field_index: usize,
}

impl FieldDescriptor {
    pub fn proto(&self) -> &FieldDescriptorProto {
        &self.message.inner.proto.field()[self.field_index]
    }

    /// Assuming this field has a named type like an enum or message, this will
    /// get that type.
    pub fn find_type(&self) -> Option<TypeDescriptor> {
        self.message
            .pool
            .find_relative_type(self.message.inner.proto.name(), self.proto().type_name())
    }
}
