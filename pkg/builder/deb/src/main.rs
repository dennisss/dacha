#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::collections::{BTreeMap, HashMap};

use common::errors::*;
use deb::ReleaseFile;
use file::{LocalPath, LocalPathBuf};
use http::uri::Uri;
use http::{ClientInterface, ClientRequestContext, RequestBuilder};

/*
Some operations to support:
- Mirror a repository to a directory on disk.
    - With all metadata.
    - Sanity check that there is no directory traversal issue.
- Should verify that all dependencies exist in the repo?

- Given a reference to a repo, I may want to install a single package and all dependencies (that aren't already installed)
    - Must respect

Repositories I want to cache:
    deb http://deb.debian.org/debian RELEASE main contrib non-free non-free-firmware
    deb http://deb.debian.org/debian-security/ RELEASE-security main contrib non-free non-free-firmware
    deb http://deb.debian.org/debian RELEASE-updates main contrib non-free non-free-firmware

    deb http://archive.raspberrypi.com/debian/ RELEASE main

*/

/*
deb http://archive.raspberrypi.org/debian/ bullseye main
*/

// TODO: Whenever downloading a file, validate the size before fully reading the
// body (either against a sane limit or the index file)

#[executor_main]
async fn main() -> Result<()> {
    /*
    let root_url = "http://archive.raspberrypi.org/debian/".parse::<Uri>()?;

    // TODO: Tune this to only use a single backend ip at a time
    // If there are multiple, only try one until it fails.
    let client = http::Client::create(root_url.clone()).await?;

    let request = RequestBuilder::new()
        .method(http::Method::GET)
        .path(
            LocalPath::new(root_url.path.as_str())
                .join("dists/bullseye/Release")
                .as_str(),
        )
        .build()?;

    let mut request_context = ClientRequestContext::default();

    let mut response = client.request(request, request_context).await?;

    if !response.ok() {
        return Err(format_err!("Request failed: {:?}", response.status()));
    }

    let mut release = String::new();
    response.body.read_to_string(&mut release).await?;


    /debian/dists/bullseye/main/binary-arm64/Packages
    */
    let cache_dir = LocalPath::new("/nonexistent").to_owned();

    {
        let mut repo = deb::Repository::create(
            "http://archive.raspberrypi.org/debian/".parse::<Uri>()?,
            cache_dir.clone(),
        )
        .await?;

        repo.update("bullseye", "main", "arm64").await?;
    }

    {
        let mut repo = deb::Repository::create(
            "http://deb.debian.org/debian".parse::<Uri>()?,
            cache_dir.clone(),
        )
        .await?;

        repo.update("bullseye", "main", "arm64").await?;
        repo.update("bullseye", "contrib", "arm64").await?;
    }

    /*
       let release = file::read_to_string(project_path!("third_party/raspbian/Release")).await?;
       let release = ReleaseFile::try_from(deb::ControlFile::parse(&release)?)?;

       println!("Components: {:?}", release.components());
       println!("Architectures: {:?}", release.architectures());
    */

    Ok(())
}
