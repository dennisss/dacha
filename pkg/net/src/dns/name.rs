use common::errors::*;

// NOTE: For efficiency, the length check is handled separately.
// TODO: The '_' character is only allows in SRV records?
regexp!(LABEL => "^[_A-Za-z](?:-?[A-Za-z0-9])*$");

/// A name is a string composed by dot limited labels
/// - Each label is at most 63 characters
/// - The entire name is at most 255 octets long when binary serialized (sum of
///   label lengths + 1 length octet per label).
#[derive(Clone)]
pub struct Name<'a> {
    // // Will be at most 255 octets and fully uncompressed.
    // value: Vec<u8>

    // Invariant: Name objects will only be constructed with valid label lists (all lengths and
    // characters are valid).
    labels: Vec<&'a str>,
}

impl<'a> Name<'a> {
    /// Parses a serialized name which starts at the beginning of 'input'.
    ///
    /// Arguments:
    /// - input: Current position at which we are parsing
    /// - message: Complete DNS message (used to resolve absolute pointers).
    pub fn parse(input: &'a [u8], message: &'a [u8]) -> Result<(Self, &'a [u8])> {
        let mut iter = LabelIterator {
            next_label: input,
            message,

            inline_bytes: 0,
            total_bytes: 0,
            followed_pointer: false,
        };

        // Iterate over the raw format and
        let mut labels = vec![];
        while let Some(label) = iter.next()? {
            labels.push(label);
        }

        Ok((Self { labels }, &input[iter.consumed_bytes()..]))
    }

    pub fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {
        for label in &self.labels {
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0);

        Ok(())
    }

    pub fn labels(&self) -> &[&str] {
        &self.labels
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new(); // Will be up to 254 characters.
        for label in &self.labels {
            s.push_str(label);
            s.push('.');
        }

        s
    }
}

impl<'a> TryFrom<&'a str> for Name<'a> {
    type Error = Error;

    fn try_from(s: &'a str) -> Result<Self> {
        let mut labels = vec![];

        let mut nbytes = 0;
        let mut done = false;
        for label in s.split('.') {
            if label.len() == 0 {
                if done {
                    return Err(err_msg("Multiple empty labels in name"));
                }

                done = true;
                continue;
            }

            if label.len() > 63 {
                return Err(err_msg("Label is too long"));
            }

            nbytes += 1 + label.len();
            if nbytes > 254 {
                return Err(err_msg("Name is too long"));
            }

            if !LABEL.test(label) {
                return Err(format_err!("Invalid characters in label: '{}'", label));
            }

            labels.push(label);
        }

        if !done {
            return Err(err_msg("Expected name to end up a dot"));
        }

        Ok(Self { labels })
    }
}

impl<'a> PartialEq for Name<'a> {
    fn eq(&self, other: &Self) -> bool {
        if self.labels.len() != other.labels.len() {
            return false;
        }

        for (a, b) in self.labels.iter().zip(other.labels.iter()) {
            if !a.eq_ignore_ascii_case(*b) {
                return false;
            }
        }

        true
    }
}

impl<'a> std::fmt::Display for Name<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl<'a> std::fmt::Debug for Name<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

pub struct NameEncoder {
    encoded_names: Vec<Vec<u16>>,
}

impl NameEncoder {
    pub fn new() -> Self {
        Self {
            encoded_names: vec![],
        }
    }

    /// NOTE: This assumes that it is always called with the exact same message
    /// buffer passed in previous calls to encode() on the same encoder
    /// instance.
    pub fn encode(&mut self, name: Name, message: &mut Vec<u8>) {
        if name.labels.len() == 0 {
            message.push(0);
            return;
        }

        let mut best_match: &[u16] = &[];

        for encoded_name in &self.encoded_names {
            let mut i = name.labels.len();
            let mut j = encoded_name.len();

            while i >= 1 && j >= 1 {
                let encoded_off = encoded_name[j - 1] as usize;
                let encoded_len = message[encoded_off] as usize;
                let encoded_data = &message[(encoded_off + 1)..(encoded_off + 1 + encoded_len)];

                if encoded_data.eq_ignore_ascii_case(name.labels[i - 1].as_bytes()) {
                    i -= 1;
                    j -= 1;
                } else {
                    break;
                }
            }

            // We didn't match any labels in this case.
            if name.labels.len() - i == 0 {
                continue;
            }

            let m = &encoded_name[j..];
            if m.len() > best_match.len() {
                best_match = m;
            }
        }

        let mut offsets = vec![];

        // Append all labels occuring before the match
        for i in 0..(name.labels().len() - best_match.len()) {
            let s = name.labels[i].as_bytes();
            offsets.push(message.len() as u16);
            message.push(s.len() as u8);
            message.extend_from_slice(s);
        }

        // Append the suffix.
        if best_match.len() > 0 {
            let ptr = best_match[0] | (0b11 << 14);
            message.extend_from_slice(&ptr.to_be_bytes());

            offsets.extend_from_slice(best_match);
        } else {
            message.push(0);
        }

        self.encoded_names.push(offsets);
    }
}

struct LabelIterator<'a> {
    next_label: &'a [u8],
    message: &'a [u8],

    inline_bytes: u8,
    total_bytes: u8,
    followed_pointer: bool,
}

impl<'a> LabelIterator<'a> {
    fn consume_bytes(&mut self, num: usize) -> Result<()> {
        self.total_bytes = self
            .total_bytes
            .checked_add(num as u8)
            .ok_or_else(|| err_msg("Name is too long"))?;

        if !self.followed_pointer {
            // NOTE: Will never overflow if total_bytes didn't overflow
            self.inline_bytes += num as u8;
        }

        Ok(())
    }

    fn consumed_bytes(&self) -> usize {
        self.inline_bytes as usize
    }

    fn next(&mut self) -> Result<Option<&'a str>> {
        let len = parse_next!(self.next_label, parsing::binary::be_u8) as usize;
        self.consume_bytes(1)?;

        if len >> 6 == 0b11 {
            let len2 = parse_next!(self.next_label, parsing::binary::be_u8) as usize;
            self.consume_bytes(1)?;

            // 16-bit offset
            let offset = ((len & 0b111111) << 8) | len2;

            self.next_label = &self.message[offset..];
            self.followed_pointer = true;
            return self.next();
        }

        self.consume_bytes(len)?;

        if len == 0 {
            return Ok(None);
        }

        if len > 63 {
            return Err(err_msg("Label is too long"));
        }

        let label = parse_next!(self.next_label, parsing::take_exact(len));
        if !LABEL.test(label) {
            return Err(format_err!("Invalid label characters: {:?}", label));
        }

        // Guranteed to be UTF-8 by the regexp.
        Ok(Some(std::str::from_utf8(label).unwrap()))
    }
}
