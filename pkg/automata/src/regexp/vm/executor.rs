use std::collections::HashSet;
use std::rc::Rc;

use common::bit_set::BitSet;
use common::hash::FastHasherBuilder;

use crate::regexp::vm::flags::Flags;
use crate::regexp::vm::instruction::*;
use crate::regexp::vm::string_pointers::SavedStringPointers;

/// Helper for performing a single step of the executor given the current input
/// and position.
macro_rules! step {
    ($executor:ident, $input_value:expr, $i:expr, $j:expr) => {
        match $executor.step($input_value, $i, $j) {
            ExecutorStepResult::Matched(v) => {
                return Some(v);
            }
            ExecutorStepResult::NeedMoreInput => {}
            ExecutorStepResult::Terminated => {
                return None;
            }
        };
    };
}

/// Helper for getting the next regular character value while executing the VM.
/// Either the current input value is a character or we will enqueue a retry at
/// the same program counter next time the VM is run.
macro_rules! next_character {
    ($value:ident, $thread:ident, $next_threads:expr) => {
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

pub struct Executor<P> {
    program: P,

    best_match: Option<Rc<SavedStringPointers>>,

    thread_list_a: ThreadList,
    thread_list_b: ThreadList,
    thread_list_a_active: bool,
}

impl<P: Program + Copy> Executor<P> {
    pub fn new(program: P) -> Self {
        let mut inst = Self {
            program,
            best_match: None,
            thread_list_a: ThreadList::new(program.len()),
            thread_list_a_active: true,
            thread_list_b: ThreadList::new(program.len()),
        };

        // Add initial thread
        // TODO: Use references for program counters to avoid bounds checks.
        let mut step_state = ExecutorStepState {
            program: inst.program,
            input_position: 0,
            next_position: 0,
            next_threads: &mut inst.thread_list_a,
        };
        step_state.schedule_thread(0, Rc::new(SavedStringPointers::default()));

        inst
    }

    /// NOTE: This will execute the program with START and END symbols inserted
    /// before and after the inputs.
    pub fn run(
        &mut self,
        input: &[u8],
        start_index: usize, /* , encoding: CharacterEncoding */
    ) -> Option<SavedStringPointers> {
        let mut i = start_index;

        // TODO: Deal with infinite regular expressions.
        if i == 0 {
            step!(
                self,
                InputValue::Special(SpecialSymbol::StartOfString),
                i,
                i
            );
        }

        while i < input.len() {
            let value = input[i] as u32;
            step!(self, InputValue::Character(value), i, i + 1);
            i += 1;
        }

        // TODO: Only perform this if there actually inputs?
        step!(self, InputValue::Special(SpecialSymbol::EndOfString), i, i);

        // Required in order to run any Match instructions immediately after a $ symbol.
        self.final_step();

        // If we reached the end of the input, take the best match so far.
        // This handles the case of performing a greedy match has accepted up to the
        // end of the string and wants to accept more if available.
        if let Some(m) = self.best_match.take() {
            return Some(m.as_ref().clone());
        }

        None
    }

    /// Runs the VM on one input character value.
    ///
    /// It should be noted that this will always trigger a match after consuming
    /// one more character after the end of the match string.
    ///
    /// Returns whether or not a match was found. One this matches or
    /// terminates, it is invalid to call step() any more (a new Executor
    /// should be created if execution is required on fresh inputs).
    fn step(
        &mut self,
        value: InputValue,
        input_position: StringPointer,
        next_position: StringPointer,
    ) -> ExecutorStepResult {
        let mut current_threads = &mut self.thread_list_a;
        let mut next_threads = &mut self.thread_list_b;
        if !self.thread_list_a_active {
            std::mem::swap(&mut current_threads, &mut next_threads);
        }
        self.thread_list_a_active = !self.thread_list_a_active;
        next_threads.clear();

        let mut state = ExecutorStepState {
            program: self.program,
            input_position,
            next_position,
            next_threads,
        };

        for thread in current_threads.drain() {
            let (op, next_pc) = self.program.fetch(thread.pc);
            match op {
                Instruction::Any => {
                    let _char_value = next_character!(value, thread, state.next_threads);
                    state.schedule_thread(next_pc, thread.saved);
                }
                Instruction::Range { start, end } => {
                    let mut char_value = next_character!(value, thread, state.next_threads);

                    // TODO: Deduplication this.
                    if self.program.flags().contains(Flags::CASE_INSENSITIVE) {
                        // TODO: Make the input value u8
                        char_value = (char_value as u8).to_ascii_lowercase() as u32;
                    }

                    if char_value >= start && char_value < end {
                        state.schedule_thread(next_pc, thread.saved);
                    }
                }
                Instruction::Char(expected_value) => {
                    let mut char_value = next_character!(value, thread, state.next_threads);

                    // TODO: Deduplication this.
                    if self.program.flags().contains(Flags::CASE_INSENSITIVE) {
                        // TODO: Make the input value u8
                        char_value = (char_value as u8).to_ascii_lowercase() as u32;
                    }

                    if char_value == expected_value {
                        state.schedule_thread(next_pc, thread.saved);
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
                        let saved = thread.saved;
                        state.schedule_thread(next_pc, saved);
                    }
                }
                Instruction::LUT { index } => {
                    let lut = state.program.fetch_lut(index).unwrap();

                    let char_value = next_character!(value, thread, state.next_threads);
                    if lut.contains(char_value as u8) {
                        state.schedule_thread(next_pc, thread.saved);
                    }
                }
                Instruction::Match => {
                    // Save the match. This may be overriden if a higher priority thread later
                    // finds an alternative match.
                    self.best_match = Some(thread.saved.clone());

                    // Skip executing lower priority threads.
                    break;
                }

                // These are handled in the schedule_thread code.
                Instruction::Split(_, _) | Instruction::Save { .. } | Instruction::Jump(_) => {
                    panic!()
                }
            }
        }

        if next_threads.list.len() > 0 {
            ExecutorStepResult::NeedMoreInput
        } else if let Some(saved) = &self.best_match {
            // TODO: It shouldn't require a copy as all threads should now be dead?
            ExecutorStepResult::Matched(saved.as_ref().clone())
        } else {
            ExecutorStepResult::Terminated
        }
    }

    fn final_step(&mut self) {
        let mut current_threads = &mut self.thread_list_a;
        let mut next_threads = &mut self.thread_list_b;
        if !self.thread_list_a_active {
            std::mem::swap(&mut current_threads, &mut next_threads);
        }

        for thread in &current_threads.list {
            if let (Instruction::Match, _) = self.program.fetch(thread.pc) {
                self.best_match = Some(thread.saved.clone());
                break;
            }
        }
    }
}

struct ExecutorStepState<'a, P> {
    // TODO: Consider using a ReferencedProgram type to minimize the indirection.
    program: P,
    input_position: StringPointer,
    next_position: StringPointer,
    next_threads: &'a mut ThreadList,
}

impl<'a, P: Program> ExecutorStepState<'a, P> {
    // NOTE: 'pc' is the next instruction to execute
    fn schedule_thread(&mut self, pc: ProgramCounter, mut saved: Rc<SavedStringPointers>) {
        let (op, next_pc) = self.program.fetch(pc);
        match op {
            Instruction::Jump(next_pc) => {
                // TODO: It may still be useful to mark 'pc' as included now so that other jumps
                // to the same location don't have to perform this same
                // traversal.

                self.schedule_thread(next_pc, saved);
            }
            Instruction::Split(pc1, pc2) => {
                self.schedule_thread(pc1, saved.clone());
                self.schedule_thread(pc2, saved);
            }
            Instruction::Save { index, lookbehind } => {
                let index = index as usize;

                // TODO: For regular expressions such as '.*(a)', this will run on every input
                // byte so we need to see if we can reduce the number of copies that this
                // requires (we'd need to gurantee that no other threads will need this ).
                let saved_mut = Rc::make_mut(&mut saved);

                saved_mut.set(
                    index,
                    if lookbehind {
                        self.input_position
                    } else {
                        self.next_position
                    },
                );

                // TODO: the new value of PC is not the same as in the reference material.
                self.schedule_thread(next_pc, saved);
            }
            _ => {
                self.next_threads.add_thread(pc, saved);
            }
        }
    }
}

pub enum ExecutorStepResult {
    /// We successfully matched some substring. The saved pointers are provided
    /// as recorded from the last running thread.
    Matched(SavedStringPointers),

    /// We haven't matched yet, but may match if given more inputs.
    NeedMoreInput,

    /// The program has halted and we will never match regardless of how many
    /// inputs are given.
    Terminated,
}

struct Thread {
    pc: ProgramCounter,
    saved: Rc<SavedStringPointers>,
}

/// NOTE: In a program with N instructions, there should only ever be at most N
/// threads.
struct ThreadList {
    /// NOTE: These are in order from highest to lowest priority.
    list: Vec<Thread>,

    /// Used to deduplicate threads which are added with the same program
    /// counter as an existing thread.
    ///
    /// TODO: We don't need to have this in both thread_list_a and thread_list_b
    seen_pcs: BitSet,
}

impl ThreadList {
    fn new(program_len: usize) -> Self {
        let seen_pcs = BitSet::new(program_len);

        Self {
            list: vec![],
            seen_pcs,
        }
    }

    fn clear(&mut self) {
        self.list.clear();
        self.seen_pcs.clear();
    }

    fn add_thread(&mut self, pc: ProgramCounter, saved: Rc<SavedStringPointers>) {
        if self.seen_pcs.contains(pc as usize) {
            return;
        }

        self.seen_pcs.insert(pc as usize);
        self.list.push(Thread { pc, saved });
    }

    fn drain<'a>(&'a mut self) -> impl std::iter::Iterator<Item = Thread> + 'a {
        self.list.drain(..)
    }

    fn iter(&self) -> impl std::iter::Iterator<Item = &Thread> {
        self.list.iter()
    }
}
