/*
    Mainly implements uploading and fetching o urls from the directory library

*/

use std::io::Cursor;
use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::bytes::Bytes;
use common::errors::*;
use common::futures::future::*;
use common::futures::prelude::*;
use common::FlipSign;
use protobuf_json::MessageJsonParser;

use crate::cache::api::*;
use crate::directory::*;
use crate::paths::*;
use crate::proto::service::*;
use crate::store::api::*;
use crate::types::*;

pub struct Client {
    dir: Arc<Mutex<Directory>>,
}

#[derive(Clone)]
pub struct PhotoChunk {
    pub alt_key: NeedleAltKey,
    pub data: Bytes,
}

impl Client {
    pub fn create(dir: Directory) -> Client {
        Client {
            dir: Arc::new(Mutex::new(dir)),
        }
    }

    pub async fn cluster_id(&self) -> String {
        let dir = self.dir.lock().await;
        String::from("Hello world")
        //serialize_urlbase64(&dir.cluster_id)
    }

    /// Gets a url to read a photo from the cache layer
    pub async fn read_photo_cache_url(&self, keys: &NeedleKeys) -> Result<String> {
        let dir = self.dir.lock().await;

        let photo = match dir.db.read_photo(keys.key)? {
            Some(p) => p,
            None => return Err(err_msg("No such photo")),
        };

        let vol = match dir.db.read_logical_volume(photo.volume_id.flip())? {
            Some(v) => v,
            None => return Err(err_msg("Missing the volume")),
        };

        let cache = dir.choose_cache(&photo, &vol)?;
        let store = dir.choose_store(&photo)?;

        let path = CachePath::Proxy {
            machine_ids: MachineIds::Data(vec![store.id.flip()]),
            store: StorePath::Needle {
                volume_id: vol.id.flip(),
                key: keys.key,
                alt_key: keys.alt_key,
                cookie: CookieBuf::from(&photo.cookie[..]),
            },
        };

        let host = Host::Cache(cache.id.flip());

        Ok(format!(
            "http://{}:{}{}",
            host.to_string(),
            cache.addr_port,
            path.to_string()
        ))
    }

    /// Creates a new photo containing all of the given chunks
    /// TODO: On writeability errors, relocate the photo to a new volume that
    /// doesn't have the given machines
    pub async fn upload_photo(&self, chunks: Vec<PhotoChunk>) -> Result<NeedleKey> {
        assert!(chunks.len() > 0);

        let dir = self.dir.lock().await;

        let cookie = CookieBuf::random().await?;

        let vol = dir.choose_logical_volume_for_write()?;

        let p = dir.db.create_photo(&models::NewPhoto {
            volume_id: vol.id,
            cookie: cookie.data(),
        })?;

        let machines = dir.db.read_store_machines_for_volume(p.volume_id.flip())?;

        if machines.len() == 0 {
            return Err(err_msg("Missing any machines to upload to"));
        }

        for m in machines.iter() {
            if !m.can_write(&dir.config) {
                return Err(err_msg("Some machines are not writeable"));
            }
        }

        let needles = chunks
            .into_iter()
            .map(|c| NeedleChunk {
                path: NeedleChunkPath {
                    volume_id: p.volume_id.flip(),
                    key: p.id.flip(),
                    alt_key: c.alt_key,
                    cookie: cookie.clone(),
                },
                data: c.data.clone(),
            })
            .collect::<Vec<_>>();

        // at this point, we have a batch of needles and machines that we'd need to sent
        // it to.

        // TODO: On failure of a request, retry the request once
        // TODO: On failure of the retried request, bail out and choose a new volume to
        // contain our photo (basically rerunning most of this upload_photo function)

        let num = needles.len();

        let photo_id = needles[0].path.key;

        let arr = machines
            .into_iter()
            .map(move |m| {
                let needles = (&needles[..]).to_vec();
                let m = Arc::new(m);

                async move {
                    // Client::upload_needle_sequential(&m, needles)
                    let n = Client::upload_needle_batch(&m, &needles).await?;
                    if num != n {
                        return Err(err_msg("Not all chunks uploaded"));
                    }

                    Ok(())
                }
            })
            .collect::<Vec<_>>();

        // TODO: Verify all of them are successful
        common::futures::future::join_all(arr).await;

        Ok(photo_id)
    }

    /// Uploads many chunks using traditional sequential requests (flushed after
    /// every single request) TODO: Currently this will never respond with a
    /// partial count
    async fn upload_needle_sequential(
        mac: &models::StoreMachine,
        chunks: &[NeedleChunk],
    ) -> Result<usize> {
        let client = http::Client::create(http::ClientOptions::from_uri(&mac.addr().parse()?)?)?;
        let mac_id = mac.id as MachineId;

        for c in chunks {
            let req = http::RequestBuilder::new()
                .method(http::Method::POST)
                .path(
                    StorePath::Needle {
                        volume_id: c.path.volume_id,
                        key: c.path.key,
                        alt_key: c.path.alt_key,
                        cookie: c.path.cookie.clone(),
                    }
                    .to_string(),
                )
                .header("Host", Host::Store(mac_id).to_string())
                .body(http::BodyFromData(c.data.clone()))
                .build()?;

            let res = client.request(req).await?;
            if !res.ok() {
                return Err(format_err!(
                    "Received status {:?} while uploading",
                    res.status()
                ));
            }
        }

        Ok(chunks.len())
    }

    /// Uploads some number of chunks to a single machine/volume and returns how
    /// many of the chunks succeeded in being flushed to the volume
    async fn upload_needle_batch(
        mac: &models::StoreMachine,
        chunks: &[NeedleChunk],
    ) -> Result<usize> {
        let mut body_parts = vec![];

        for c in chunks {
            let mut header = vec![];
            c.write_header(&mut Cursor::new(&mut header))
                .expect("Failure making chunk header");

            body_parts.push(Bytes::from(header));
            body_parts.push(c.data.clone());
        }

        let client = http::Client::create(http::ClientOptions::from_uri(&mac.addr().parse()?)?)?;

        let req = http::RequestBuilder::new()
            .method(http::Method::PATCH)
            .path(StorePath::Index.to_string())
            .header("Host", Host::Store(mac.id as MachineId).to_string())
            .body(http::BodyFromParts(body_parts.into_iter()))
            .build()?;

        let mut resp = client.request(req).await?;

        if !resp.ok() {
            return Err(format_err!("Request failed with code: {:?}", resp.status()).into());
        }

        let mut resp_body = vec![];
        resp.body.read_to_end(&mut resp_body).await?;

        let resp_body_str = std::str::from_utf8(&resp_body)?;

        let res = StoreWriteBatchResponse::parse_json(
            resp_body_str,
            &protobuf_json::ParserOptions::default(),
        )
        .map_err(|_| err_msg("Invalid json response received"))?;

        if res.has_error() {
            eprintln!("Upload error: {:?}", res.error());
        }

        let num = res.num_written();

        Ok(num as usize)
    }

    pub fn get_photo_cache_url() {
        // This is where the distributed hashtable stuff will come into actual
    }

    pub fn get_photo_store_url() {}
}
