use std::io;

pub type ClusterId = [u8; 16];

pub const COOKIE_SIZE: usize = 8;
pub type Cookie = [u8; COOKIE_SIZE];




/// XXX: Basically everything below here is currently unused


pub enum HaystackResource {
	Volume,
	Needle
}

pub enum HaystackSegment {
	NeedleHeaderMagic,
	NeedleFooterMagic,
	NeedleDataChecksum
}


// See https://doc.rust-lang.org/rust-by-example/error/multiple_error_types/wrap_error.html for wrapping error

// Other things
// - Invalid cookie
// - malformed 
pub enum HaystackError {
	BadRequest,
	Corrupt(HaystackSegment),
	NotFound(HaystackResource),
	Deleted,
	Io(io::Error)
}

impl From<io::Error> for HaystackError {
    fn from(err: io::Error) -> HaystackError {
        HaystackError::Io(err)
    }
}

pub type HaystackResult<T> = std::result::Result<T, HaystackError>;

