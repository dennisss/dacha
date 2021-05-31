use std::rc::Rc;
use std::collections::HashSet;

use crate::regexp::vm::instruction::*;

/// Helper for performing a single step of the executor given the current input and position.
macro_rules! step {
    ($executor:ident, $input_value:expr, $i:expr) => {
        match $executor.step($input_value, $i) {
            ExecutorStepResult::Matched(v) => {
                return Some(v);
            },
            ExecutorStepResult::NeedMoreInput => {},
            ExecutorStepResult::Terminated => { return None; }
        };
    };
}

/// Helper for getting the next regular character value while executing the VM. Either the current
/// input value is a character or we will enqueue a retry at the same program counter next time the
/// VM is run. 
macro_rules! next_character {
    ($value:ident, $thread:ident, $next_threads:ident) => {
        match $value {
            InputValue::Character(v) => v,
            InputValue::Special(_) => {
                // Retry on next input
                $next_threads.add_thread($thread.pc, $thread.saved.clone());
                continue;
            }
        }
    };
}


pub struct Executor<'a> {
    instructions: &'a [Instruction],

    thread_list_a: ThreadList,
    thread_list_b: ThreadList,
    thread_list_a_active: bool,
}

impl<'a> Executor<'a> {
    pub fn new(instructions: &'a [Instruction]) -> Self {
        let mut thread_list_a = ThreadList::new();
        let thread_list_b = ThreadList::new();

        // Add initial thread
        // TODO: Use references for program counters to avoid bounds checks.
        thread_list_a.add_thread(0, Rc::new(SavedStringPointers::default()));

        Self {
            instructions,
            thread_list_a,
            thread_list_a_active: true,
            thread_list_b
        }
    }

    /// NOTE: This will execute the program with START and END symbols inserted before and after
    /// the inputs.
    pub fn run(&mut self, input: &[u8], start_index: usize /* , encoding: CharacterEncoding */) -> Option<SavedStringPointers> {
        let mut i = start_index;

        // TODO: Deal with infinite regular expressions.
        if i == 0 {
            step!(self, InputValue::Special(SpecialSymbol::StartOfString), i);
        }

        while i < input.len() {
            let value = input[i] as u32;
            step!(self, InputValue::Character(value), i);
            i += 1;
        }

        // TODO: Only perform this if there actually inputs?
        step!(self, InputValue::Special(SpecialSymbol::EndOfString), i);

        None
    }

    /// Runs the VM on one input character value.
    ///
    /// It should be noted that this will always trigger a match after consuming one more character
    /// after the end of the match string.
    ///
    /// Returns whether or not a match was found. One this matches or terminates, it is invalid
    /// to call step() any more (a new Executor should be created if execution is required on
    /// fresh inputs).
    pub fn step(
        &mut self, value: InputValue, input_position: StringPointer
    ) -> ExecutorStepResult {
        let mut current_threads = &mut self.thread_list_a;
        let mut next_threads = &mut self.thread_list_b;
        if !self.thread_list_a_active {
            std::mem::swap(&mut current_threads, &mut next_threads);
        }
        self.thread_list_a_active = !self.thread_list_a_active;
        next_threads.clear();


        let mut thread_i = 0;
        while thread_i < current_threads.list.len() {
            let thread = &current_threads.list[thread_i];
            thread_i += 1;

            match self.instructions[thread.pc as usize] {
                Instruction::Any => {
                    let _char_value = next_character!(value, thread, next_threads);
                    next_threads.add_thread(thread.pc + 1, thread.saved.clone());
                }
                Instruction::Range { start, end } => {
                    let char_value = next_character!(value, thread, next_threads);
                    if char_value >= start && char_value < end {
                        next_threads.add_thread(thread.pc + 1, thread.saved.clone());
                    }
                }
                Instruction::Char(expected_value) => {
                    let char_value = next_character!(value, thread, next_threads);
                    if char_value == expected_value {
                        next_threads.add_thread(thread.pc + 1, thread.saved.clone());
                    }
                }
                Instruction::Lookahead(expected_value) => {
                    let char_value = next_character!(value, thread, next_threads);
                    if char_value == expected_value {
                        // TODO: Can we safely mutate the current thread instead of making a new one?
                        
                        let pc = thread.pc + 1;
                        let saved = thread.saved.clone();
                        current_threads.add_thread(pc, saved);
                    }
                }
                Instruction::Special(expected_symbol) => {
                    let symbol_value = match value {
                        InputValue::Special(s) => s,
                        InputValue::Character(_) => {
                            // Terminate this thread.
                            continue;
                        }
                    };

                    if symbol_value == expected_symbol {
                        let pc = thread.pc + 1;
                        let saved = thread.saved.clone();
                        current_threads.add_thread(pc, saved);
                    }
                }
                Instruction::Match => {
                    return ExecutorStepResult::Matched(thread.saved.as_ref().clone());
                }
                Instruction::Jump(next_pc) => {
                    let saved = thread.saved.clone();
                    current_threads.add_thread(next_pc, saved);
                }
                Instruction::Split(pc1, pc2) => {
                    let saved = thread.saved.clone();
                    current_threads.add_thread(pc1, saved.clone());
                    current_threads.add_thread(pc2, saved);
                }
                Instruction::Save(index) => {
                    // TODO: For regular expressions such as '.*(a)', this will run on every input
                    // byte so we need to see if we can reduce the number of copies that this
                    // requires (we'd need to gurantee that no other threads will need this ).
                    let mut saved = thread.saved.as_ref().clone();
                    if saved.list.len() <= index {
                        saved.list.resize(index + 1, None);
                    }
                    saved.list[index] = Some(input_position);

                    let next_pc = thread.pc + 1;

                    // TODO: the new value of PC is not the same as in the reference material.
                    current_threads.add_thread(next_pc, Rc::new(saved));
                }
            }
        }

        if next_threads.list.len() > 0 {
            ExecutorStepResult::NeedMoreInput
        } else {
            ExecutorStepResult::Terminated
        }
    }
}

#[derive(Clone, Default)]
pub struct SavedStringPointers {
    pub list: Vec<Option<StringPointer>>
}

pub enum ExecutorStepResult {
    /// We successfully matched some substring. The saved pointers are provided as recorded
    /// from the last running thread.
    Matched(SavedStringPointers),

    /// We haven't matched yet, but may match if given more inputs.
    NeedMoreInput,

    /// The program has halted and we will never match regardless of how many inputs are given.
    Terminated
}


struct Thread {
    pc: ProgramCounter,
    saved: Rc<SavedStringPointers>
}

/// NOTE: In a program with N instructions, there should only ever be at most N threads.
struct ThreadList {
    list: Vec<Thread>,

    /// Used to deduplicate threads which are added with the same program counter as an existing
    /// thread.
    ///
    /// NOTE: We don't just store the threads in a HashMap to ensure that 
    seen_pcs: HashSet<ProgramCounter>
}

impl ThreadList {
    fn new() -> Self {
        Self { list: vec![], seen_pcs: HashSet::new() }
    }

    fn clear(&mut self) {
        self.list.clear();
        self.seen_pcs.clear();
    }

    fn add_thread(&mut self, pc: ProgramCounter, saved: Rc<SavedStringPointers>) {
        if self.seen_pcs.contains(&pc) {
            return;
        }

        self.seen_pcs.insert(pc);
        self.list.push(Thread {
            pc, saved
        });
    }

    fn iter(&self) -> impl std::iter::Iterator<Item=&Thread> {
        self.list.iter()
    }
}


