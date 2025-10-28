use std::collections::HashMap;

use common::{bytes::Bytes, errors::*};
use reflection::ParseFrom;

pub struct HianimeClient {
    http_client: http::SimpleClient,
}


#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct EpisodesResponseObject {
    pub status: bool,
    pub html: String,
    pub totalItems: usize,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct ServersResponseObject {
    pub status: bool,
    pub html: String,
}

#[derive(Clone, Debug)]
pub struct Episode {
    pub title: String,
    pub number: usize,
    pub id: usize,
    // TODO: Also japanese name.
}

#[derive(Clone, Debug)]
pub struct Server {
    pub id: usize,
    pub typ: String,
    pub name: String,

}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct Sources {
    pub link: String,
}


impl HianimeClient {
    pub async fn create() -> Result<Self> {
        let client = http::SimpleClient::new(http::SimpleClientOptions::default());
        Ok(Self {
            http_client: client
        })
    }

    pub async fn get_series_episodes(&self, series_id: usize) -> Result<Vec<Episode>> {
        let obj = self.get_json(&format!("https://hianime.to/ajax/v2/episode/list/{}", series_id)).await?;
        let obj = EpisodesResponseObject::parse_from(json::ValueParser::new(&obj))?;
        if !obj.status {
            return Err(err_msg("Failed to get list of episodes"));
        }

        let doc = xml::parse(&obj.html)?;

        let mut list = vec![];

        xml::traverse_all_elements(&doc.root_element, &mut |el| {
            if el.name != "a" {
                return Ok(());
            }

            let title = el.attributes.get("title").ok_or_else(|| err_msg("Missing title"))?.into();
            let number = el.attributes.get("data-number").ok_or_else(|| err_msg("Missing number"))?.parse()?;
            let id = el.attributes.get("data-id").ok_or_else(|| err_msg("Missing id"))?.parse()?;

            list.push(Episode {
                title,
                number,
                id
            });            

            Ok(())
        })?;

        // TODO: Check count against obj.totalItems

        Ok(list)        
    }

    pub async fn get_episode_servers(&self, episode_id: usize) -> Result<Vec<Server>> {
        let obj = self.get_json(&format!("https://hianime.to/ajax/v2/episode/servers?episodeId={}", episode_id)).await?;
        let obj = ServersResponseObject::parse_from(json::ValueParser::new(&obj))?;
        if !obj.status {
            return Err(err_msg("Failed to get list of episodes"));
        }

        let doc = xml::parse(&format!("<div>{}</div>", obj.html))?;

        let mut list = vec![];

        xml::traverse_all_elements(&doc.root_element, &mut |el| {
            if el.attributes.get("class").map(|s| s.as_str()).unwrap_or("") != "item server-item" {
                return Ok(());
            }

            let id = el.attributes.get("data-id").ok_or_else(|| err_msg("Missing id"))?.parse()?;
            let typ = el.attributes.get("data-type").ok_or_else(|| err_msg("Missing type"))?.into();
            let name = el.inner_text().trim().to_string();

            list.push(Server {
                name,
                typ,
                id
            });

            Ok(())
        })?;

        Ok(list)
    }

    pub async fn get_episode_sources(&self, server_id: usize) -> Result<Sources> {
        let obj = self.get_json(&format!("https://hianime.to/ajax/v2/episode/sources?id={}", server_id)).await?;
        Sources::parse_from(json::ValueParser::new(&obj))
    }

    async fn get_json(&self, url: &str) -> Result<json::Value> {
        let request_header =
            http::RequestBuilder::new()
                .method(http::Method::GET)
                .uri(url)
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

        json::parse(std::str::from_utf8(&res.body)?)
    }
}
