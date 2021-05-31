use std::collections::HashSet;

use common::errors::*;

use crate::fsm::FiniteStateMachine;
use crate::regexp::symbol::*;
use crate::regexp::alphabet::*;
use crate::regexp::state_machine::*;
use crate::regexp::instance::RegExpMetadata;  // TODO: Refactor out this edge.

pub type RegExpNodePtr = Box<RegExpNode>;


// TODO: Whenever we see an alternation of many single character nodes
// we can probably compile it down to a simple class of values.

/*
Other safe simplifications:
- Collapse a Capture group that is non-capturing.
    - Either change into an Expr or merge into an upper Expr
- Collapse single item Alt
- Remove zero item Alt

*/


/// A node in the tree 
#[derive(Debug)]
pub enum RegExpNode {
    /// Alternation. e.g. 'a|b|c|d'
    Alt(Vec<RegExpNodePtr>),
    
    /// Many adjacent nodes. e.g. 'abcd'
    Expr(Vec<RegExpNodePtr>),

    /// e.g. 'a*' or 'a?'
    Quantified(RegExpNodePtr, Quantifier),

    // We will most likely replace these with capture groups
    // Simplifying method:
    // For each operation, we will
    Class { chars: Vec<Char>, inverted: bool },

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
}

impl RegExpNode {
    pub fn parse(s: &str) -> Result<RegExpNodePtr> {
        crate::regexp::syntax::parse_root_expression(s)
    }

    // TODO: We will want to optimize this to have type &CharSet or something or
    // some other pointer so that it can optimize out the Option<S> inside of the
    // FSM code
    // pub fn to_automata(&self) -> RegExpStateMachine {
    //     let mut alpha = RegExpAlphabet::new();
    //     self.fill_alphabet(&mut alpha);
    //     self.to_automata_inner(&alpha)
    // }

    pub(crate) fn to_automata_inner(
        &self,
        alpha: &RegExpAlphabet,
        metadata: &mut RegExpMetadata,
    ) -> RegExpStateMachine {
        match self {
            Self::Alt(list) => {
                let mut a = RegExpStateMachine::new();
                for r in list.iter() {
                    a.join(r.to_automata_inner(alpha, metadata));
                }

                a
            }
            Self::Expr(list) => {
                let mut a = RegExpStateMachine::zero();
                for r in list.iter() {
                    a.then(r.to_automata_inner(alpha, metadata));
                }

                a
            }
            Self::Quantified(r, q) => {
                let mut a = r.to_automata_inner(alpha, metadata);

                match q {
                    Quantifier::ZeroOrOne => {
                        a.join(FiniteStateMachine::zero());
                    }
                    Quantifier::ZeroOrMore => {
                        a.then_loop();
                        a.join(FiniteStateMachine::zero());
                    }
                    Quantifier::OneOrMore => {
                        a.then_loop();
                    }
                    Quantifier::ExactlyN(n) => {
                        if *n == 0 {
                            return FiniteStateMachine::zero();
                        }

                        let base = a.clone();
                        for i in 0..(n - 1) {
                            a.then(base.clone());
                        }
                    }
                    Quantifier::Between(lower, upper) => {
                        panic!("Not supported");
                    }
                    Quantifier::NOrMore(n) => {
                        // Same as ZeroOrOne
                        if *n == 0 {
                            a.join(FiniteStateMachine::zero());
                            return a;
                        }

                        let base = a.clone();
                        for i in 0..(n - 1) {
                            a.then(base.clone());
                        }

                        let mut or_more = base.clone();
                        or_more.then_loop();
                        or_more.join(FiniteStateMachine::zero());
                        a.then(or_more);
                    }
                }

                a
            }

            Self::Capture {
                inner,
                capturing,
                name,
            } => {
                if !capturing {
                    inner.to_automata_inner(alpha, metadata)
                } else {
                    let group_id: usize = metadata.num_groups;
                    metadata.num_groups += 1;

                    if !name.is_empty() {
                        metadata.named_groups.insert(name.clone(), group_id);
                    }

                    let mut state_machine =
                        Self::zero_transitions(RegExpEvent::StartGroup(group_id));

                    let inner_machine = inner.to_automata_inner(alpha, metadata);

                    state_machine.then(inner_machine);

                    state_machine.then({
                        let mut m = RegExpStateMachine::new();
                        let start = m.add_state();
                        let end = m.add_state();
                        m.mark_start(start);
                        m.mark_accept(end);

                        let mut events = HashSet::new();
                        events.insert(RegExpEvent::EndGroup(group_id));

                        m.add_epsilon_transducer(start, end, events);
                        m
                    });

                    // state_machine.then(Self::zero_transitions(RegExpEvent::EndGroup(group_id)));

                    state_machine
                }
            }
            // TODO: After an automata has been built, we need to go back and
            // split all automata (or merge any indivual characters into a
            // single character class if they all point to the same place)
            Self::Class { chars, inverted } => {
                let mut syms = vec![];
                for c in chars {
                    syms.extend_from_slice(&alpha.decimate_many(c.raw_symbols()));
                }
                if *inverted {
                    syms = invert_symbols(syms);
                }

                syms = alpha.decimate_many(syms);

                // TODO: Same as RegExprNode::Literal case below
                let mut a = RegExpStateMachine::new();
                let start = a.add_state();
                a.mark_start(start);
                let end = a.add_state();
                a.mark_accept(end);
                for sym in syms {
                    a.add_transition(start, sym, end);
                }
                a
            }
            Self::Literal(c) => {
                let syms = alpha.decimate_many(c.raw_symbols());

                let mut a = RegExpStateMachine::new();
                let start = a.add_state();
                a.mark_start(start);
                let end = a.add_state();
                a.mark_accept(end);
                for sym in syms {
                    a.add_transition(start, sym, end);
                }
                a
            }
            // TODO: Use transducer for these too.
            Self::Start => Self::one_transition(RegExpSymbol::start_of_string()),
            Self::End => Self::one_transition(RegExpSymbol::end_of_string()),
        }
    }

    pub fn zero_transitions(tag: RegExpEvent) -> RegExpStateMachine {
        let mut a = RegExpStateMachine::new();
        let state = a.add_state();
        a.add_tag(state, tag);
        a.mark_start(state);
        a.mark_accept(state);

        a
    }

    pub fn one_transition(sym: RegExpSymbol) -> RegExpStateMachine {
        let mut a = RegExpStateMachine::new();
        let start = a.add_state();
        a.mark_start(start);
        let end = a.add_state();
        a.mark_accept(end);
        a.add_transition(start, sym, end);
        a
    }

    pub fn fill_alphabet(&self, alpha: &mut RegExpAlphabet) {
        match self {
            Self::Alt(list) => {
                for r in list.iter() {
                    r.fill_alphabet(alpha);
                }
            }
            Self::Capture { inner, .. } => inner.fill_alphabet(alpha),
            Self::Class { chars, .. } => {
                for item in chars {
                    item.fill_alphabet(alpha);
                }
            }
            Self::Literal(c) => {
                c.fill_alphabet(alpha);
            }
            Self::Quantified(e, _) => e.fill_alphabet(alpha),
            Self::Expr(list) => {
                for item in list {
                    item.fill_alphabet(alpha);
                }
            }
            Self::Start | Self::End => {}
        }
    }
}

// TODO: Unused right now
pub enum GroupType {
    /// Captured and just output based on its index
    Regular,

    /// Captured and output indexed by a name
    Named(String),

    /// Implying that the capture group is not maintained in the output
    Ignore,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum Char {
    /// A single character that must exactly match the next input.
    Value(char),
    /// A character range expression like '[a-z]'.
    /// NOTE: This will never occur in a RegExprNode::Literal
    /// NOTE: We don't allow '[.-\d]'
    Range(char, char),
    Wildcard, // '.'
    Word,
    Digit,
    Whitespace, // '\w' '\d' '\s'
    NotWord,
    NotDigit,
    NotWhiteSpace, // '\W' '\D' '\S'
}

impl Char {
    // NOTE: These symbols may contain a lot of overlap.
    pub fn raw_symbols(&self) -> Vec<RegExpSymbol> {
        let mut out = vec![];

        match self {
            Char::Value(c) => {
                out.push(RegExpSymbol::single(*c));
            }
            Char::Range(s, e) => {
                out.push(RegExpSymbol::inclusive_range(*s, *e));
            }
            // [0-9]
            Char::Digit | Char::NotDigit => {
                out.push(RegExpSymbol::inclusive_range('0', '9'));
            }
            // [0-9A-Za-z_]
            Char::Word | Char::NotWord => {
                out.push(RegExpSymbol::inclusive_range('0', '9'));
                out.push(RegExpSymbol::inclusive_range('A', 'Z'));
                out.push(RegExpSymbol::inclusive_range('a', 'z'));
                out.push(RegExpSymbol::single('_'));
            }
            // [\t\n\f\r ]
            Char::Whitespace | Char::NotWhiteSpace => {
                out.push(RegExpSymbol::single('\t'));
                out.push(RegExpSymbol::single('\n'));
                out.push(RegExpSymbol::single('\x0C'));
                out.push(RegExpSymbol::single('\r'));
                out.push(RegExpSymbol::single(' '));
            }
            Char::Wildcard => {
                out.push(RegExpSymbol::inclusive_range(0 as char, std::char::MAX));
            }
        }

        let invert = match self {
            Char::NotWhiteSpace | Char::NotDigit | Char::NotWord => true,
            _ => false,
        };

        if invert {
            out = invert_symbols(out);
        }

        out
    }

    fn fill_alphabet(&self, alpha: &mut RegExpAlphabet) {
        for sym in self.raw_symbols() {
            alpha.insert(sym);
        }
    }
}

#[derive(Debug)]
pub enum Quantifier {
    /// e.g. 'A?'
    ZeroOrOne,

    /// e.g. 'A*'
    ZeroOrMore,

    /// e.g. 'A+'
    OneOrMore,

    // TODO: To keep the memory usage down, these should limit the max count to say '256'
    ExactlyN(usize),
    
    NOrMore(usize),
    
    Between(usize, usize),
}
