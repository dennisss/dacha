use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use common::{bytes::Bytes, errors::*};
use file::{LocalPath, LocalPathBuf};
use http::uri::Uri;
use http::ClientInterface;

use crate::{ControlFile, PackagesFile, ReleaseFile, ReleaseFileEntry};

pub struct Repository {
    client: http::SimpleClient,
    root_url: Uri,
    dists: HashMap<String, Distribution>,
    cache_dir: LocalPathBuf,
}

impl Repository {
    pub async fn create(root_url: Uri, cache_dir: LocalPathBuf) -> Result<Self> {
        // TODO: Tune this to only use a single backend ip at a time
        // If there are multiple, only try one until it fails.
        let client = http::SimpleClient::new(http::SimpleClientOptions::default());

        Ok(Self {
            client,
            root_url,
            dists: HashMap::new(),
            cache_dir,
        })
    }

    async fn get_file<P: AsRef<LocalPath>>(&self, path: P) -> Result<Bytes> {
        let path = path.as_ref();
        let request = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri2(self.root_url.join(&path.as_str().parse()?)?)
            .build()?;

        // println!("GET {:?}", self.root_path.join(path).as_str());

        let mut request_context = http::ClientRequestContext::default();

        let mut response = self
            .client
            .request(&request.head, Bytes::new(), &request_context)
            .await?;
        if !response.ok() {
            return Err(format_err!("Request failed: {:?}", response.status()));
        }

        println!("=> Downloaded: {} bytes", response.body.len());

        Ok(response.body)
    }

    /*
    /binary-[arch]/Packages
     */

    pub async fn update(&mut self, distribution: &str, component: &str, arch: &str) -> Result<()> {
        let mut distribution = self.get_distribution(distribution).await?;

        let packages_file = {
            let packages = format!("{}/binary-{}/Packages", component, arch);
            let packages_gz = format!("{}.gz", packages);

            let data = {
                if distribution.index_files.contains_key(&packages_gz) {
                    let data = self.get_file(distribution.path.join(packages_gz)).await?;
                    let mut uncompressed = vec![];

                    compression::transform::transform_to_vec(
                        compression::gzip::GzipDecoder::new(),
                        &data,
                        &mut uncompressed,
                    )?;
                    Bytes::from(uncompressed)
                } else {
                    self.get_file(distribution.path.join(packages)).await?
                }
            };

            PackagesFile::parse(&data)?
        };

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

    /// Indexed from self.release.sha256()
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
