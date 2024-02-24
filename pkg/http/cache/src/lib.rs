#[macro_use]
extern crate common;

use std::time::{SystemTime, UNIX_EPOCH};

use common::{errors::*, io::Writeable};
use crypto::{hasher::Hasher, random::SharedRng, sha256::SHA256Hasher};
use datastore_meta_client::key_encoding::KeyEncoder;
use file::{LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf};
use http_cache_proto::RequestCacheEntry;
use protobuf::{Message, StaticMessage};
use sstable::{db::WriteBatch, iterable::Iterable, EmbeddedDB, EmbeddedDBOptions};

const REQUESTS_TABLE_ID: u64 = 1;

// TODO: Make sure that we don't cache any transport level headers.

// pub struct DiskCacheOptions {
//     pub cached_statuses: Vec<http::status_code::StatusCode>,
// }

/*
TODO: For HTTP1, we seem to be consistently creating new connections.

e.g. when sending to http://deb.debian.org/debian/pool/main/z/zlib/zlib1g_1.2.13.dfsg-1_arm64.deb

We don't seme to be able to handle the keep-alive

*/

/// On-disk cache of persistently storing and serving HTTP requests/responses.
pub struct DiskCache {
    client: http::SimpleClient,
    metadata: EmbeddedDB,
    blobs_dir: LocalPathBuf,
    temp_dir: LocalPathBuf,
}

impl DiskCache {
    pub async fn open(client: http::SimpleClient, dir: &LocalPath) -> Result<Self> {
        let mut options = EmbeddedDBOptions::default();
        options.create_if_missing = true;
        options.error_if_exists = false;

        let metadata = EmbeddedDB::open(dir.join("metadata"), options).await?;

        let blobs_dir = dir.join("blobs");
        file::create_dir_all(&blobs_dir).await?;

        let temp_dir = dir.join("temp");
        file::create_dir_all(&temp_dir).await?;

        Ok(Self {
            client,
            metadata,
            blobs_dir,
            temp_dir,
        })
    }

    // TODO: Need a timeout for how long we allow requesting the real response.
    // (this would prevent attempting to accidentally cache unbounded requests like
    // websockets or long polling stuff).

    // TODO: Implement storage of the request body if given.
    pub async fn request(&self, request: http::Request) -> Result<http::Response> {
        let uri = request.head.uri.to_string()?;

        // TODO: Need to acquire an in-memory lock on the URL to prevent other
        // requestors from making redundant requests.

        // TODO: Also need a lock on the blob with the same hash.

        if let Some(entry) = self.get_latest_cache_entry(&uri).await? {
            return self.response_for_entry(&entry).await;
        }

        println!("Cache Miss: {}", request.head.uri.to_string()?);

        let mut entry = RequestCacheEntry::default();

        entry.set_timestamp_millis(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        );
        entry.request_mut().set_method(request.head.method.as_str());
        entry.request_mut().set_url(request.head.uri.to_string()?);

        for header in &request.head.headers.raw_headers {
            let proto = entry.request_mut().new_headers();
            proto.set_name(header.name.as_str());
            proto.set_value(std::str::from_utf8(header.value.as_bytes())?);
        }

        let mut response = self
            .client
            .request_raw(request, http::ClientRequestContext::default())
            .await?;

        // TODO: Allow customizing what the behavior should be when given a
        // non-cacheable response.
        if !response.ok() {
            return Err(format_err!("Proxy request failed: {:?}", response.status()));
        }

        entry
            .response_mut()
            .set_status_code(response.head.status_code.as_u16() as u32);

        for header in &response.head.headers.raw_headers {
            if header.is_transport_level() {
                continue;
            }

            let proto = entry.response_mut().new_headers();
            proto.set_name(header.name.as_str());
            proto.set_value(std::str::from_utf8(header.value.as_bytes())?);
        }

        let mut temp_id = vec![0u8; 16];
        crypto::random::global_rng()
            .generate_bytes(&mut temp_id)
            .await;

        let temp_path = self.temp_dir.join(base_radix::hex_encode(&temp_id));

        // TODO: Need to have a special fast path for empty bodies.
        {
            let mut temp_file = LocalFile::open_with_options(
                &temp_path,
                &LocalFileOpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true),
            )?;

            let mut writer = HashedWriteable {
                hasher: SHA256Hasher::default(),
                inner: temp_file,
            };

            response.body.pipe(&mut writer).await?;

            writer.flush().await?;

            entry.response_mut().set_body_sha256(writer.hasher.finish());
        }

        // TODO: Dedup this code
        let blob_path = self
            .blobs_dir
            .join(base_radix::hex_encode(entry.response().body_sha256()));

        file::rename(&temp_path, &blob_path).await?;

        // TODO: Cache response trailers.

        self.put_cache_entry(&entry).await?;

        self.response_for_entry(&entry).await
    }

    async fn response_for_entry(&self, entry: &RequestCacheEntry) -> Result<http::Response> {
        let body = {
            if entry.response().body_sha256().is_empty() {
                http::EmptyBody()
            } else {
                let blob_path = self
                    .blobs_dir
                    .join(base_radix::hex_encode(entry.response().body_sha256()));
                Box::new(http::static_file_handler::StaticFileBody::open(&blob_path).await?)
            }
        };

        let mut response_builder = http::ResponseBuilder::new()
            .status(
                http::status_code::StatusCode::from_u16(entry.response().status_code() as u16)
                    .unwrap(),
            )
            .body(body);

        for header in entry.response().headers() {
            response_builder = response_builder.header(header.name(), header.value());
        }

        response_builder.build()
    }

    async fn put_cache_entry(&self, entry: &RequestCacheEntry) -> Result<()> {
        let mut entry = entry.clone();

        let mut batch = WriteBatch::new();

        let mut key = vec![];
        KeyEncoder::encode_varuint(REQUESTS_TABLE_ID, false, &mut key);
        KeyEncoder::encode_bytes(entry.request().url().as_bytes(), &mut key);
        KeyEncoder::encode_varuint(entry.timestamp_millis(), true, &mut key);

        entry.request_mut().clear_url();
        entry.clear_timestamp_millis();

        let value = entry.serialize()?;
        batch.put(&key, &value);

        self.metadata.write(&batch).await?;

        Ok(())
    }

    async fn get_latest_cache_entry(&self, url: &str) -> Result<Option<RequestCacheEntry>> {
        let mut start_key = vec![];
        KeyEncoder::encode_varuint(REQUESTS_TABLE_ID, false, &mut start_key);
        KeyEncoder::encode_bytes(url.as_bytes(), &mut start_key);

        let mut iter = self.metadata.snapshot().await.iter().await?;
        iter.seek(&start_key).await?;

        let entry = match iter.next().await? {
            Some(v) => v,
            None => return Ok(None),
        };

        if !entry.key.starts_with(&start_key.as_ref()) {
            return Ok(None);
        }

        let value = match entry.value {
            Some(v) => v,
            None => return Ok(None),
        };

        Ok(Some(RequestCacheEntry::parse(&value)?))
    }
}

pub struct HashedWriteable<W: Writeable> {
    hasher: crypto::sha256::SHA256Hasher,
    inner: W,
}

#[async_trait]
impl<W: Writeable> Writeable for HashedWriteable<W> {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.hasher.update(data);

        self.inner.write(data).await
    }

    async fn flush(&mut self) -> Result<()> {
        self.inner.flush().await
    }
}
