use common::errors::*;

define_bit_flags!(
    Flags u8 {
        // If set, we will ignore differences in ASCII upper/lower casing when doing matching.
        CASE_INSENSITIVE = (1 << 0)
    }
);

impl Flags {
    pub fn parse_from(s: &str) -> Result<Self> {
        let mut val = Self::empty();

        for c in s.chars() {
            val |= match c {
                'i' => Self::CASE_INSENSITIVE,
                _ => return Err(format_err!("Unknown flag character: {}", c)),
            };
        }

        Ok(val)
    }

    pub fn codegen(&self) -> String {
        format!(
            "::automata::regexp::vm::flags::Flags::from_raw({})",
            self.to_raw()
        )
    }
}
