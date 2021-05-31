use crate::regexp::symbol::RegExpSymbol;


pub type StringPointer = usize;

/// TODO: Pick the min(u32, usize)
pub type ProgramCounter = u32;

pub type CharacterValue = u32;

/// The discrete input value/token/symbol on which the VM operates during a single step.
pub enum InputValue {
    /// A real non-zero length character value (represents >=1 bytes in the input search string).
    Character(CharacterValue),

    /// Special zero length symbol that marks a special place in the input search string.
    Special(SpecialSymbol)
}

/// A special zero length input symbol.
///
/// This can be matched used the Special(_) instruction. Other regular character matching
/// instructions will skip over these symbols until the first non zero length character is
/// seen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpecialSymbol {
    StartOfString,
    EndOfString
}

/// NOTE: A u32 character type is used to be compatible with the RegExpSymbol struct. 
#[derive(Clone, Debug)]
pub enum Instruction {
    /// Match any character (excluding special symbols).
    Any,

    /// Matches any character in the range [start, end).
    Range { start: CharacterValue, end: CharacterValue },

    /// Matches a character that contains exactly a single value.
    Char(CharacterValue),

    /// Without consuming any inputs, verifies that the current character is the
    /// given value and just procedes to the next instruction.
    ///
    /// TODO: Support lookahead of a Range
    Lookahead(CharacterValue),

    /// Similar to 'Char', except matches a special symbol. If a regular character value is seen
    /// instead, this instruction will terminate the current thread.
    Special(SpecialSymbol),

    /// When executed, indicates that the matching is complete.
    Match,

    Jump(ProgramCounter),

    /// 
    ///
    /// TODO: Does this really need two pointers. It will always be the next instruction + 1 other.
    Split(ProgramCounter, ProgramCounter),

    /// Saves the current position of the string to the given index in a list of string pointers
    /// in the current thread.
    ///
    /// TODO: Reduce to u16 as we almost never need that many groups.
    Save(usize),
}

impl Instruction {
    pub fn assembly(&self) -> String {
        match self {
            Instruction::Any => format!("any"),
            Instruction::Range { start, end } => {
                format!("range {} - {}", RegExpSymbol::debug_offset(*start), RegExpSymbol::debug_offset(*end))
            },
            Instruction::Char(v) => format!("char {}", RegExpSymbol::debug_offset(*v)),
            Instruction::Lookahead(v) => format!("lookahead {}", RegExpSymbol::debug_offset(*v)),
            Instruction::Special(v) => format!("special {:?}", v),
            Instruction::Match => format!("match"),
            Instruction::Jump(index) => format!("jump {}", index),
            Instruction::Split(a, b) => format!("split {}, {}", a, b),
            Instruction::Save(index) => format!("save {}", index)
        }
    }

    pub fn codegen(&self) -> String {
        if let Instruction::Special(s) = self {
            return format!("::automata::regexp::vm::instruction::Instruction::Special(
                ::automata::regexp::vm::instruction::SpecialSymbol::{:?})", s);

        }

        format!("::automata::regexp::vm::instruction::Instruction::{:?}", self)
    }
}
