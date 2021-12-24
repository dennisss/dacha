pub use failure::err_msg;
pub use failure::format_err;
pub use failure::Error;

pub type Result<T> = std::result::Result<T, Error>;
