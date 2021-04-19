use std::collections::HashMap;

use common::errors::*;

use crate::regexp::alphabet::*;
use crate::fsm::*;
use crate::regexp::symbol::*;
use crate::regexp::state_machine::*;
use crate::regexp::node::*;
use crate::regexp::r#match::*;


#[derive(Debug)]
pub struct RegExp {
    // NOTE: Currently most of this state is shared with the Match.
    pub(crate) alphabet: RegExpAlphabet,
    pub(crate) state_machine: RegExpStateMachine,
    pub(crate) metadata: RegExpMetadata,
}

impl RegExp {
    pub fn new(expr: &str) -> Result<Self> {
        let tree = RegExpNode::parse(expr)?;

        let mut alpha = RegExpAlphabet::new();
        tree.fill_alphabet(&mut alpha);

        // NOTE: These must always be in the alphabet. Otherwise the FSM will fail if it
        // sees unknown symbols.
        alpha.insert(RegExpSymbol::start_of_string());
        alpha.insert(RegExpSymbol::end_of_string());

        // Any number of wildcard symbols are accepted at the beginning of the string.
        // (this implements the case of having no '^' in the regexp).
        let mut state_machine = Self::wildcard_machine(&alpha);

        state_machine.then(RegExpNode::zero_transitions(RegExpEvent::StartMatch));

        let mut metadata = RegExpMetadata::default();
        state_machine.then(tree.to_automata_inner(&alpha, &mut metadata));

        // Any number of wildcard symbols are accepted at the end of the string. (this
        // implements the case of having no '$' in the regexp)
        state_machine.then(Self::wildcard_machine(&alpha));

        // TODO: Merge adjacent symbols (but this will require us to use another
        // approach for submatches as we will lose edge transducer behavior).
        state_machine = state_machine.compute_dfa(); // .minimal();

        Ok(Self {
            alphabet: alpha,
            state_machine,
            metadata,
        })
    }

    /// Creates a state machine that implements the '.*' expression.
    fn wildcard_machine(alpha: &RegExpAlphabet) -> RegExpStateMachine {
        let mut state_machine = RegExpStateMachine::new();
        let first_state = state_machine.add_state();
        state_machine.mark_accept(first_state);
        state_machine.mark_start(first_state);

        for sym in alpha.all_symbols() {
            state_machine.add_transition(first_state, sym, first_state);
        }

        state_machine
    }

    // TODO: Ensure that matching is greedy, and always continues matching until
    // the current match is exhausted.
    pub fn test(&self, value: &str) -> bool {
        let iter = RegExpSymbolIter {
            char_iter: value.chars(),
            alphabet: &self.alphabet,
            start_emitted: false,
            end_emitted: false,
        };
        self.state_machine.accepts(iter)
    }

    pub fn exec<'a, 'b, T: AsRef<[u8]> + ?Sized>(&'a self, value: &'b T) -> Result<Option<RegExpMatch<'a, 'b>>> {
        let state = RegExpMatch {
            instance: self,
            value: value.as_ref(),
            index: 0,
            remaining: value.as_ref(),
            consumed_start: false,
            consumed_end: false,
            state: 0, // Will be initialized in the next statement.
            group_starts: vec![],
            group_values: vec![],
        };

        state.next()
    }
}

#[derive(Default, Debug)]
pub(crate) struct RegExpMetadata {
    pub num_groups: usize,
    pub named_groups: HashMap<String, usize>,
}


struct RegExpSymbolIter<'a> {
    char_iter: std::str::Chars<'a>,
    alphabet: &'a RegExpAlphabet,
    start_emitted: bool,
    end_emitted: bool,
}

impl<'a> std::iter::Iterator for RegExpSymbolIter<'a> {
    type Item = RegExpSymbol;
    fn next(&mut self) -> Option<Self::Item> {
        if !self.start_emitted {
            self.start_emitted = true;
            return Some(RegExpSymbol::start_of_string());
        }

        match self.char_iter.next().map(|c| self.alphabet.get(c)) {
            Some(v) => Some(v),
            None => {
                if !self.end_emitted {
                    self.end_emitted = true;
                    Some(RegExpSymbol::end_of_string())
                } else {
                    None
                }
            }
        }
    }
}










#[cfg(test)]
mod tests {

    use super::*;

    // #[test]
    // fn parsing_inline() {
    //     let r = parse_regexp("123");

    //     println!("{:?}", r);
    // }

    #[test]
    fn regexp_test() {
        // These three are basically a test case for equivalent languages that can
        // produce different automata

        let a = RegExp::new("a(X|Y)c").unwrap();
        // println!("A: {:?}", a);

        let b = RegExp::new("(aXc)|(aYc)").unwrap();
        // println!("B: {:?}", b);

        let c = RegExp::new("a(Xc|Yc)").unwrap();
        // println!("C: {:?}", c);
        assert!(c.test("aXc"));
        assert!(c.test("aYc"));
        assert!(!c.test("a"));
        assert!(!c.test("c"));
        assert!(!c.test("Y"));
        assert!(!c.test("Yc"));
        assert!(!c.test(""));

        let d = RegExp::new("a").unwrap();
        // println!("{:?}", d);

        assert!(d.test("a"));
        assert!(!d.test("b"));

        // NOTE: This has infinite matches and matches everything
        let e = RegExp::new("[a-z0-9]*").unwrap();
        // println!("{:?}", e);
        assert!(e.test("a9034343"));
        assert!(e.test(""));

        let j = RegExp::new("[a-b]").unwrap();
        println!("{:?}", j);
        assert!(j.test("a"));
        assert!(j.test("b"));
        assert!(!j.test("c"));
        assert!(!j.test("d"));

        assert!(j.test("zzzzzzzaxxxxx"));

        let k = RegExp::new("^a$").unwrap();
        assert!(k.test("a"));
        assert!(!k.test("za"));

        let l = RegExp::new("a").unwrap();
        assert!(l.test("a"));
        assert!(l.test("za"));

        let match1 = a.exec("aXc").unwrap();
        println!("{:?}", match1);

        let match2 = b.exec("aYc blah blah blah aXc hello").unwrap().unwrap();
        assert_eq!(match2.as_str(), "aYc");
        assert_eq!(match2.index(), 0);
        assert_eq!(match2.groups().collect::<Vec<_>>(), &[None, Some("aYc")]);
        println!("{:?}", match2);

        let match21 = match2.next().unwrap().unwrap();
        assert_eq!(match21.as_str(), "aXc");
        assert_eq!(match21.index(), 19);
        assert_eq!(match21.groups().collect::<Vec<_>>(), &[Some("aXc"), None]);
        println!("{:?}", match21);

        assert!(match21.next().unwrap().is_none());
    }

    #[test]
    fn regexp_group_test() {
        let r = RegExp::new("^(?:(a)|(b))").unwrap();
        let m = r.exec("a hello").unwrap().unwrap();
        println!("{:?}", m);
        // TODO: Check that there are only 2 groups
        assert!(m.group(0).is_some());
        assert!(!m.group(1).is_some());
    }

    #[test]
    fn regexp_group2_test() {
        return;

        let r = RegExp::new("((a)(b))|((a)(c))").unwrap();
        let m = r.exec("ac").unwrap().unwrap();
        println!("{:#?}", r);
        println!("{:?}", m);
        assert!(false);
    }

    // TODO: It should be noted that we don't support empty capture groups!
    // (e.g. 'a()b')
}