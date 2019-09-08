pub use deflate::*;
pub use inflate::*;


pub struct Progress {
	/// Number of input bytes consumed during the update.
	pub input_read: usize,
	/// Number of output bytes written into the given buffer during the update.
	pub output_written: usize,
	/// If true, then all output has been written.
	pub done: bool
}


mod cyclic_buffer;
mod shared;
mod deflate;
mod inflate;