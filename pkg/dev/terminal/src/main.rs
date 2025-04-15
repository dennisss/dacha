#[macro_use]
extern crate macros;

use base_error::*;

#[executor_main]
async fn main() -> Result<()> {
    terminal::run_terminal_client().await?;

    Ok(())
}