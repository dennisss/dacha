use std::collections::HashSet;

use common::errors::*;

use crate::fsm::*;
use crate::regexp::instance::*;
use crate::regexp::symbol::*;
use crate::regexp::state_machine::*;


// NOTE: Visibility is provided to the RegExp class.
pub struct RegExpMatch<'a, 'b> {
    /// The RegExp being used for matching.
    pub(crate) instance: &'a RegExp,

    /// The complete value that was initially given for matching.
    pub(crate) value: &'b [u8],

    /// Start index into 'value' of the current match.
    pub(crate) index: usize,

    pub(crate) remaining: &'b [u8],
    pub(crate) consumed_start: bool,
    pub(crate) consumed_end: bool,

    pub(crate) state: StateId,
    pub(crate) group_starts: Vec<Option<usize>>,
    pub(crate) group_values: Vec<Option<&'b [u8]>>,
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

            let sym = match self.remaining.first().cloned() {
                Some(c) => {
                    self.remaining = &self.remaining[1..];
                    self.instance.alphabet.get(c as char)
                },
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