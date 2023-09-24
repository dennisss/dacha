// Re-fetches all the Discovery API JSON data from Google servers.
// (normally while building programs, we only use a cached copy)

#[macro_use]
extern crate macros;

use common::errors::*;
use file::project_path;

#[executor_main]
async fn main() -> Result<()> {
    let mut client = google_discovery::Client::new(google_discovery::ClientOptions {
        cache_directory: Some(project_path!("third_party/google/discovery/data")),
        cached_only: false,
    })?;

    client.compile_all().await?;

    Ok(())
}
