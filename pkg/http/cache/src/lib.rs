#[macro_use]
extern crate common;

mod table;

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use common::{errors::*, io::Writeable};
use crypto::{hasher::Hasher, random::SharedRng, sha256::SHA256Hasher};
use db_table::{db::ProtobufDB, query_one};
use db_txn::TransactionalDB;
use file::{LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf};
use http_cache_proto::RequestCacheEntry;
use protobuf::{Message, StaticMessage};
use table::RequestCacheEntryTable;

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
    metadata: ProtobufDB,
    blobs_dir: LocalPathBuf,
    temp_dir: LocalPathBuf,
}

impl DiskCache {
    pub async fn open(client: http::SimpleClient, dir: &LocalPath) -> Result<Self> {
        if !file::exists(dir).await? {
            file::create_dir(dir).await?;
        }

        let metadata = TransactionalDB::create_local(&dir.join("metadata")).await?;
        let blobs_dir = dir.join("blobs");
        file::create_dir_all(&blobs_dir).await?;

        let temp_dir = dir.join("temp");
        file::create_dir_all(&temp_dir).await?;

        Ok(Self {
            client,
            metadata: ProtobufDB::new(Arc::new(metadata)),
            blobs_dir,
            temp_dir,
        })
    }

    // TODO: Need some amount of url normalization.

    // TODO: Need a timeout for how long we allow requesting the real response.
    // (this would prevent attempting to accidentally cache unbounded requests like
    // websockets or long polling stuff).

    // TODO: Implement storage of the request body if given.
    pub async fn request(&self, request: http::Request) -> Result<http::Response> {
        let uri = request.head.uri.to_string()?;

        // TODO: Need to acquire an in-memory lock on the URL to prevent other
        // requestors from making redundant requests.

        // TODO: Also need a lock on the blob with the same hash.

        if let Some(entry) = query_one!(
            self.metadata,
            RequestCacheEntryTable,
            "request.url = ?",
            &uri
        ) {
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

        self.metadata
            .insert::<RequestCacheEntryTable>(&entry)
            .await?;

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
                // TODO: Implement the support for Content-Range headers here.
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
