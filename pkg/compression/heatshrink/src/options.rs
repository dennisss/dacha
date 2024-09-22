use common::errors::*;


#[derive(Clone)]
pub struct Options {
    pub window_bits: usize,
    pub lookahead_bits: usize,
}

impl Options {
    pub fn validate(&self) -> Result<()> {
        if self.window_bits < 4 || self.window_bits > 15 {
            return Err(err_msg("Bad window bits"));
        }

        if self.lookahead_bits < 3 || self.lookahead_bits > 32 {
            return Err(err_msg("Bad lookahead bits"));
        }

        Ok(())
    }
}
