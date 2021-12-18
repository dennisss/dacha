#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use std::convert::Into;

/// Helper for creating multi-line strings.
/// Designed to be used for simple code generation.
pub struct LineBuilder {
    lines: Vec<String>,
}

impl LineBuilder {
    pub fn new() -> Self {
        Self { lines: vec![] }
    }

    pub fn add<T: std::convert::Into<String>>(&mut self, line: T) {
        self.lines.push(line.into());
    }

    pub fn add_inline<T: std::convert::Into<String> + std::convert::AsRef<str>>(
        &mut self,
        line: T,
    ) {
        if let Some(last) = self.lines.last_mut() {
            *last += line.as_ref();
        } else {
            self.lines.push(line.into());
        }
    }

    pub fn append(&mut self, mut lines: LineBuilder) {
        self.lines.append(&mut lines.lines);
    }

    /// Similar to append() except the first line is merged with the last line
    /// of the current builder.
    pub fn append_inline(&mut self, mut lines: LineBuilder) {
        if let Some(last) = self.lines.last_mut() {
            if lines.lines.len() > 0 {
                *last += &lines.lines.remove(0);
            }
        }

        self.append(lines);
    }

    pub fn indent(&mut self) {
        for s in self.lines.iter_mut() {
            *s = format!("\t{}", s);
        }
    }

    pub fn indented<T, F: FnOnce(&mut LineBuilder) -> T>(&mut self, mut f: F) -> T {
        let mut inner = LineBuilder::new();
        let ret = f(&mut inner);
        inner.indent();
        self.append(inner);
        ret
    }

    pub fn wrap_with(&mut self, first: String, last: String) {
        let mut lines = vec![];
        lines.reserve(self.lines.len() + 2);
        lines.push(first);
        lines.append(&mut self.lines);
        lines.push(last);
        self.lines = lines;
    }

    pub fn empty(&self) -> bool {
        self.lines.len() == 0
    }

    pub fn nl(&mut self) {
        self.lines.push(String::new());
    }

    pub fn wrap_module(&mut self, name: &str) {
        let mut lines = vec![];
        lines.push(format!("pub mod {} {{", name));
        lines.push("\tuse super::*;".into());
        lines.push("".into());
        for s in self.lines.iter() {
            lines.push(format!("\t{}", s));
        }
        lines.push("}\n".into());
        self.lines = lines;
    }

    pub fn to_string(&self) -> String {
        let mut out = self.lines.join("\n");
        out.push('\n');
        out
    }
}
