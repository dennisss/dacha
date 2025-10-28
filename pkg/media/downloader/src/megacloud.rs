use std::collections::HashMap;

use common::{bytes::Bytes, errors::*};
use reflection::ParseFrom;

pub struct MegacloudClient {
    http_client: http::SimpleClient,
}

/*
Example PlayerData object:

{
    "sources":[
        {"file":"https://dm.netmagcdn.com:2228/hls-playback/.../master.m3u8","type":"hls"}
    ],
    "tracks":[{"file":"https://mgstatics.xyz/subtitle/16fb0a17dbb92e6d662bb5aad68cd34f/16fb0a17dbb92e6d662bb5aad68cd34f.vtt","label":"English","kind":"captions","default":true},{"file":"https://mgstatics.xyz/thumbnails/102c05f2064e12e6cfc1db200fba9a61/thumbnails.vtt","kind":"thumbnails"}],
    "encrypted":false,
    "intro":{"start":55,"end":144},
    "outro":{"start":1362,"end":1458},"server":1
}
*/
#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct PlayerData {
    pub sources: Vec<PlayerSourceData>,
    pub tracks: Vec<PlayerTrackData>,
    pub encrypted: bool,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct PlayerSourceData {
    pub file: String,
    #[parse(name = "type")]
    pub typ: String,
}

#[derive(Parseable, Debug, Default)]
#[parse(allow_unknown = true)]
pub struct PlayerTrackData {
    pub file: String,
    pub kind: String,

    // NOTE: Not always present.
    pub label: Option<String>,
}

// 48 characters long
regexp!(CLIENT_KEY_TYPE1 => " _is_th:([^ ]+) ");

// 16 characters each
regexp!(CLIENT_KEY_TYPE2 => "window._lk_db = {x: \"([^\"]+)\", y: \"([^\"]+)\", z: \"([^\"]+)\"};");

//     <script nonce="H9CN32o5wQ642ZktDA7r9Crnr7aOk3I2Y36TrHDrtRd75Yax">/* empty nonce script */</script>
regexp!(CLIENT_KEY_TYPE3 => "<script nonce=\"([^\"]+)\">");

//     <div data-dpi="KPmd5ekNtjX4M0MKshbiefd0mwLiGE9Szx1XGJX6VdH58JOi" style="display:none"></div>
regexp!(CLIENT_KEY_TYPE4 => "<div data-dpi=\"([^\"]+)\"");

// <meta name="_gg_fb" content="vxe3pAWjAURB9EDjDNUebiyAC4HAfuOXjHeEB0bsp8xeGSU6">
regexp!(CLIENT_KEY_TYPE5 => "<meta name=\"_[^\"]+\" content=\"([^\"]+)\"");

//     <script>window._xy_ws = "ubaIBo61D3D7BsrHXVJZCw1Oge5n54DtpE4saGrvRIhOyClA";</script>
regexp!(CLIENT_KEY_TYPE6 => "window._xy_ws = \"([^\"]+)\";");


regexp!(FILE_ID => "id=\"megacloud-player\" data-id=\"([^\"]+)\"");


impl MegacloudClient {
    pub async fn create() -> Result<Self> {
        let client = http::SimpleClient::new(http::SimpleClientOptions::default());
        Ok(Self {
            http_client: client
        })
    }
    
    pub async fn get_player_data(&self, url: &str) -> Result<Option<PlayerData>> {

        let page = self.get(url).await?;

        if page.contains("File not found") {
            return Ok(None);
        }

        let id = {

            let mut id = None;
            if let Some(m) = FILE_ID.exec(&page) {
                id = Some(m.group_str(1).unwrap().unwrap().to_string());
            }
            /*
            let doc = xml::parse(&page)?;


            xml::traverse_all_elements(&doc.root_element, &mut |el| {
                if el.attributes.get("id").map(|s| s.as_str()).unwrap_or("") != "megacloud-player" {
                    return Ok(());
                }

                if id.is_some() {
                    return Err(err_msg("Multiple player els on page"));
                }

                id = Some(el.attributes.get("data-id").ok_or_else(|| err_msg("Missing id attr"))?.to_string());
                Ok(())
            })?;

            */

            if id.is_none() {
                println!("BAD PAGE: {}", page);

            }

            id.ok_or_else(|| err_msg("Failed to find file id on page"))?
        };


        let client_key = {
            let mut client_key = None;

            if let Some(m) = CLIENT_KEY_TYPE1.exec(&page) {
                client_key = Some(m.group_str(1).unwrap().unwrap().to_string());
            }
            if let Some(m) = CLIENT_KEY_TYPE2.exec(&page) {
                client_key = Some(format!("{}{}{}",
                    m.group_str(1).unwrap().unwrap(),
                    m.group_str(2).unwrap().unwrap(),
                    m.group_str(3).unwrap().unwrap(),
                ));
            }
            if let Some(m) = CLIENT_KEY_TYPE3.exec(&page) {
                client_key = Some(m.group_str(1).unwrap().unwrap().to_string());
            }
            if let Some(m) = CLIENT_KEY_TYPE4.exec(&page) {
                client_key = Some(m.group_str(1).unwrap().unwrap().to_string());
            }

            if let Some(m) = CLIENT_KEY_TYPE5.exec(&page) {
                client_key = Some(m.group_str(1).unwrap().unwrap().to_string());
            }

            if let Some(m) = CLIENT_KEY_TYPE6.exec(&page) {
                client_key = Some(m.group_str(1).unwrap().unwrap().to_string());
            }


            if client_key.is_none() {
                println!("BAD PAGE: {}", page);

            }

            client_key.ok_or_else(|| err_msg("Failed to extract client key from player page"))?
        };


        let sources = self.get(&format!("https://megacloud.blog/embed-2/v3/e-1/getSources?id={}&_k={}", id, client_key)).await?;

        let obj = json::parse(&sources)?;

        let obj = PlayerData::parse_from(json::ValueParser::new(&obj))?;

        if obj.encrypted {
            return Err(err_msg("Encrypted data not supported"));
        }

        Ok(Some(obj))
    }

    async fn get(&self, url: &str) -> Result<String> {
        let request_header =
            http::RequestBuilder::new()
                .method(http::Method::GET)
                .uri(url)
                .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36")
                .header("Origin", "https://megacloud.blog")
                .header("Referer", "https://megacloud.blog/")
                .build()?
                .head;

        let request_body = common::bytes::Bytes::from("");

        let res = self
            .http_client
            .request(
                &request_header,
                request_body,
                &http::ClientRequestContext::default(),
            )
            .await?;

        if res.head.status_code != http::status_code::OK {
            return Err(format_err!(
                "Request failure: {:?}: {:?}",
                res.head.status_code,
                res.body
            ));
        }

        Ok(std::str::from_utf8(&res.body)?.to_string())
    }
}