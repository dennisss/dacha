extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate crypto;
extern crate datastore;
extern crate http;
extern crate parsing;
extern crate protobuf;
#[macro_use]
extern crate macros;
extern crate rpc;
extern crate web;
#[macro_use]
extern crate file;

pub mod proto;
pub mod viewer;

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use common::errors::*;
use crypto::random::RngExt;
use datastore_meta_client::key_encoding::KeyEncoder;
use executor::bundle::TaskResultBundle;
use parsing::{ascii::AsciiString, parse_next};
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

    pub async fn iter(&self, metric_name: &str) -> Result<MetricValueIterator> {
        let snapshot = self.db.snapshot().await;
        let iter = snapshot.iter().await?;
        Ok(MetricValueIterator {
            iter,
            metric_name: metric_name.to_string(),
        })
    }

    pub async fn query(
        &self,
        metric_name: &str,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> Result<Vec<MetricValue>> {
        let mut iter = self.iter(metric_name).await?;
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
        let mut iter = self.iter(metric_name).await?;
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
        loop {
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

            let value = match &entry.value {
                Some(v) => v,
                None => continue,
            };

            if value.len() != 4 {
                return Err(err_msg("Value wrong length for f32"));
            }

            let float_value = f32::from_le_bytes(*array_ref![&value, 0, 4]);

            return Ok(Some(MetricValue {
                metric_name: current_metric_name,
                timestamp,
                float_value,
            }));
        }
    }
}

#[derive(Clone)]
struct MetricServiceImpl {
    metric_store: Arc<MetricStore>,
}

#[async_trait]
impl MetricService for MetricServiceImpl {
    async fn Query(
        &self,
        request: rpc::ServerRequest<QueryRequest>,
        response: &mut rpc::ServerResponse<QueryResponse>,
    ) -> Result<()> {
        // TODO: Validate that the start/end timestamps are non-zero and look sane (not
        // too far apart)

        let mut values = self
            .metric_store
            .query(
                request.metric_name(),
                request.start_timestamp(),
                request.end_timestamp(),
            )
            .await?;

        // Change from descending time order to ascending time order.
        values.reverse();

        let mut line = QueryResponse_Line::default();
        line.set_name("Main");

        for value in values {
            let mut point = QueryResponse_Point::default();
            point.set_timestamp(value.timestamp);
            point.set_value(value.float_value);
            line.add_points(point);
        }

        response.add_lines(line);

        Ok(())
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

        executor::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn run() -> Result<()> {
    let store = Arc::new(MetricStore::open("/tmp/metricstore").await?);

    executor::spawn(collect_random(store.clone()));

    let mut task_bundle = TaskResultBundle::new();

    task_bundle.add("WebServer", {
        let web_handler = web::WebServerHandler::new(web::WebServerOptions {
            pages: vec![web::WebPageOptions {
                title: "Sensor Monitor".into(),
                path: "/".into(),
                script_path: "built/pkg/app/sensor_monitor/web.js".into(),
                vars: None,
            }],
        });

        let web_server = http::Server::new(web_handler, http::ServerOptions::default());

        web_server.run(8000)
    });

    task_bundle.add("RpcServer", {
        let mut rpc_server = rpc::Http2Server::new();
        rpc_server.add_service(
            MetricServiceImpl {
                metric_store: store.clone(),
            }
            .into_service(),
        )?;
        rpc_server.enable_cors();
        rpc_server.allow_http1();
        rpc_server.run(8001)
    });

    task_bundle.join().await
}
