use std::collections::HashMap;

use base_error::*;
use common::hash::FastHasherBuilder;

use crate::command::{Command, CommandCodec};
use crate::parser::*;

#[derive(Default)]
pub struct LineBuilder {
    words: Vec<Word>,
}

impl LineBuilder {
    pub fn add<C: CommandCodec>(&mut self, cmd: &C) {
        cmd.to_command_words(&mut self.words);
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    fn to_string_impl(&self, compact: bool) -> String {
        let mut out = String::new();

        for word in &self.words {
            if !compact && !out.is_empty() {
                out.push(' ');
            }

            out.push_str(&format!("{}{}", word.key, word.value.to_string()));
        }

        out.push('\n');
        out
    }

    pub fn to_string(&self) -> String {
        self.to_string_impl(false)
    }

    pub fn to_string_compact(&self) -> String {
        self.to_string_impl(true)
    }
}
