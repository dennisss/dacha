extern crate bittorrent;
extern crate common;
extern crate crypto;

use std::str::FromStr;

use bittorrent::ben::BENValue;
use bittorrent::*;
use common::async_std::task;
use common::errors::*;
use crypto::hasher::Hasher;
use parsing::ascii::AsciiString;

async fn run() -> Result<()> {
    let data = std::fs::read(
        common::project_dir()
            .join("pkg/bittorrent/2020-08-20-raspios-buster-armhf-full.zip.torrent"),
    )?;

    let info = Metainfo::parse(&data)?;
    // TODO: Verify non-syntanctic parts of the file.

    let info_hash = {
        // TODO: Eventually support getting this as a slice of the input data instead of
        // reserializing it.
        let mut data = vec![];
        let info_ben: BENValue = info.info.clone().into();
        info_ben.serialize(&mut data);

        let mut hasher = crypto::sha1::SHA1Hasher::default();
        hasher.update(&data);
        hasher.finish()
    };

    let mut url = http::uri::Uri::from_str(&info.announce)?;

    let tracker_request = TrackerRequest {
        info_hash,
        peer_id: vec![0xDA; 20],
        ip: None,
        port: 6881,
        uploaded: 0,
        downloaded: 0,
        left: info.info.length.unwrap() as u64,
        event: "empty".into(),
    };

    url.query = Some(tracker_request.to_query_string());

    let mut client = http::Client::create(url)?;

    let mut http_request = http::RequestBuilder::new()
        .method(http::Method::GET)
        .uri(url)
        .build()?;

    let mut http_response = client.request(http_request).await?;

    let mut http_response_data = vec![];
    http_response
        .body
        .read_to_end(&mut http_response_data)
        .await?;

    println!("{:?}", http_response.head);

    let (v, _) = parsing::complete(BENValue::parse)(&http_response_data)?;

    println!("{:?}", v);

    // Compact: [4-byte ipv4] [2 byte port] (both in network order)

    // println!("{:?}", info);

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
