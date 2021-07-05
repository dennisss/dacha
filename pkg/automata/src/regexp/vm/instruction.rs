use std::collections::HashMap;

use common::errors::*;

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

    /// Similar to 'Char', except matches a special symbol. If a regular character value is seen
    /// instead, this instruction will terminate the current thread.
    Special(SpecialSymbol),

    /// When executed, indicates that the matching is complete.
    Match,

    Jump(ProgramCounter),

    /// Schedules execution on the next input at two different program counters in parallel.
    ///
    /// Effectively this schedules two threads. The first thread is considered to be
    /// 'higher priority' and will execute first and matches from this thread will be preferred to
    /// matches from the second thread. 
    ///
    /// TODO: Does this really need two pointers. It will always be the next instruction + 1 other.
    Split(ProgramCounter, ProgramCounter),

    /// Saves the current position of the string to the string pointers list for current thread.
    ///
    /// TODO: Reduce to u16 as we almost never need that many groups.
    Save {
        /// The index into the string pointers list at which to store the position.
        index: usize,
    
        /// If true, instead of storing the current position, we will store the position
        /// immediately before the last input value.
        lookbehind: bool
    },
}

impl Instruction {
    pub fn assembly(&self) -> String {
        match self {
            Instruction::Any => format!("any"),
            Instruction::Range { start, end } => {
                format!("range {} - {}", RegExpSymbol::debug_offset(*start), RegExpSymbol::debug_offset(*end))
            },
            Instruction::Char(v) => format!("char {}", RegExpSymbol::debug_offset(*v)),
            Instruction::Special(v) => format!("special {:?}", v),
            Instruction::Match => format!("match"),
            Instruction::Jump(index) => format!("jump {}", index),
            Instruction::Split(a, b) => format!("split {}, {}", a, b),
            Instruction::Save { index, lookbehind } => format!("save {} {}", index, lookbehind)
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

pub trait Program {
    /// Retrieves a single instruction from the program at a given position.
    ///
    /// Returns the fetched instruction and a pointer to the next instruction.
    fn fetch(&self, pc: ProgramCounter) -> (Instruction, ProgramCounter);

    fn size_of(&self) -> usize;
}

/// A simple program which just uses a dynamic Vec to store instructions.
pub struct VecProgram {
    instructions: Vec<Instruction>
}

impl VecProgram {
    pub fn new() -> Self {
        Self { instructions: vec![] }
    }

    pub fn as_referenced_program(&self) -> ReferencedProgram {
        ReferencedProgram::new(&self.instructions)
    }
}

impl std::ops::Deref for VecProgram {
    type Target = Vec<Instruction>;
    fn deref(&self) -> &Self::Target {
        &self.instructions
    }
}

impl std::ops::DerefMut for VecProgram {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.instructions        
    }
}

impl Program for VecProgram {
    fn fetch(&self, pc: ProgramCounter) -> (Instruction, ProgramCounter) {
        (self.instructions[pc as usize].clone(), pc + 1)
    }

    fn size_of(&self) -> usize {
        self.as_referenced_program().size_of()
    }
}

#[derive(Clone, Copy)]
pub struct ReferencedProgram<'a> {
    instructions: &'a [Instruction]
}

impl<'a> ReferencedProgram<'a> {
    pub const fn new(instructions: &[Instruction]) -> ReferencedProgram {
        ReferencedProgram { instructions }
    }
}

impl<'a> Program for ReferencedProgram<'a> {
    fn fetch(&self, pc: ProgramCounter) -> (Instruction, ProgramCounter) {
        (self.instructions[pc as usize].clone(), pc + 1)
    }

    fn size_of(&self) -> usize {
        std::mem::size_of::<Instruction>() * self.instructions.len()
    }
}


/*
pub struct PackedProgram {
    data: Vec<u8>
}

impl PackedProgram {
    pub fn pack(program: ReferencedProgram) -> Self {
        let mut pc_map = vec![];
        let mut out = vec![];

        for (i, instruction) in program.instructions.iter() {
            pc_map.push(out.len());


        }


    }
}

enum_def!(InstructionType u8 => 
    Any = 0,
    Range = 1,
    Char = 2,
    Special = 3,
    Match = 4,
    Jump = 5,
    Split = 6,
    Save = 7
);
*/