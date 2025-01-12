#[macro_use]
extern crate regexp_macros;

pub mod excellon;
mod expression;
pub mod graphics;
pub mod processor;
pub mod syntax;

pub use excellon::*;
pub use graphics::*;
pub use processor::*;
pub use syntax::{Command, File};

use base_error::*;
use file::LocalPath;

/// Reads a file from disk, interpretes it as a gerber file and outputs the
/// contained graphics objects.
pub async fn read(
    path: &LocalPath,
    options: CommandsProcessorOptions,
) -> Result<Vec<GraphicsObject>> {
    let data = file::read(path).await?;

    let f = File::parse(&data)?;

    let mut processor = CommandsProcessor::create(options)?;

    let mut objs = vec![];

    for cmd in f.commands {
        processor.process(&cmd, &mut objs)?;
    }

    Ok(objs)
}
