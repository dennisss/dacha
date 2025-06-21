#[macro_use]
extern crate common;

mod scope;
mod pipe;
mod table;

pub use scope::*;
pub use pipe::*;
pub use table::*;

pub fn start_hyperlink(url: &str) -> String {
    let params = "";
    format!("\x1B]8;{};{}\x1B\\", params, url)
}