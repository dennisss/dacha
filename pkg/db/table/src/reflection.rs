use base_error::*;
use protobuf::reflection::{Reflection, ReflectionMut};
use protobuf::{FieldNumber, MessageReflection};

pub fn field_by_path<'a>(
    mut message: &'a dyn MessageReflection,
    path: &[FieldNumber],
) -> Result<Reflection<'a>> {
    if path.is_empty() {
        return Err(err_msg("Empty field path"));
    }

    for num in &path[0..(path.len() - 1)] {
        message = match message.field_by_number(*num) {
            Some(Reflection::Message(m)) => m,
            _ => {
                return Err(err_msg(
                    "Failed to find a message field with the given number",
                ));
            }
        };
    }

    message
        .field_by_number(path[path.len() - 1])
        .ok_or_else(|| err_msg("Missing field with requested number"))
}

pub fn field_by_path_mut<'a>(
    mut message: &'a mut dyn MessageReflection,
    path: &[FieldNumber],
) -> Result<ReflectionMut<'a>> {
    if path.is_empty() {
        return Err(err_msg("Empty field path"));
    }

    for num in &path[0..(path.len() - 1)] {
        message = match message.field_by_number_mut(*num) {
            Some(ReflectionMut::Message(m)) => m,
            _ => {
                return Err(err_msg(
                    "Failed to find a message field with the given number",
                ));
            }
        };
    }

    message
        .field_by_number_mut(path[path.len() - 1])
        .ok_or_else(|| err_msg("Missing field with requested number"))
}

pub fn clear_field_by_path<'a>(
    mut message: &'a mut dyn MessageReflection,
    path: &[FieldNumber],
) -> Result<()> {
    if path.is_empty() {
        return Err(err_msg("Empty field path"));
    }

    for num in &path[0..(path.len() - 1)] {
        message = match message.field_by_number_mut(*num) {
            Some(ReflectionMut::Message(m)) => m,
            _ => {
                return Err(err_msg(
                    "Failed to find a message field with the given number",
                ));
            }
        };
    }

    // TODO: Check that it is a valid field number
    message.clear_field_with_number(path[path.len() - 1]);

    Ok(())
}
