use std::collections::HashMap;
use std::fmt::Write;
use std::iter::Iterator;

use common::errors::*;

use crate::regexp::node::*;
use crate::regexp::symbol::invert_symbols;
use crate::regexp::vm::instruction::*;

pub struct Compilation {
    pub program: VecProgram,

    /// For each captured group in the original expression, this contains the
    /// two indices of the string pointers used to save the start and end
    /// index of the group during execution.
    pub groups: Vec<(usize, usize)>,

    /// Map of group names to the index of the group in the 'groups' list.
    pub groups_by_name: HashMap<String, usize>,
}

impl Compilation {
    pub fn assembly(&self) -> String {
        let mut out = String::new();
        for i in 0..self.program.len() {
            write!(&mut out, "{:3}: {}\n", i, self.program[i].assembly()).unwrap();
        }

        out
    }

    /// Retrieves an iterator over all references to program counters in the
    /// instructions.
    fn iter_referenced_pcs(&mut self) -> impl Iterator<Item = &mut ProgramCounter> {
        self.program
            .iter_mut()
            .flat_map(|inst| {
                let slice: [Option<&mut ProgramCounter>; 2] = match inst {
                    Instruction::Any
                    | Instruction::Range { .. }
                    | Instruction::Char(_)
                    | Instruction::Match
                    | Instruction::Special(_)
                    | Instruction::Save { .. } => [None, None],

                    Instruction::Jump(x) => [Some(x), None],
                    Instruction::Split(x, y) => [Some(x), Some(y)],
                };

                slice
            })
            .filter_map(|v| v)
    }
}

/// Compiler for converting a RegExpNode to a Program that can be executed.
pub struct Compiler {
    output: Compilation,
    next_string_index: usize,
}

impl Compiler {
    pub fn compile(root: &RegExpNode) -> Result<Compilation> {
        let mut inst = Self {
            output: Compilation {
                program: VecProgram::new(),
                groups: vec![],
                groups_by_name: HashMap::new(),
            },
            next_string_index: 0,
        };

        inst.compile_node(root)?;
        inst.add_instruction(Instruction::Match);
        inst.optimize();

        // TODO: Validate that there are no infinite loops (e.g. jump to self or any
        // other continous sequence of non-input consuming instructions)

        Ok(inst.output)
    }

    fn add_instruction(&mut self, instruction: Instruction) {
        self.output.program.push(instruction);
    }

    fn program_counter(&self) -> ProgramCounter {
        self.output.program.len() as ProgramCounter
    }

    // TODO: For any sub-expressions that don't contain contain captured groups, we
    // can convert it first to a DFA to simplify the expression before
    // generating the instructions.
    fn compile_node(&mut self, node: &RegExpNode) -> Result<()> {
        match node {
            RegExpNode::Start => {
                self.add_instruction(Instruction::Special(SpecialSymbol::StartOfString));
            }
            RegExpNode::End => {
                self.add_instruction(Instruction::Special(SpecialSymbol::EndOfString));
            }

            RegExpNode::Literal(c) => {
                // TODO: Check that everything is only ASCII for now?

                if let Char::Wildcard = c {
                    self.add_instruction(Instruction::Any);
                    return Ok(());
                }

                // TODO: These may contain overlap. Need to consolidate them. (ideally that
                // would be done generically on all alternations).
                let symbols = c.raw_symbols();

                self.compile_alternation(&symbols, |c, sym| {
                    if sym.end == sym.start + 1 {
                        c.add_instruction(Instruction::Char(sym.start));
                    } else {
                        c.add_instruction(Instruction::Range {
                            start: sym.start,
                            end: sym.end,
                        });
                    }

                    Ok(())
                })?;
            }
            RegExpNode::Alt(nodes) => {
                self.compile_alternation(&nodes, |c, node| c.compile_node(node))?;
            }
            RegExpNode::Expr(nodes) => {
                for node in nodes {
                    self.compile_node(node)?;
                }
            }
            RegExpNode::Quantified {
                node,
                quantifier,
                greedy,
            } => {
                match quantifier {
                    Quantifier::ZeroOrOne => {
                        // a?
                        let split_idx = self.program_counter();
                        self.add_instruction(Instruction::Split(0, 0)); // Placeholder.

                        self.compile_node(node)?;

                        let a = split_idx + 1;
                        let b = self.program_counter();

                        self.output.program[split_idx as usize] = if *greedy {
                            Instruction::Split(a, b)
                        } else {
                            Instruction::Split(b, a)
                        };
                    }
                    Quantifier::ZeroOrMore => {
                        let split_idx = self.program_counter();
                        self.add_instruction(Instruction::Split(0, 0)); // Placeholder.

                        self.compile_node(node)?;

                        self.add_instruction(Instruction::Jump(split_idx));

                        let a = split_idx + 1;
                        let b = self.program_counter();

                        self.output.program[split_idx as usize] = if *greedy {
                            Instruction::Split(a, b)
                        } else {
                            Instruction::Split(b, a)
                        };
                    }
                    Quantifier::OneOrMore => {
                        let start_idx = self.program_counter();
                        self.compile_node(node)?;

                        let a = start_idx;
                        let b = self.program_counter() + 1;
                        self.add_instruction(if *greedy {
                            Instruction::Split(a, b)
                        } else {
                            Instruction::Split(b, a)
                        });
                    }
                    Quantifier::ExactlyN(num) => {
                        // TODO: Instead implement support for counters in the state.
                        for _ in 0..*num {
                            self.compile_node(node)?;
                        }
                    }
                    _ => {
                        println!("Unsupported quantifier: {:?}", quantifier);
                    }
                }
            }
            RegExpNode::Capture {
                inner,
                capturing,
                name,
            } => {
                if *capturing {
                    let start_idx = self.next_string_index;
                    self.next_string_index += 1;

                    let end_idx = self.next_string_index;
                    self.next_string_index += 1;

                    let group_idx = self.output.groups.len();
                    self.output.groups.push((start_idx, end_idx));

                    if !name.is_empty() {
                        self.output
                            .groups_by_name
                            .insert(name.to_string(), group_idx);
                    }

                    self.add_instruction(Instruction::Save {
                        index: start_idx,
                        lookbehind: false,
                    });
                    self.compile_node(inner)?;
                    self.add_instruction(Instruction::Save {
                        index: end_idx,
                        lookbehind: false,
                    });
                } else {
                    self.compile_node(inner)?;
                }
            }
            RegExpNode::Class { chars, inverted } => {
                let mut symbols = vec![];
                for c in chars {
                    symbols.extend(c.raw_symbols());
                }

                if *inverted {
                    symbols = invert_symbols(symbols);
                }

                // TODO: Deduplicate with the ::Literal case.
                self.compile_alternation(&symbols, |c, sym| {
                    if sym.end == sym.start + 1 {
                        c.add_instruction(Instruction::Char(sym.start));
                    } else {
                        c.add_instruction(Instruction::Range {
                            start: sym.start,
                            end: sym.end,
                        });
                    }

                    Ok(())
                })?;
            }
            _ => {
                return Err(format_err!("Unsupported node type: {:?}", node));
            }
        }

        Ok(())

        /*
        /// Alternation. e.g. 'a|b|c|d'
        Alt(Vec<RegExpNodePtr>),
        /// Many adjacent nodes. e.g. 'abcd'
        Expr(Vec<RegExpNodePtr>),
        /// e.g. 'a*' or 'a?'
        Quantified(RegExpNodePtr, Quantifier),

        // We will most likely replace these with capture groups
        // Simplifying method:
        // For each operation, we will
        Class(Vec<Char>, bool),

        /// e.g. '(a)'
        Capture {
            inner: RegExpNodePtr,
            capturing: bool,
            name: String,
        },

        /// e.g. 'a'
        Literal(Char),

        Start,
        End,

        */
    }

    fn compile_alternation<T, F: Fn(&mut Compiler, &T) -> Result<()>>(
        &mut self,
        nodes: &[T],
        compile_node: F,
    ) -> Result<()> {
        if nodes.len() == 0 {
            return Ok(());
        }

        if nodes.len() == 1 {
            return compile_node(self, &nodes[0]);
        }

        let split_idx = self.program_counter();
        self.add_instruction(Instruction::Split(0, 0)); // Placeholder.

        let first_idx = self.program_counter();
        compile_node(self, &nodes[0])?;

        let jump_idx = self.program_counter();
        self.add_instruction(Instruction::Jump(0)); // Placeholder.

        let second_idx = self.program_counter();
        self.compile_alternation(&nodes[1..], compile_node)?;

        let end_idx = self.program_counter();

        self.output.program[split_idx as usize] = Instruction::Split(first_idx, second_idx);
        self.output.program[jump_idx as usize] = Instruction::Jump(end_idx);

        Ok(())
    }

    fn optimize(&mut self) {

        // Replace the instruction pattern:
        // 'Save(x) Char(y)' with 'Char(y) BeforeSave Lookahead(y) Save(x) Any'
        //
        // This works as long as there are no jumps to the 'Char(y)' and maybe
        // jumps to the 'Save(x)'.
        //
        // This optimization avoids copies of the string pointer buffer when we
        // see
        /*
        {
            for i in 0..(self.program.instructions.len() - 1) {
                let x = match self.program.instructions[i] {
                    Instruction::Save { index, .. } => index,
                    _ => continue
                };

                let y = match self.program.instructions[i + 1] {
                    Instruction::Char(y) => y,
                    _ => continue
                };

                let mut fail = false;
                for pc in self.program.iter_referenced_pcs() {
                    // Fail if there is an instruction that jumps to the 'Char(y)'
                    if *pc == (i + 1) as ProgramCounter {
                        fail = true;
                        break;
                    }
                }

                if fail {
                    continue;
                }

                self.program.instructions.insert(i, Instruction::Lookahead(y));
                self.program.instructions[i + 2] = Instruction::Any;

                for pc in self.program.iter_referenced_pcs() {
                    if *pc == i as ProgramCounter {
                        // Previous pointers to the Save(x) will now point to the Lookahead(y).
                        // (located at the same position as the old Save(x)).
                    } else if *pc > i as ProgramCounter {
                        // Push all PCs forward by one given that we just inserted a new op.
                        *pc += 1;
                    }
                }
            }
        }
        */

        // TODO: Consolidate consecutive lookaheads?
        // 'Lookahead(x) Lookahead(y)' will trivially terminate if x != y

        // TODO: Having a Special(StartOfString) instruction at the beginning
        // with no jumps to it can be removed (similarly at the end)

        // TODO: Jumps to a 'Match' can be replaced with a 'Match'
    }
}
