use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use common::errors::*;
use file::{LocalPath, LocalPathBuf};
use http::uri::Uri;
use http::ClientInterface;

use crate::{ControlFile, PackagesFile, ReleaseFile, ReleaseFileEntry};

pub struct Repository {
    client: http::Client,
    root_path: LocalPathBuf,
    dists: HashMap<String, Distribution>,
}

impl Repository {
    pub fn create(root_url: Uri) -> Result<Self> {
        // TODO: Tune this to only use a single backend ip at a time
        // If there are multiple, only try one until it fails.
        let client = http::Client::create(root_url.clone())?;

        Ok(Self {
            client,
            root_path: root_url.path.as_str().into(),
            dists: HashMap::new(),
        })
    }

    async fn get_file<P: AsRef<LocalPath>>(&self, path: P) -> Result<Vec<u8>> {
        let request = http::RequestBuilder::new()
            .method(http::Method::GET)
            .path(self.root_path.join(path).as_str())
            .build()?;

        let mut request_context = http::ClientRequestContext::default();

        let mut response = self.client.request(request, request_context).await?;
        if !response.ok() {
            return Err(format_err!("Request failed: {:?}", response.status()));
        }

        let mut out = vec![];
        response.body.read_to_end(&mut out).await?;
        Ok(out)
    }

    /*
    /binary-[arch]/Packages
     */

    pub async fn update(&mut self, distribution: &str, component: &str, arch: &str) -> Result<()> {
        let mut distribution = self.get_distribution(distribution).await?;

        let packages_path = distribution
            .path
            .join(component)
            .join(format!("binary-{}", arch))
            .join("Packages");

        // TODO: Optionally use the .gz version
        let packages_file = PackagesFile::parse(&self.get_file(&packages_path).await?)?;

        let mut total_size = 0;
        for pkg in packages_file.packages() {
            total_size += pkg.size()?;
        }

        println!("Size: {:?}", base_units::ByteCount::from(total_size));

        // distribution.components.insert(componen, v)

        // for

        Ok(())
    }

    async fn get_distribution(&mut self, name: &str) -> Result<Distribution> {
        let path = LocalPath::new("dists").join(name);

        let release_data = self.get_file(path.join("Release")).await?;
        let release =
            ReleaseFile::try_from(ControlFile::parse(std::str::from_utf8(&release_data)?)?)?;

        let mut files = BTreeMap::new();

        for file in release.sha256()? {
            files.insert(file.path.clone(), file);
        }

        let dist = Distribution {
            path,
            release,
            index_files: files,
            components: HashMap::new(),
        };

        Ok(dist)
    }
}

pub struct Distribution {
    path: LocalPathBuf,
    release: ReleaseFile,
    index_files: BTreeMap<String, ReleaseFileEntry>,
    components: HashMap<String, PackagesFile>,
}

/*
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

*/
