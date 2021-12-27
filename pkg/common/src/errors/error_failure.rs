pub use failure::err_msg;
pub use failure::format_err;
pub use failure::Error;

pub type Result<T, E = Error> = core::result::Result<T, E>;
