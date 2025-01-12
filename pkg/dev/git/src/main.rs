#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use base_error::*;

#[executor_main]
async fn main() -> Result<()> {
    let v = git::read_index().await?;

    for entry in &v.entries {
        println!("{}", entry.name);
    }

    Ok(())
}
