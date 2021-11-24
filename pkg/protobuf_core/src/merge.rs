use common::errors::*;

use crate::reflection::Reflection;
use crate::reflection::ReflectionMut;
use crate::MessageReflection;

pub trait ReflectMergeFrom {
    fn reflect_merge_from(&mut self, other: &Self) -> Result<()>;
}

impl<T: MessageReflection> ReflectMergeFrom for T {
    fn reflect_merge_from(&mut self, other: &Self) -> Result<()> {
        // TODO: Check that the type url is the same (otherwise we can't make the below
        // assumptions about the ReflectionMut and Reflection enum cases matching).

        merge_messages(self, other)?;

        Ok(())
    }
}

fn merge_messages(this: &mut dyn MessageReflection, other: &dyn MessageReflection) -> Result<()> {
    for field in other.fields() {
        let new_value = match other.field_by_number(field.number) {
            Some(v) => v,
            None => continue,
        };

        let old_value = this.field_by_number_mut(field.number).unwrap();

        assign_reflection(old_value, new_value)?;
    }

    Ok(())
}

fn assign_reflection(to: ReflectionMut, from: Reflection) -> Result<()> {
    match to {
        ReflectionMut::F32(to) => {
            if let Reflection::F32(from) = from {
                *to = *from;
            }
        }
        ReflectionMut::F64(to) => {
            if let Reflection::F64(from) = from {
                *to = *from;
            }
        }
        ReflectionMut::I32(to) => {
            if let Reflection::I32(from) = from {
                *to = *from;
            }
        }
        ReflectionMut::I64(to) => {
            if let Reflection::I64(from) = from {
                *to = *from;
            }
        }
        ReflectionMut::U32(to) => {
            if let Reflection::U32(from) = from {
                *to = *from;
            }
        }
        ReflectionMut::U64(to) => {
            if let Reflection::U64(from) = from {
                *to = *from;
            }
        }
        ReflectionMut::Bool(to) => {
            if let Reflection::Bool(from) = from {
                *to = *from;
            }
        }
        ReflectionMut::String(to) => {
            if let Reflection::String(from) = from {
                *to = from.clone();
            }
        }
        ReflectionMut::Bytes(to) => {
            if let Reflection::Bytes(from) = from {
                to.clear();
                to.extend_from_slice(from);
            }
        }
        ReflectionMut::Repeated(to) => {
            if let Reflection::Repeated(from) = from {
                for i in 0..from.len() {
                    let v = to.add();
                    assign_reflection(v, from.get(i).unwrap())?;
                }
            }
        }
        ReflectionMut::Message(to) => {
            if let Reflection::Message(from) = from {
                merge_messages(to, from)?;
            }
        }
        ReflectionMut::Enum(to) => {
            if let Reflection::Enum(from) = from {
                to.assign(from.value())?;
            }
        }
        ReflectionMut::Set(to) => {
            if let Reflection::Set(from) = from {
                for item in from.iter() {
                    let mut entry = to.entry_mut();
                    assign_reflection(entry.value(), item)?;
                    entry.insert();
                }
            }
        }
    }

    Ok(())
}
