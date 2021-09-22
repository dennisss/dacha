#[macro_use]
extern crate common;
extern crate crypto;
extern crate datastore;
extern crate http;
extern crate parsing;
extern crate protobuf;
extern crate protobuf_json;
#[macro_use]
extern crate macros;

mod proto;

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use common::async_std::task;
use common::errors::*;
use crypto::random::RngExt;
use http::static_file_handler::StaticFileHandler;
use parsing::{ascii::AsciiString, parse_next};
use protobuf_json::{MessageJsonParser, MessageJsonSerialize};

use datastore::key_encoding::KeyEncoder;
use sstable::{db::SnapshotIterator, iterable::Iterable, EmbeddedDB, EmbeddedDBOptions};

use crate::proto::data::*;

struct MetricValue {
    metric_name: String,
    /// Measurement time in micros since epoch.
    timestamp: u64,

    float_value: f32,
}

const METRIC_VALUES_TABLE_ID: u64 = 10;

struct MetricStore {
    db: EmbeddedDB,
}

impl MetricStore {
    pub async fn open(path: &str) -> Result<Self> {
        let mut options = EmbeddedDBOptions::default();
        options.create_if_missing = true;

        Ok(Self {
            db: EmbeddedDB::open(path, options).await?,
        })
    }

    fn encode_key(metric_name: &str, timestamp: u64) -> Vec<u8> {
        let mut key = vec![];
        KeyEncoder::encode_varuint(METRIC_VALUES_TABLE_ID, false, &mut key);
        KeyEncoder::encode_bytes(metric_name.as_bytes(), &mut key);
        KeyEncoder::encode_varuint(timestamp, true, &mut key);
        key
    }

    fn decode_key(mut key: &[u8]) -> Result<Option<(String, u64)>> {
        let table_id = parse_next!(key, |v| KeyEncoder::decode_varuint(v, false));
        if table_id != METRIC_VALUES_TABLE_ID {
            return Ok(None);
        }

        let metric_name_raw = parse_next!(key, KeyEncoder::decode_bytes);
        let metric_name = String::from_utf8(metric_name_raw)?;

        let timestamp = parse_next!(key, |v| KeyEncoder::decode_varuint(v, true));

        Ok(Some((metric_name, timestamp)))
    }

    pub async fn record(&self, metric_value: &MetricValue) -> Result<()> {
        let key = Self::encode_key(&metric_value.metric_name, metric_value.timestamp);
        let value = metric_value.float_value.to_le_bytes();

        self.db.set(&key, &value).await
    }

    pub async fn iter(&self, metric_name: &str) -> MetricValueIterator {
        let snapshot = self.db.snapshot().await;
        let iter = snapshot.iter().await;
        MetricValueIterator {
            iter,
            metric_name: metric_name.to_string(),
        }
    }

    pub async fn query(
        &self,
        metric_name: &str,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> Result<Vec<MetricValue>> {
        let mut iter = self.iter(metric_name).await;
        iter.seek(end_timestamp).await?;

        let mut out = vec![];
        while let Some(metric_value) = iter.next().await? {
            if metric_value.timestamp < start_timestamp {
                break;
            }

            out.push(metric_value);
        }

        Ok(out)
    }

    pub async fn last_value(&self, metric_name: &str) -> Result<Option<MetricValue>> {
        let mut iter = self.iter(metric_name).await;
        iter.seek(std::u64::MAX).await?;
        iter.next().await
    }
}

struct MetricValueIterator {
    iter: SnapshotIterator,
    metric_name: String,
}

impl MetricValueIterator {
    pub async fn seek(&mut self, end_timestamp: u64) -> Result<()> {
        self.iter
            .seek(&MetricStore::encode_key(&self.metric_name, end_timestamp))
            .await
    }

    pub async fn next(&mut self) -> Result<Option<MetricValue>> {
        let entry = match self.iter.next().await? {
            Some(v) => v,
            None => {
                return Ok(None);
            }
        };

        let (current_metric_name, timestamp) = match MetricStore::decode_key(&entry.key)? {
            Some(v) => v,
            None => {
                return Ok(None);
            }
        };

        if current_metric_name != self.metric_name {
            return Ok(None);
        }

        if entry.value.len() != 4 {
            return Err(err_msg("Value wrong length for f32"));
        }

        let value = f32::from_le_bytes(*array_ref![&entry.value, 0, 4]);

        Ok(Some(MetricValue {
            metric_name: current_metric_name,
            timestamp,
            float_value: value,
        }))
    }
}

pub fn json_response<M>(code: http::status_code::StatusCode, obj: &M) -> http::Response
where
    M: protobuf::MessageReflection,
{
    let body = obj.serialize_json();

    // TODO: Perform response compression.

    http::ResponseBuilder::new()
        .status(code)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(http::BodyFromData(body))
        .build()
        .unwrap()
}

struct RequestHandler {
    metric_store: Arc<MetricStore>,
    static_handler: StaticFileHandler,
    build_handler: StaticFileHandler,
    lib_handler: StaticFileHandler,
}

impl RequestHandler {
    pub fn new(metric_store: Arc<MetricStore>) -> Self {
        Self {
            metric_store,
            // TODO: Maybe implement caching of some of these?
            static_handler: StaticFileHandler::new(&project_path!("pkg/sensor_monitor/static")),
            build_handler: StaticFileHandler::new(&project_path!("build/sensor_monitor")),
            lib_handler: StaticFileHandler::new(&project_path!("node_modules")),
        }
    }

    async fn handle_api_request(&self, mut request: http::Request) -> Result<http::Response> {
        if request.head.method != http::Method::POST {
            return Err(err_msg("Wrong method"));
        }

        if request.head.uri.path.as_str() == "/query" {
            let mut req_data = vec![];
            request.body.read_to_end(&mut req_data).await?;

            let query = QueryRequest::parse_json(
                std::str::from_utf8(&req_data)?,
                &protobuf_json::ParserOptions::default(),
            )?;

            // TODO: Validate that the start/end timestamps are non-zero and look sane (not
            // too far apart)

            let mut values = self
                .metric_store
                .query(
                    query.metric_name(),
                    query.start_timestamp(),
                    query.end_timestamp(),
                )
                .await?;

            // Change from descending time order to ascending time order.
            values.reverse();

            let mut response = QueryResponse::default();

            let mut line = QueryResponse_Line::default();
            line.set_name("Main");

            for value in values {
                let mut point = QueryResponse_Point::default();
                point.set_timestamp(value.timestamp);
                point.set_value(value.float_value);
                line.add_points(point);
            }

            response.add_lines(line);

            return Ok(json_response(http::status_code::OK, &response));
        }

        Err(err_msg("Unknown path"))
    }
}

#[async_trait]
impl http::server::RequestHandler for RequestHandler {
    async fn handle_request(&self, mut request: http::Request) -> http::Response {
        let mut path = request.head.uri.path.as_str();
        if path == "/" {
            let contents = common::async_std::fs::read_to_string(&project_path!(
                "pkg/sensor_monitor/web/index.html"
            ))
            .await
            .unwrap();

            return http::ResponseBuilder::new()
                .status(http::status_code::OK)
                .header(http::header::CONTENT_TYPE, "text/html")
                .body(http::BodyFromData(contents))
                .build()
                .unwrap();
        }

        // TODO: Check that each of these prefixes is followed with a '/'
        if let Some(path) = path.strip_prefix("/assets/static") {
            request.head.uri.path = AsciiString::from(path).unwrap();
            return self.static_handler.handle_request(request).await;
        }

        if let Some(path) = path.strip_prefix("/assets/build") {
            request.head.uri.path = AsciiString::from(path).unwrap();
            return self.build_handler.handle_request(request).await;
        }

        if let Some(path) = path.strip_prefix("/assets/lib") {
            request.head.uri.path = AsciiString::from(path).unwrap();
            return self.lib_handler.handle_request(request).await;
        }

        if let Some(path) = path.strip_prefix("/api") {
            request.head.uri.path = AsciiString::from(path).unwrap();

            let mut res = self.handle_api_request(request).await;
            return match res {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error handling request: {:?}", e);
                    http::ResponseBuilder::new()
                        .status(http::status_code::INTERNAL_SERVER_ERROR)
                        .build()
                        .unwrap()
                }
            };
        }

        http::ResponseBuilder::new()
            .status(http::status_code::NOT_FOUND)
            .build()
            .unwrap()
    }
}

async fn collect_random(metric_store: Arc<MetricStore>) {
    if let Err(e) = collect_random_metric_inner(metric_store).await {
        eprintln!("While collecting random metric: {:?}", e);
    }
}

async fn collect_random_metric_inner(metric_store: Arc<MetricStore>) -> Result<()> {
    let mut rng = crypto::random::clocked_rng();

    let mid_value = 5.0;

    let mut y = mid_value;
    if let Some(last_value) = metric_store.last_value("random").await? {
        y = last_value.float_value;

        println!("Last value: {} @ {}", y, last_value.timestamp);
    }

    loop {
        y += rng.between(-1.0f32, 1.0f32);
        y = (0.9 * y) + (0.1 * mid_value);

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros() as u64;
        metric_store
            .record(&MetricValue {
                metric_name: "random".into(),
                timestamp,
                float_value: y,
            })
            .await?;

        println!("Recorded {} @ {}", y, timestamp);

        task::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn run() -> Result<()> {
    let store = Arc::new(MetricStore::open("/tmp/metricstore").await?);

    task::spawn(collect_random(store.clone()));

    let handler = Arc::new(RequestHandler::new(store));
    let server = http::Server::new(handler, http::ServerOptions::default());
    server.run(8000).await
}
