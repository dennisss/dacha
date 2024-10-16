use crate::regexp::symbol::RegExpSymbol;

#[derive(Default)]
pub struct RegExpSymbolSetBuilder {
    syms: Vec<RegExpSymbol>,
}

impl RegExpSymbolSetBuilder {
    pub fn add(&mut self, symbol: RegExpSymbol) {
        self.syms.push(symbol);
    }

    pub fn extend(&mut self, symbols: &[RegExpSymbol]) {
        self.syms.extend_from_slice(symbols);
    }

    pub fn build(mut self) -> RegExpSymbolSet {
        self.syms.sort();

        // Merging overlapping / duplicate symbols.
        let mut final_symbols = vec![];
        for symbol in self.syms.iter().cloned() {
            let last_symbol = match final_symbols.last_mut() {
                Some(v) => v,
                None => {
                    final_symbols.push(symbol);
                    continue;
                }
            };

            if symbol.start <= last_symbol.end {
                last_symbol.end = core::cmp::max(last_symbol.end, symbol.end);
                continue;
            }

            final_symbols.push(symbol);
        }

        RegExpSymbolSet {
            syms: final_symbols,
        }
    }
}

/// Set of symbols. Represented as a list of non-overlapping symbol ranges.
#[derive(Clone, Debug)]
pub struct RegExpSymbolSet {
    syms: Vec<RegExpSymbol>,
}

impl RegExpSymbolSet {
    pub fn symbols(&self) -> &[RegExpSymbol] {
        &self.syms
    }

    /// NOTE: Inverting will only newly include symbols that are valid chars (it
    /// won't start matching special symbols).
    pub fn inverted(&self) -> Self {
        let mut new_symbols = vec![];

        let mut last_end = 0;

        for symbol in &self.syms {
            if last_end != symbol.start {
                new_symbols.push(RegExpSymbol {
                    start: last_end,
                    end: symbol.start,
                });
            }

            last_end = symbol.end;
        }

        if last_end < std::char::MAX as u32 {
            new_symbols.push(RegExpSymbol {
                start: last_end,
                end: std::char::MAX as u32,
            });
        }

        Self { syms: new_symbols }
    }
}
