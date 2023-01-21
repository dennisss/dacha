use common::errors::*;

use crate::regexp::node::*;
use crate::regexp::vm::compiler::*;
use crate::regexp::vm::executor::*;
use crate::regexp::vm::instruction::*;

pub struct RegExp {
    compilation: Compilation,
}

impl RegExp {
    pub fn new(expr: &str) -> Result<Self> {
        // TODO: Don't add a '.*?' if the expression begins with a '^'
        let root = RegExpNode::parse(&format!(".*?({})", expr))?;
        let compilation = Compiler::compile(&root)?;
        Ok(Self { compilation })
    }

    /// Returns true if and only if a match for the given regular expression is
    /// found somewhere in the given input string.
    pub fn test<T: AsRef<[u8]>>(&self, input: T) -> bool {
        let mut executor = Executor::new(self.compilation.program.as_referenced_program());
        let results = executor.run(input.as_ref(), 0);
        results.is_some()
    }

    pub fn exec<'a, 'b, T: 'b + AsRef<[u8]> + ?Sized>(
        &'a self,
        input: &'b T,
    ) -> Option<RegExpMatch<'b, ReferencedProgram<'a>>> {
        Self::exec_impl(
            self.compilation.program.as_referenced_program(),
            input.as_ref(),
        )
    }

    fn exec_impl<'a, P: Program + Copy>(program: P, input: &'a [u8]) -> Option<RegExpMatch<'a, P>> {
        let state = RegExpMatch {
            program,
            input,
            index: 0,
            last_index: 0,
            string_pointers: SavedStringPointers::default(),
        };

        state.next()
    }

    /// NOTE: Only meant for usage in the 'regexp_macros' package.
    pub fn to_static_codegen(&self) -> String {
        let instructions = self
            .compilation
            .program
            .iter()
            .map(|i| i.codegen())
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "::automata::regexp::vm::instance::StaticRegExp::from_compilation(&[{}])",
            instructions
        )
    }
}

pub struct RegExpMatch<'a, P> {
    program: P,

    /// Full original input given to exec().
    input: &'a [u8],

    /// Byte offset at which this match begins
    index: usize,

    /// Byte offset at which the last match ended.
    /// We will look for the next match starting with the character at this
    /// position.
    last_index: usize,

    /// String pointers recorded for the last match.
    string_pointers: SavedStringPointers,
}

impl<'a, P: Program + Copy> RegExpMatch<'a, P> {
    pub fn next(mut self) -> Option<Self> {
        let mut executor = Executor::new(self.program);
        let string_pointers = match executor.run(self.input, self.last_index) {
            Some(v) => v,
            None => {
                return None;
            }
        };

        // TODO: Need to avoid infinite matches.

        // Add the offset
        self.index = string_pointers.list[0].unwrap();
        self.last_index = string_pointers.list[1].unwrap();
        self.string_pointers = string_pointers;

        Some(self)
    }

    /// Byte offset at which the match begins.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Byte offset at which the match ends.
    pub fn last_index(&self) -> usize {
        self.last_index
    }

    /// NOTE: Group 0 will always contain the complete match.
    pub fn group(&self, i: usize) -> Option<&'a [u8]> {
        let start = self.string_pointers.list.get(2 * i).and_then(|v| *v);
        let end = self.string_pointers.list.get(2 * i + 1).and_then(|v| *v);

        if let Some(start) = start {
            if let Some(end) = end {
                return Some(&self.input[start..end]);
            }
        }

        None
    }

    pub fn group_str(&self, i: usize) -> Option<Result<&'a str>> {
        if let Some(input) = self.group(i) {
            // TODO: This is only possible if we use a UTF-8 compatible encoding.
            return Some(std::str::from_utf8(input).map_err(|e| Error::from(e)));
        }

        None
    }

    // TODO: Support lookup by name.
}

pub struct RegExpSplitIterator<'a, P> {
    input: &'a str,

    current_index: usize,

    last_match: Option<RegExpMatch<'a, P>>,

    final_emitted: bool,
}

impl<'a, P: Program + Copy> std::iter::Iterator for RegExpSplitIterator<'a, P> {
    type Item = &'a str;

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

pub type StaticRegExpMatch<'a, 'b> = RegExpMatch<'b, ReferencedProgram<'a>>;

/// Pre-compiled regular expression.
pub struct StaticRegExp {
    program: ReferencedProgram<'static>,
}

impl StaticRegExp {
    pub const fn from_compilation(instructions: &'static [Instruction]) -> Self {
        Self {
            program: ReferencedProgram::new(instructions),
        }
    }

    pub fn test<T: AsRef<[u8]>>(&self, input: T) -> bool {
        let mut executor = Executor::new(self.program);
        let results = executor.run(input.as_ref(), 0);
        results.is_some()
    }

    pub fn exec<'a, 'b, T: 'b + AsRef<[u8]> + ?Sized>(
        &'a self,
        input: &'b T,
    ) -> Option<RegExpMatch<'b, ReferencedProgram<'a>>> {
        RegExp::exec_impl(self.program, input.as_ref())
    }

    pub fn split<'a, 'b>(
        &'a self,
        input: &'b str,
    ) -> RegExpSplitIterator<'b, ReferencedProgram<'a>> {
        RegExpSplitIterator {
            input,
            current_index: 0,
            last_match: self.exec(input),
            final_emitted: false,
        }
    }

    pub fn estimated_memory_usage(&self) -> usize {
        self.program.size_of()
    }
}
