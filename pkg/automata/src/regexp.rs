use super::fsm::*;
use common::errors::*;
use parsing::*;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::ops::Bound;

/*
    PCRE Style RegExp Parser

    Grammar rules derived from: https://github.com/bkiers/pcre-parser/blob/master/src/main/antlr4/nl/bigo/pcreparser/PCRE.g4
*/

/*
    Annotate all state ids which correspond to a group
    - Once we hit an acceptor for one of these, then we can close the group

*/

/*
    Representing capture groups:
    - The only complication is alternation:
        - We must pretty much record the list of all states in each branch of an alternation
        - Then after minimization, compare all lists of states and figure out how to differentiate the cases
            -> Based on this we should be able to

    - Computing shortest string:
        -> Basically remove epsilons and compute graph shortest path
        -> If we can check for cycles then we tell whether or not a language is finite or infinite

    Optimization:
        - When observing a long sequence of chained characters, we can optimize the problem into:
            - https://en.wikipedia.org/wiki/Knuth%E2%80%93Morris%E2%80%93Pratt_algorithm
            - Basically the same idea as automata but in a rather condensed format

    More ideas on how to implement capture groups:
    - https://stackoverflow.com/questions/28941425/capture-groups-using-dfa-based-linear-time-regular-expressions-possible
*/

/*
    Character classes:
    -> [abcdef]
        -> For now, regular ones can be split
        -> We must make every single character class totally orthogonal
*/

/*
    We will ideally be able to reduce everything to

    Given a character class such as:
        '[aa]'
        -> Deduplication will be handled by the FSM
        -> But if we have '[ab\w]'
            -> Then we do need to perform splitting of to

*/

// TODO: What would '[\s-\w]' mean

// NOTE: This will likely end up being the token data for our state machine
// NOTE: We will also need to be able to represent inverses in the character
// sets Otherwise, we will need to make it a closed set ranges up to the maximum
// value
/*
    Given a sorted set of all possible alphabetic characters
    - We can choose exactly one in log(n) time where n is the size of the alphabet observed in the regex

*/

/*
    Given a list of all characters, we will in most cases perform two operations to look up a symbol id for a given symbol:
    ->

    -> General idea of subset decimation:
        -> Start out with the set of all symbols:
        ->

    -> Future optimizations:
        -> If a node has all or almost all possible symbols as edges out of it, it is likely gonna be more efficient to perform a method of negation comparison rather than inclusion
            -> Mainly to save on memory storing the transition table


    So we will have two internal types of regexps:
    -> One full ast tree

    -> then another which excludes custom character classes and non-well defined character classes
    -> We will Want to reduce all non-direct value sets to ranges or other types of closures

*/
/*
    Implementing ^ and $.

    - Without these, DFS should start and end with accepting
    - Every state goes back to itself on the 'start' symbol.

    - After one transition, we should know if it is possible to get from the current node to an acceptance state.
    - We should also know after one transition if everything will be an acceptance.
*/

#[derive(Debug)]
pub struct RegExp {
    alphabet: RegExpAlphabet,
    state_machine: RegExpStateMachine,
    metadata: RegExpMetadata,
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

type RegExpStateMachine = FiniteStateMachine<RegExpSymbol, RegExpEvent, HashSet<RegExpEvent>>;

#[derive(Default, Debug)]
struct RegExpMetadata {
    num_groups: usize,
    named_groups: HashMap<String, usize>,
}

pub struct RegExpMatch<'a, 'b> {
    /// The RegExp being used for matching.
    instance: &'a RegExp,

    /// The complete value that was initially given for matching.
    value: &'b [u8],

    /// Start index into 'value' of the current match.
    index: usize,

    remaining: &'b [u8],
    consumed_start: bool,
    consumed_end: bool,

    state: StateId,
    group_starts: Vec<Option<usize>>,
    group_values: Vec<Option<&'b [u8]>>,
}

impl<'a, 'b> RegExpMatch<'a, 'b> {
    /// Gets the complete string for the current match.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(
            self.value
                .split_at(self.index)
                .1
                .split_at(self.last_index() - self.index)
                .0,
        )
        .unwrap()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn last_index(&self) -> usize {
        self.value.len() - self.remaining.len()
    }

    pub fn groups(&self) -> impl Iterator<Item = Option<&str>> + std::fmt::Debug {
        self.group_values
            .iter()
            .map(|v| v.map(|v| std::str::from_utf8(v).unwrap()))
    }

    pub fn group(&self, index: usize) -> Option<&str> {
        self.group_values[index]
            .clone()
            .map(|v| std::str::from_utf8(v).unwrap())
    }

    pub fn named_group(&self, name: &str) -> Result<Option<&str>> {
        let group_id = *self
            .instance
            .metadata
            .named_groups
            .get(name)
            .ok_or(err_msg("No such group"))?;
        Ok(self.group_values[group_id]
            .clone()
            .map(|v| std::str::from_utf8(v).unwrap()))
    }

    pub fn next(mut self) -> Result<Option<Self>> {
        // TODO: Return an error if its possible to have infinite matches?

        // Reset state
        {
            self.group_starts.clear();
            self.group_starts
                .resize(self.instance.metadata.num_groups, None);
            self.group_values.clear();
            self.group_values
                .resize(self.instance.metadata.num_groups, None);

            // Point at starting state.
            self.restart()?;
        }

        if !self.consumed_start {
            self.consumed_start = true;
            self.consume_symbol(RegExpSymbol::start_of_string())?;
        }

        loop {
            if self.instance.state_machine.is_accepting_state(self.state) {
                return Ok(Some(self));
            }

            let sym = match self.remaining.next() {
                Some(c) => self.instance.alphabet.get(c as char),
                None => {
                    if self.consumed_end {
                        return Ok(None);
                    }

                    self.consumed_end = true;
                    RegExpSymbol::end_of_string()
                }
            };

            self.consume_symbol(sym)?;
        }
    }

    fn restart(&mut self) -> Result<()> {
        self.update_state(
            *self
                .instance
                .state_machine
                .starts()
                .next()
                .ok_or(err_msg("No starting state"))?,
            &HashSet::new(),
        )
    }

    fn consume_symbol(&mut self, sym: RegExpSymbol) -> Result<()> {
        let (next_state, events) = self
            .instance
            .state_machine
            .lookup_transducer(self.state, &sym)
            .next()
            // NOTE: This should only fail if the DFA was constructed poorly
            .ok_or(err_msg("No transitions for symbol"))?;

        self.update_state(*next_state, events)?;

        Ok(())
    }

    fn update_state(&mut self, state: StateId, events: &HashSet<RegExpEvent>) -> Result<()> {
        self.state = state;

        let cur_idx = self.last_index();

        for event in self.instance.state_machine.tags(state) {
            match event {
                RegExpEvent::StartMatch => {
                    self.index = cur_idx;
                }
                RegExpEvent::StartGroup(group_id) => {
                    self.group_starts[*group_id] = Some(cur_idx);
                    // group_allowlist.insert(*group_id);
                }
                _ => {}
            }
        }

        // Every alternation needs a group in each edge .
        // An alternation may have internal group ids [g_0, g_1, g_2]
        // - Find the first active one and all others must be dead

        // TODO: Use a bit set (maybe a BitVector)
        // let mut group_allowlist = HashSet::new();
        for event in events {
            match event {
                RegExpEvent::EndGroup(group_id) => {
                    // TODO: We probably don't want to use 'take' in order to support the '(a+)'
                    // pattern.
                    let start_idx = match self.group_starts[*group_id] {
                        Some(v) => v,
                        None => continue,
                    };
                    let end_idx = cur_idx;
                    let segment = self
                        .value
                        .split_at(start_idx)
                        .1
                        .split_at(end_idx - start_idx)
                        .0;
                    self.group_values[*group_id] = Some(segment);
                    // group_allowlist.insert(*group_id);
                }
                _ => {}
            }
        }

        // TODO: Improve the performance of this procedure. There will likely always
        // only be a few groups that are actually set at any
        // TODO: This is wrong because we may see groups later on
        // for group_id in 0..self.group_starts.len() {
        //     if !group_allowlist.contains(&group_id) {
        //         self.group_starts[group_id] = None;
        //     }
        // }

        Ok(())
    }
}

impl<'a, 'b> std::fmt::Debug for RegExpMatch<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegExpMatch")
            .field("index", &self.index())
            .field("match", &self.as_str())
            .field("groups", &self.groups())
            .finish()
    }
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

#[derive(Clone, PartialEq, Hash, Eq, Debug)]
enum RegExpEvent {
    StartMatch,
    StartGroup(usize),
    EndGroup(usize),
    // NaN, // InGroup(usize),
}

// TODO: Unused right now
enum GroupType {
    /// Captured and just output based on its index
    Regular,

    /// Captured and output indexed by a name
    Named(String),

    /// Implying that the capture group is not maintained in the output
    Ignore,
}

/*
    For
*/

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
enum Char {
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
    fn raw_symbols(&self) -> Vec<RegExpSymbol> {
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

fn invert_symbols(syms: Vec<RegExpSymbol>) -> Vec<RegExpSymbol> {
    let mut out = vec![];
    for item in syms {
        if item.start > 0 {
            out.push(RegExpSymbol {
                start: 0,
                end: item.start,
            });
        }
        if item.end < (std::char::MAX as u32) {
            out.push(RegExpSymbol {
                start: item.end,
                end: std::char::MAX as u32,
            })
        }
    }

    out
}

/// Symbols (character point values) used to represent the start and end of
/// string/line nodes in the FSM.
///
/// NOTE: These are chosen to be outside of the UTF-8 range.
/// NOTE: The largest utf-8 character is 0x10FFFF
/// TODO: Verify that we never get inputs out of that range.
const START_SYMBOL: u32 = (std::char::MAX as u32) + 1;
// TODO: Have an assertion that this is < std::u32::max (as we need to add one
// to this to get an inclusive range.)
const END_SYMBOL: u32 = (std::char::MAX as u32) + 2;

/// Internal representation of a set of values associated with an edge between
/// nodes in the regular expression's state machine.
///
/// All symbols used in the internal state machine will be non-overlapping.
#[derive(PartialEq, PartialOrd, Clone, Hash, Eq, Ord)]
struct RegExpSymbol {
    start: u32,
    end: u32,
}

impl RegExpSymbol {
    fn single(c: char) -> Self {
        Self::inclusive_range(c, c)
    }
    fn inclusive_range(s: char, e: char) -> Self {
        Self {
            start: (s as u32),
            end: (e as u32) + 1,
        }
    }
    fn start_of_string() -> Self {
        Self {
            start: START_SYMBOL,
            end: START_SYMBOL + 1,
        }
    }
    fn end_of_string() -> Self {
        Self {
            start: END_SYMBOL,
            end: END_SYMBOL + 1,
        }
    }

    fn debug_offset(v: u32) -> String {
        if v == 0 {
            "0".into()
        } else if v == START_SYMBOL {
            "^".into()
        } else if v == END_SYMBOL {
            "$".into()
        } else if v > END_SYMBOL {
            "inf".into()
        } else if v == std::char::MAX as u32 {
            "CMAX".into()
        } else {
            format!("\"{}\"", char::try_from(v).unwrap())
        }
    }
}

impl std::fmt::Debug for RegExpSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "[{}, {})",
            Self::debug_offset(self.start),
            Self::debug_offset(self.end)
        ))
    }
}

/// e.g. The regular expression '(a|[a-z])' will have an alphabet of the
/// following value ranges:
/// [0, 'a'), ['a', 'b'), ['b', 'z'+1), ['z+1', MAX)
/// ^ This will only exist in the intermediate automata format. once a final
/// DFA has been generated, we can re-merge the symbols per source node into
/// the most optimal matching format.
#[derive(Debug)]
struct RegExpAlphabet {
    offsets: std::collections::BTreeSet<u32>,
}

impl RegExpAlphabet {
    fn new() -> Self {
        let mut offsets = std::collections::BTreeSet::new();
        offsets.insert(0);
        offsets.insert(std::char::MAX as u32);
        Self { offsets }
    }

    fn insert(&mut self, sym: RegExpSymbol) {
        self.offsets.insert(sym.start);
        self.offsets.insert(sym.end);
    }

    fn all_symbols(&self) -> Vec<RegExpSymbol> {
        let mut out = vec![];
        let mut iter = self.offsets.iter();
        let mut i = *iter.next().unwrap();
        for j in iter {
            out.push(RegExpSymbol { start: i, end: *j });
            i = *j;
        }

        out
    }

    fn get(&self, c: char) -> RegExpSymbol {
        let v = c as u32;

        // Closest offset <= v
        let i = *self
            .offsets
            .range((Bound::Unbounded, Bound::Included(v)))
            .rev()
            .next()
            .unwrap();
        // Closest offset > v
        let j = *self
            .offsets
            .range((Bound::Excluded(v), Bound::Unbounded))
            .next()
            .unwrap();

        assert!(i <= v && j > v, "{} <= {} < {}", i, v, j);

        RegExpSymbol { start: i, end: j }
    }

    fn decimate(&self, sym: RegExpSymbol) -> Vec<RegExpSymbol> {
        let mut range = self
            .offsets
            .range((Bound::Included(sym.start), Bound::Included(sym.end)));

        let mut out = vec![];
        let mut i = range.next().unwrap();
        assert_eq!(*i, sym.start);
        for j in range {
            out.push(RegExpSymbol { start: *i, end: *j });
            i = j;
        }
        assert_eq!(*i, sym.end);

        out
    }

    // NOTE: The output of this may have overlapping symbols.
    fn decimate_many(&self, syms: Vec<RegExpSymbol>) -> Vec<RegExpSymbol> {
        let mut out = vec![];
        for sym in syms {
            out.extend_from_slice(&self.decimate(sym));
        }
        out
    }
}

type RegExpNodePtr = Box<RegExpNode>;

#[derive(Debug)]
enum RegExpNode {
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
}

impl RegExpNode {
    pub fn parse(s: &str) -> Result<RegExpNodePtr> {
        let (res, _) = complete(alternation)(s)?;
        Ok(res)
        //		match parse_regexp(s) {
        //			Ok((rest, r)) => {
        //				if rest.len() == 0 {
        //					Ok(r)
        //				}
        //				else {
        //					Err(s.len() - rest.len())
        //				}
        //			},
        //			Err(_) => Err(0)
        //		}
    }

    // TODO: We will want to optimize this to have type &CharSet or something or
    // some other pointer so that it can optimize out the Option<S> inside of the
    // FSM code
    // pub fn to_automata(&self) -> RegExpStateMachine {
    //     let mut alpha = RegExpAlphabet::new();
    //     self.fill_alphabet(&mut alpha);
    //     self.to_automata_inner(&alpha)
    // }

    fn to_automata_inner(
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
            Self::Class(chars, inverted) => {
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

    fn zero_transitions(tag: RegExpEvent) -> RegExpStateMachine {
        let mut a = RegExpStateMachine::new();
        let state = a.add_state();
        a.add_tag(state, tag);
        a.mark_start(state);
        a.mark_accept(state);

        a
    }

    fn one_transition(sym: RegExpSymbol) -> RegExpStateMachine {
        let mut a = RegExpStateMachine::new();
        let start = a.add_state();
        a.mark_start(start);
        let end = a.add_state();
        a.mark_accept(end);
        a.add_transition(start, sym, end);
        a
    }

    fn fill_alphabet(&self, alpha: &mut RegExpAlphabet) {
        match self {
            Self::Alt(list) => {
                for r in list.iter() {
                    r.fill_alphabet(alpha);
                }
            }
            Self::Capture { inner, .. } => inner.fill_alphabet(alpha),
            Self::Class(items, _invert) => {
                for item in items {
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

/*
    General grammar is:
        Regexp -> Alternation
        Alternation -> Expr | ( Expr '|' Alternation )
        Expr -> Element Expr | <empty>
        Element -> '^' | '$' | Quantified
        Quantified -> (Group | CharacterClass | EscapedLiteral | Literal) Repetitions
        Repetitions -> '*' | '?' | '+' | <empty>

        Group -> '(' Regexp ')'

*/

parser!(alternation<&str, RegExpNodePtr> => {
    map(delimited(expr, tag("|")), |alts| Box::new(RegExpNode::Alt(alts)))
});

parser!(expr<&str, RegExpNodePtr> => {
    map(many(element), |els| Box::new(RegExpNode::Expr(els)))
});

parser!(element<&str, RegExpNodePtr> => alt!(
    map(tag("^"), |_| Box::new(RegExpNode::Start)),
    map(tag("$"), |_| Box::new(RegExpNode::End)),
    quantified
));

// Quantified -> Atom Quantifier | Atom
parser!(quantified<&str, RegExpNodePtr> => {
    seq!(c => {
        let a = c.next(atom)?;
        if let Some(q) = c.next(opt(Quantifier::parse))? {
            return Ok(Box::new(RegExpNode::Quantified(a, q)));
        }

        Ok(a)
    })
});

#[derive(Debug)]
enum Quantifier {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    // TODO: To keep the memory usage down, these should limit the max count to say '256'
    ExactlyN(usize),
    NOrMore(usize),
    Between(usize, usize),
}

impl Quantifier {
    parser!(parse<&str, Self> => alt!(
        map(tag("?"), |_| Quantifier::ZeroOrOne),
        map(tag("*"), |_| Quantifier::ZeroOrMore),
        map(tag("+"), |_| Quantifier::OneOrMore),
        seq!(c => {
            c.next(tag("{"))?;
            let lower_num = c.next(number)?;

            let upper_num: Option<Option<usize>> = c.next(opt(seq!(c => {
                c.next(tag(","))?;
                c.next(opt(number))
            })))?;

            c.next(tag("}"))?;

            Ok(match upper_num {
                Some(Some(upper_num)) => {
                    if lower_num > upper_num {
                        return Err(err_msg("Invalid quantifier lower > higher"));
                    }

                    Quantifier::Between(lower_num, upper_num)
                },
                Some(None) => Quantifier::NOrMore(lower_num),
                None => Quantifier::ExactlyN(lower_num)
            })
        })
    ));
}

parser!(atom<&str, RegExpNodePtr> => alt!(
    map(shared_atom, |c| Box::new(RegExpNode::Literal(c))),
    literal,
    character_class,
    capture
));

// TODO: Strategy for implementing character classes
// If there are other overlapping symbols,

// TODO: In PCRE, '[]]' would parse as a character class matching the character
// ']' but for simplity we will require that that ']' be escaped in a character
// class
parser!(character_class<&str, RegExpNodePtr> => seq!(c => {
    c.next(tag("["))?;
    let invert = c.next(opt(tag("^")))?;
    let inner = c.next(many(character_class_atom))?; // NOTE: We allow this to be empty.
    c.next(tag("]"))?;

    return Ok(Box::new(RegExpNode::Class(inner, invert.is_some())));
}));

parser!(capture<&str, RegExpNodePtr> => seq!(c => {
    c.next(tag("("))?;
    let (capturing, name) = c.next(opt(capture_flags))?.unwrap_or((true, String::new()));
    let inner = c.next(alternation)?;
    c.next(tag(")"))?;

    Ok(Box::new(RegExpNode::Capture { inner, capturing, name } ))
}));

parser!(capture_flags<&str, (bool, String)> => seq!(c => {
    c.next(tag("?"))?;

    c.next(alt!(
        map(tag(":"), |_| (false, String::new())),
        seq!(c => {
            c.next(tag("<"))?;
            let name: &str = c.next(take_while1(|c: char| c != '>'))?;
            c.next(tag(">"))?;
            Ok((true, name.to_owned()))
        })
    ))
}));

parser!(character_class_atom<&str, Char> => alt!(
    seq!(c => {
        let start = c.next(character_class_literal)?;
        c.next(tag("-"))?;
        let end = c.next(character_class_literal)?;

        // TODO: Return this as an error, but don't allow trying to parse
        // other alt! cases.
        assert!(end >= start);

        // In this case,
        Ok(Char::Range(start, end))
    }),

    shared_atom,
    map(character_class_literal, |c| Char::Value(c))
));

// TODO: Ensure that we support \t, \f \a, etc. See:
// https://github.com/google/re2/wiki/Syntax (search 'Escape Sequences')

// TODO: It seems like it could be better to combine this with the
// shared_literal class
parser!(shared_atom<&str, Char> => alt!(
    map(tag("\\w"), |_| Char::Word),
    map(tag("\\d"), |_| Char::Digit),
    map(tag("\\s"), |_| Char::Whitespace),
    map(tag("\\W"), |_| Char::NotWord),
    map(tag("\\D"), |_| Char::NotDigit),
    map(tag("\\S"), |_| Char::NotWhiteSpace)
));

// A single plain character that must be exactly matched.
// This rule does not apply to anything inside a character class.
// e.g. the regexp 'ab' contains 2 literals.
parser!(literal<&str, RegExpNodePtr> => {
    map(alt!(shared_literal,
             map(tag("]"), |_| ']')),
        |c| Box::new(RegExpNode::Literal(Char::Value(c))))
});

// TODO: Check this
parser!(character_class_literal<&str, char> => {
    //shared_literal |
    map(not_one_of("]"), |c| c as char)
});

// Single characters which need to be matched exactly
// (excluding symbols which may have a different meaning depending on context)
parser!(shared_literal<&str, char> => alt!(
    map(not_one_of("[]\\^$.|?*+()"), |v| v as char),
    quoted
));

// TODO: Verify that '01' is a valid number
parser!(number<&str, usize> => and_then(
    take_while1(|c: char| c.is_digit(10)),
    // NOTE: We don't unwrap as it could be out of range.
    |s: &str| { let n = s.parse::<usize>()?; Ok(n) }
));

// Matches '\' followed by the character being escaped.
parser!(quoted<&str, char> => {
    seq!(c => {
        c.next(tag("\\"))?;
        let v = c.next::<&str, _>(take_exact(1))?.chars().next().unwrap() as char;
        if v.is_alphanumeric() {
            return Err(err_msg("Expected non alphanumeric character"));
        }

        Ok(v)
    })
});

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
