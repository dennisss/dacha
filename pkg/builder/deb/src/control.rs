use std::collections::HashMap;

use common::errors::*;

/*
Control Files

    Is UTF-8
    Lines starting with '#' are comments and ignored

    FieldName = US-ASCII (non-control or whitespace)
    Followed by ':'

    Followed by value
    Multi-line valuesstart with a space or a tab.

    " ." is an empty line (should trim any trailing whitespace tyough)

ControlFile => Multiple Stanzas (separated by whitespace only lines)
 */

regexp!(FIELD_PATTERN => "^([-A-Za-z0-9_+]+):\\s*(.*)$");

#[derive(Clone, Debug)]
pub struct ControlFile {
    pub stanzas: Vec<ControlFileStanza>,
}

#[derive(Clone, Debug)]
pub struct ControlFileStanza {
    pub fields: HashMap<String, String>,
}

impl ControlFile {
    pub fn parse(data: &str) -> Result<Self> {
        let mut stanzas = vec![];

        let mut current_stanza: Option<ControlStanzaBuilder> = None;

        for line in data.lines() {
            if line.starts_with('#') {
                // Comment
                continue;
            }

            // Stanza delimiter
            if line.trim().is_empty() {
                if let Some(s) = current_stanza.take() {
                    stanzas.push(s.finish());
                }

                continue;
            }

            if let Some(value_line) = line.strip_prefix(" ").or(line.strip_prefix("\t")) {
                let mut builder = current_stanza
                    .as_mut()
                    .ok_or_else(|| err_msg("Got mulit-line value outside of a stanza"))?;

                builder.add_value_line(value_line)?;
                continue;
            }

            let field = match FIELD_PATTERN.exec(line) {
                Some(v) => v,
                None => return Err(format_err!("Expected a field but got: {}", line)),
            };

            let field_name = field.group_str(1).unwrap()?;
            let field_value = field.group_str(2).unwrap()?;

            let s = current_stanza.get_or_insert_with(|| ControlStanzaBuilder::new());
            s.add_field(field_name, field_value)?;
        }

        if let Some(s) = current_stanza.take() {
            stanzas.push(s.finish());
        }

        Ok(Self { stanzas })
    }
}

struct ControlStanzaBuilder {
    fields: HashMap<String, String>,
    current_field: Option<(String, String)>,
}

impl ControlStanzaBuilder {
    fn new() -> Self {
        Self {
            fields: HashMap::new(),
            current_field: None,
        }
    }

    fn add_field(&mut self, name: &str, value: &str) -> Result<()> {
        self.finish_field();

        if self.fields.contains_key(name) {
            return Err(format_err!("Duplicate field named: {}", name));
        }

        self.current_field = Some((name.to_string(), value.to_string()));

        Ok(())
    }

    fn add_value_line(&mut self, mut value_line: &str) -> Result<()> {
        // Escaped new line.
        if value_line.trim() == "." {
            value_line = "";
        }

        let value = match self.current_field.as_mut() {
            Some((_, v)) => v,
            None => {
                return Err(format_err!(
                    "Got multi-line value outside of a field: {}",
                    value_line
                ));
            }
        };

        value.push('\n');
        value.push_str(value_line);

        Ok(())
    }

    fn finish_field(&mut self) {
        if let Some((key, value)) = self.current_field.take() {
            self.fields.insert(key, value);
        }
    }

    fn finish(mut self) -> ControlFileStanza {
        self.finish_field();
        ControlFileStanza {
            fields: self.fields,
        }
    }
}
