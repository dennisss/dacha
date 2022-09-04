use crate::deflate::cyclic_buffer::*;
use std::collections::HashMap;
use std::collections::VecDeque;

type Trigram = [u8; 3];

/*
    Core operations:
    - Given max_reference_distance

*/

pub struct AbsoluteReference {
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub struct RelativeReference {
    /// Number of bytes before the current position in the input data at which
    /// this reference occurs.
    pub distance: usize,

    ///
    pub length: usize,
}

pub struct MatchingWindowOptions {
    /// Maximum number of references to a single trigram which we will keep
    /// track of.
    pub max_chain_length: usize,

    /// Maximum length of a matching chunk of bytes which we can return.
    pub max_match_length: usize,
}

/// A buffer of past uncompressed input which is
pub struct MatchingWindow<B: WindowBuffer> {
    // TODO: We don't need to maintain a cyclic buffer if we have the entire input available to us
    // during compression time.
    buffer: B,

    options: MatchingWindowOptions,

    /// Map of three bytes in the back history to it's absolute position in the
    /// output buffer.
    ///
    /// The linked list is maintained in order of descending order of absolute
    /// position in the vector (such that closer matches are traversed first).
    trigrams: HashMap<Trigram, VecDeque<usize>>,
}

impl<B: WindowBuffer> MatchingWindow<B> {
    pub fn new(buffer: B, options: MatchingWindowOptions) -> Self {
        MatchingWindow {
            buffer,
            options,
            trigrams: HashMap::new(),
        }
    }

    // TODO: keep track of the total number of trigrams in the window.
    // If this number gets too large, then perform a full sweep of the table to GC
    // unused trigrams.

    /// NOTE: One should call this after the internal buffer has been updated.
    /// NOTE: We assume that the given offset is larger than any previously
    /// inserted offset.
    fn insert_trigram(&mut self, gram: Trigram, offset: usize) {
        if let Some(list) = self.trigrams.get_mut(&gram) {
            // Enforce max chain length and discard offsets before the start of the current
            // buffer.
            list.truncate(self.options.max_chain_length);
            while let Some(last_offset) = list.back() {
                if *last_offset < self.buffer.start_offset() {
                    list.pop_back();
                } else {
                    break;
                }
            }

            // NOTE: No attempt is made to validate that this offset hasn't already been
            // inserted.
            list.push_front(offset);

            if list.len() == 0 {
                self.trigrams.remove(&gram);
            }
        } else {
            let mut list = VecDeque::new();
            list.push_back(offset);
            self.trigrams.insert(gram, list);
        }
    }

    /// Given the next segment of uncompressed data, pushes it to the end of
    /// the window and in the process removing any data farther back the window
    /// size.
    pub fn advance(&mut self, data: &[u8]) {
        // TODO: If extending by more than the max_reference_distance, just wipe
        // the entire trigrams datastructure.
        self.buffer.extend_from_slice(data);

        // Index of the first new trigram
        let mut first = self
            .buffer
            .end_offset()
            .checked_sub(data.len() + 2)
            .unwrap_or(0);
        if first < self.buffer.start_offset() {
            first = self.buffer.start_offset();
        }

        // Index of the last new trigram.
        let last = self.buffer.end_offset().checked_sub(2).unwrap_or(0);

        for i in first..last {
            let gram = [self.buffer[i], self.buffer[i + 1], self.buffer[i + 2]];
            self.insert_trigram(gram, i);
        }
    }

    /// Given the next slice of unprocessed input data, attempts to match as
    /// many of the starting bytes of the new input data in the history of past
    /// inputs.
    ///
    /// NOTE: Will only ever return matches with >= 3 bytes.
    pub fn find_match(&self, data: &[u8]) -> Option<RelativeReference> {
        if data.len() < 3 {
            return None;
        }

        let mut best_match: Option<AbsoluteReference> = None;

        let gram = [data[0], data[1], data[2]];
        let offsets = match self.trigrams.get(&gram) {
            Some(l) => l,
            None => {
                return None;
            }
        };

        for off in offsets {
            // If off is too far back, then stop immediately as all later
            // ones will only be even further away.
            //
            // Note that because we don't truncate trigrams until it is seen again, it is
            // possible that all occurences are outside of the buffer if we haven't seen it
            // in a while.
            //
            // TODO: This is a good time to truncate the trigram buffer?
            if *off < self.buffer.start_offset() {
                break;
            }

            let s = self.buffer.slice_from(*off).append(data);

            // We trivially hae at least a match of 3 because we matched the trigram.
            let mut len = 3;
            for i in 3..s.len() {
                if i >= self.options.max_match_length || i >= data.len() || s[i] != data[i] {
                    len = i;
                    break;
                }
            }

            if let Some(m) = &best_match {
                // NOTE: Even if they are equal, we prefer to use a later lower
                // distance match of the same length.
                if m.length > len {
                    continue;
                }
            }

            best_match = Some(AbsoluteReference {
                offset: *off,
                length: len,
            });
        }

        // Converting from absolute offset to relative distance.
        best_match.map(|r| RelativeReference {
            distance: self.buffer.end_offset() - r.offset,
            length: r.length,
        })
    }
}
