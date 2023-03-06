pub type Error = sys::Errno;
pub type Result<T> = core::result::Result<T, Error>;

pub(crate) fn ok_if_zero(code: i32) -> Result<()> {
    if code != 0 {
        return Err(sys::Errno(-code as i64));
    }

    Ok(())
}

pub(crate) fn to_result(code: i32) -> Result<i32> {
    if code < 0 {
        return Err(sys::Errno(-code as i64));
    }

    Ok(code)
}
