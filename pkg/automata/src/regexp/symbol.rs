use std::convert::TryFrom;

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
pub struct RegExpSymbol {
    pub start: u32,
    pub end: u32,
}

impl RegExpSymbol {
    pub fn single(c: char) -> Self {
        Self::inclusive_range(c, c)
    }
    pub fn inclusive_range(s: char, e: char) -> Self {
        Self {
            start: (s as u32),
            end: (e as u32) + 1,
        }
    }
    pub fn start_of_string() -> Self {
        Self {
            start: START_SYMBOL,
            end: START_SYMBOL + 1,
        }
    }
    pub fn end_of_string() -> Self {
        Self {
            start: END_SYMBOL,
            end: END_SYMBOL + 1,
        }
    }

    // TODO: Make private eventually
    pub(crate) fn debug_offset(v: u32) -> String {
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

pub fn invert_symbols(syms: Vec<RegExpSymbol>) -> Vec<RegExpSymbol> {
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