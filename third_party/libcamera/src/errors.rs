pub use nix::{Error, Result};

pub(crate) fn ok_if_zero(code: i32) -> Result<()> {
    if code != 0 {
        return Err(Error::from_i32(-code));
    }

    Ok(())
}

pub(crate) fn to_result(code: i32) -> Result<i32> {
    if code < 0 {
        return Err(Error::from_i32(-code));
    }

    Ok(code)
}
