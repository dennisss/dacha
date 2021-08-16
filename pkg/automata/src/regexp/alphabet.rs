use std::ops::Bound;

use crate::regexp::symbol::*;

/// e.g. The regular expression '(a|[a-z])' will have an alphabet of the
/// following value ranges:
/// [0, 'a'), ['a', 'b'), ['b', 'z'+1), ['z+1', MAX)
/// ^ This will only exist in the intermediate automata format. once a final
/// DFA has been generated, we can re-merge the symbols per source node into
/// the most optimal matching format.
#[derive(Debug)]
pub struct RegExpAlphabet {
    offsets: std::collections::BTreeSet<u32>,
}

impl RegExpAlphabet {
    pub fn new() -> Self {
        let mut offsets = std::collections::BTreeSet::new();
        offsets.insert(0);
        offsets.insert(std::char::MAX as u32);
        Self { offsets }
    }

    pub fn insert(&mut self, sym: RegExpSymbol) {
        self.offsets.insert(sym.start);
        self.offsets.insert(sym.end);
    }

    pub fn all_symbols(&self) -> Vec<RegExpSymbol> {
        let mut out = vec![];
        let mut iter = self.offsets.iter();
        let mut i = *iter.next().unwrap();
        for j in iter {
            out.push(RegExpSymbol { start: i, end: *j });
            i = *j;
        }

        out
    }

    pub fn get(&self, c: char) -> RegExpSymbol {
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
    pub fn decimate_many(&self, syms: Vec<RegExpSymbol>) -> Vec<RegExpSymbol> {
        let mut out = vec![];
        for sym in syms {
            out.extend_from_slice(&self.decimate(sym));
        }
        out
    }
}
