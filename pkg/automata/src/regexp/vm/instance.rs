use common::errors::*;

use crate::regexp::node::*;
use crate::regexp::vm::instruction::*;
use crate::regexp::vm::compiler::*;
use crate::regexp::vm::executor::*;

pub struct RegExp {
    program: Program
}

impl RegExp {
    pub fn new(expr: &str) -> Result<Self> {
        let root = RegExpNode::parse(&format!(".*({})", expr))?;
        let program = Compiler::compile(&root)?;
        Ok(Self { program })
    }

    /// Returns true if and only if a match for the given regular expression is found somewhere
    /// in the given input string.
    pub fn test<T: AsRef<[u8]>>(&self, input: T) -> bool {
        let mut executor = Executor::new(&self.program.instructions);
        let results = executor.run(input.as_ref(), 0);
        results.is_some()
    }

    pub fn exec<'a, 'b, T: 'b + AsRef<[u8]> + ?Sized>(&'a self, input: &'b T) -> Option<RegExpMatch<'a, 'b>> {
        Self::exec_impl(&self.program.instructions, input.as_ref())
    }

    fn exec_impl<'a, 'b>(instructions: &'a [Instruction], input: &'b [u8]) -> Option<RegExpMatch<'a, 'b>> {
        let state = RegExpMatch {
            instructions,
            input,
            index: 0,
            last_index: 0,
            string_pointers: SavedStringPointers::default()
        };

        state.next()
    }


    /// NOTE: Only meant for usage in the 'regexp_macros' package.
    pub fn to_static_codegen(&self) -> String {
        let instructions =
            self.program.instructions.iter().map(|i| i.codegen()).collect::<Vec<_>>()
            .join(", ");

        format!("::automata::regexp::vm::instance::StaticRegExp::from_compilation(&[{}])", instructions)
    }
}

pub struct RegExpMatch<'a, 'b> {
    instructions: &'a [Instruction],

    /// Full original input given to exec().
    input: &'b [u8],

    /// Byte offset at which this match begins
    index: usize,

    /// Byte offset at which the last match ended.
    /// We will look for the next match starting with the character at this position.
    last_index: usize,
    
    /// String pointers recorded for the last match.
    string_pointers: SavedStringPointers
}

impl<'a, 'b> RegExpMatch<'a, 'b> {
    pub fn next(mut self) -> Option<Self> {
        let mut executor = Executor::new(self.instructions);
        let string_pointers = match executor.run(self.input, self.last_index) {
            Some(v) => v,
            None => { return None; }
        };

        // TODO: Need to avoid infinite matches.

        // Add the offset 
        self.index = string_pointers.list[0].unwrap();
        self.last_index = string_pointers.list[1].unwrap();
        self.string_pointers = string_pointers;

        Some(self)
    }

    /// Byte offset at which the match begins.
    pub fn index(&self) -> usize { self.index }

    /// Byte offset at which the match ends.
    pub fn last_index(&self) -> usize { self.last_index }

    /// NOTE: Group 0 will always contain the complete match.
    pub fn group(&self, i: usize) -> Option<&'b [u8]> {
        let start = self.string_pointers.list.get(2*i).and_then(|v| *v);
        let end = self.string_pointers.list.get(2*i + 1).and_then(|v| *v);

        if let Some(start) = start {
            if let Some(end) = end {
                return Some(&self.input[start..end]);
            }
        }

        None
    }

    pub fn group_str(&self, i: usize) -> Option<Result<&str>> {
        if let Some(input) = self.group(i) {
            // TODO: This is only possible if we use a UTF-8 compatible encoding.
            return Some(std::str::from_utf8(input)
                .map_err(|e| Error::from(e)));
        }

        None
    }

    // TODO: Support lookup by name.
}

pub struct RegExpSplitIterator<'a, 'b> {
    input: &'b str,

    current_index: usize,

    last_match: Option<RegExpMatch<'a, 'b>>,

    final_emitted: bool
}

impl<'a, 'b> std::iter::Iterator for RegExpSplitIterator<'a, 'b> {
    type Item = &'b str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.final_emitted {
            return None;
        }

        let end_index;
        let next_current_index;

        if let Some(m) = self.last_match.take() {
            end_index = m.index();
            next_current_index = m.last_index();
            self.last_match = m.next();
        } else {
            end_index = self.input.len();
            next_current_index = self.input.len();
            self.final_emitted = true;
        }

        let value = &self.input[self.current_index..end_index];
        self.current_index = next_current_index;

        Some(value)
    }
}


/// Pre-compiled regular expression.
pub struct StaticRegExp {
    instructions: &'static [Instruction]
}

impl StaticRegExp {
    pub const fn from_compilation(instructions: &'static [Instruction]) -> Self {
        Self { instructions }
    }

    pub fn test<T: AsRef<[u8]>>(&self, input: T) -> bool {
        let mut executor = Executor::new(&self.instructions);
        let results = executor.run(input.as_ref(), 0);
        results.is_some()
    }

    pub fn exec<'a, 'b, T: 'b + AsRef<[u8]> + ?Sized>(&'a self, input: &'b T) -> Option<RegExpMatch<'a, 'b>> {
        RegExp::exec_impl(&self.instructions, input.as_ref())
    }

    pub fn split<'a, 'b>(&'a self, input: &'b str) -> RegExpSplitIterator<'a, 'b> {
        RegExpSplitIterator {
            input,
            current_index: 0,
            last_match: self.exec(input),
            final_emitted: false
        }
    }

    pub fn estimated_memory_usage(&self) -> usize {
        std::mem::size_of::<Instruction>() * self.instructions.len()
    }
}