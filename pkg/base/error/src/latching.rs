use crate::{err_msg, Error, Result};

pub enum LatchingError {
    NoError,
    FirstError(Error),
    PreviousErrors,
}

impl Default for LatchingError {
    fn default() -> Self {
        Self::NoError
    }
}

impl LatchingError {
    pub fn set(&mut self, error: Error) {
        *self = LatchingError::FirstError(error);
    }

    pub fn get(&mut self) -> Result<()> {
        let mut v = LatchingError::PreviousErrors;
        core::mem::swap(self, &mut v);

        match v {
            LatchingError::NoError => {
                *self = LatchingError::NoError;
                Ok(())
            }
            LatchingError::FirstError(e) => {
                *self = LatchingError::PreviousErrors;
                Err(e)
            }
            LatchingError::PreviousErrors => {
                *self = LatchingError::PreviousErrors;
                Err(err_msg("Previous errors occured so in a terminal state."))
            }
        }
    }

    pub fn is_err(&self) -> bool {
        match self {
            LatchingError::NoError => false,
            LatchingError::FirstError(_) => true,
            LatchingError::PreviousErrors => true,
        }
    }
}
