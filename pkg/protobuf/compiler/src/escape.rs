pub fn escape_rust_identifier(ident: &str) -> &str {
    if ident == "type" {
        return "typ";
    }

    if ident == "Option" {
        return "OptionProto";
    }

    if ident == "override" {
        return "override_field";
    }
    if ident == "in" {
        return "in_field";
    }

    if ident == "dyn" {
        return "dyn_field";
    }

    if ident == "Enum" {
        return "EnumProto";
    }

    if ident == "Message" {
        return "MessageProto";
    }

    ident
}
