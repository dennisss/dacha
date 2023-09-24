#[macro_use]
extern crate macros;
extern crate common;
extern crate file;

use common::errors::*;
use file::LocalPathBuf;

#[executor_main]
async fn main() -> Result<()> {
    let output_dir = LocalPathBuf::from(std::env::var("OUT_DIR")?);

    // TODO: Print cargo file changed statements so that this re-compiles when the
    // API discovery files change

    let mut client = google_discovery::Client::cache_only()?;
    let out = client.compile_all().await?;

    file::write(output_dir.join("generated.rs"), out).await?;

    Ok(())
}
