use std::collections::HashMap;

use base_error::*;
use common::hash::FastHasherBuilder;

use crate::decimal::*;
use crate::parser::*;

#[derive(Clone)]
pub struct Line {
    command: Command,
    params: HashMap<char, WordValue, FastHasherBuilder>,
    params_order: Vec<char>,
}

impl Line {
    pub fn command(&self) -> &Command {
        &self.command
    }

    pub fn params(&self) -> &HashMap<char, WordValue, FastHasherBuilder> {
        &self.params
    }

    pub fn param_mut(&mut self, key: char) -> Option<&mut WordValue> {
        self.params.get_mut(&key)
    }

    pub fn to_string(&self) -> String {
        let mut out = self.command.to_string();

        for key in &self.params_order {
            let val = self.params.get(key).unwrap().to_string();
            out.push_str(&format!(" {}{}", *key, val));
        }

        out.push('\n');
        out
    }

    pub fn to_string_compact(&self) -> String {
        let mut out = self.command.to_string();

        for key in &self.params_order {
            let val = self.params.get(key).unwrap().to_string();
            out.push_str(&format!("{}{}", *key, val));
        }

        out.push('\n');
        out
    }
}

#[derive(Clone, PartialEq)]
pub struct Command {
    pub group: char,
    pub number: Decimal,
}

impl Command {
    pub fn new<N: Into<Decimal>>(group: char, number: N) -> Self {
        Self {
            group,
            number: number.into(),
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}{}", self.group, self.number)
    }
}

pub struct LineBuilder {
    command: Option<Command>,
    params: HashMap<char, WordValue, FastHasherBuilder>,
    params_order: Vec<char>,
}

impl LineBuilder {
    pub fn new() -> Self {
        Self {
            command: None,
            params: HashMap::default(),
            params_order: vec![],
        }
    }

    pub fn add_word(&mut self, word: Word) -> Result<()> {
        if self.command.is_none() {
            let number = match word.value {
                WordValue::RealValue(v) => v,
                _ => return Err(err_msg("Command does not have a valid number.")),
            };

            self.command = Some(Command {
                group: word.key,
                number,
            });

            return Ok(());
        }

        if self.params.contains_key(&word.key) {
            return Err(err_msg("Duplicate parameter"));
        }

        self.params.insert(word.key, word.value);
        self.params_order.push(word.key);
        Ok(())
    }

    pub fn finish(self) -> Option<Line> {
        let command = match self.command {
            Some(v) => v,
            None => return None,
        };

        Some(Line {
            command,
            params: self.params,
            params_order: self.params_order,
        })
    }
}
