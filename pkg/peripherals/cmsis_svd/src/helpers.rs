use common::errors::*;

pub fn get_child_named<'a>(element: &'a xml::Element, name: &str) -> Result<&'a xml::Element> {
    let el = get_optional_child_named(element, name)?;
    el.ok_or_else(|| format_err!("No child named: {} in <{}>", name, element.name))
}

pub fn get_optional_child_named<'a>(
    element: &'a xml::Element,
    name: &str,
) -> Result<Option<&'a xml::Element>> {
    let mut items = element.children().filter(|e| e.name == name);
    let el = items.next();

    if items.next().is_some() {
        return Err(format_err!("More than one element named: {}", name));
    }

    Ok(el)
}

pub fn inner_text(element: &xml::Element) -> Result<&str> {
    if element.content.len() != 1 {
        return Err(err_msg("Expected element to have one text child"));
    }

    let text = match &element.content[0] {
        xml::Node::Text(t) => t.as_str(),
        _ => {
            return Err(err_msg("Not text"));
        }
    };

    Ok(text)
}

pub fn decode_number(text: &str) -> Result<usize> {
    if let Some(hex_num) = text.strip_prefix("0x") {
        return Ok(usize::from_str_radix(hex_num, 16)?);
    }

    Ok(usize::from_str_radix(text, 10)?)
}

pub fn find_one<I: Iterator>(mut iter: I) -> Result<I::Item> {
    let value = iter
        .next()
        .ok_or_else(|| err_msg("Expected at least one"))?;
    if iter.next().is_some() {
        return Err(err_msg("More than one value present"));
    }

    Ok(value)
}

pub fn escape_keyword(name: &str) -> String {
    if name.chars().next().unwrap().is_ascii_digit() {
        return format!("_{}", name);
    }

    if [
        "match", "loop", "in", "loop", "mod", "let", "mut", "type", "break",
    ]
    .contains(&name)
    {
        format!("r#{}", name)
    } else {
        name.to_string()
    }
}
