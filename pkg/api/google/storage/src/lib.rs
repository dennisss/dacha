/*
TODO: Do big uploads with parallel composite uuploads:
- https://cloud.google.com/storage/docs/parallel-composite-uploads

Resumable upload:
- https://cloud.google.com/storage/docs/performing-resumable-uploads

Streaming downloads:
- https://cloud.google.com/storage/docs/streaming-downloads
- (Need to do our own checksum checks though)

*/

use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use google_auth::GoogleRestClient;
use google_discovery_generated::storage_v1;

pub use google_discovery_generated::storage_v1::Object;

pub struct Client {
    raw: storage_v1::StorageClient,
}

impl Client {
    pub fn new(rest_client: Arc<GoogleRestClient>) -> Result<Self> {
        Ok(Self {
            raw: storage_v1::StorageClient::new(rest_client)?,
        })
    }

    pub async fn get(&self, bucket: &str, name: &str) -> Result<Object> {
        self.raw
            .objects_get(bucket, name, &storage_v1::ObjectsGetParameters::default())
            .await
    }

    pub async fn download(&self, bucket: &str, name: &str) -> Result<Box<dyn http::Body>> {
        self.raw
            .objects_get_download(bucket, name, &storage_v1::ObjectsGetParameters::default())
            .await
    }

    pub async fn upload(&self, bucket: &str, name: &str, data: Box<dyn http::Body>) -> Result<()> {
        let mut request = storage_v1::Object::default();

        let mut params = storage_v1::ObjectsInsertParameters::default();
        params.name = name.into();

        self.raw
            .objects_insert_with_upload(bucket, &request, &params, data)
            .await?;

        Ok(())
    }
}
