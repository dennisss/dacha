#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    graphics::point_picker::run().await?;

    Ok(())
}
