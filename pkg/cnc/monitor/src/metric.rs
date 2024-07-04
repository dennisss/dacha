use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::hash::SumHasherBuilder;
use crypto::hasher::Hasher;
use crypto::sip::SipHasher;
use executor::lock;
use executor::sync::AsyncMutex;
use executor_multitask::{impl_resource_passthrough, TaskResource};
use protobuf::Message;

use crate::db::{ProtobufDB, Query, QueryAllOf, QueryOperation, QueryValue};
use crate::tables::MetricSampleTable;

/*
TODO: Need to switch this file and all metric collectors to monotonic timestamps.

TODO: Limit max num enqueued samples
- When there are too many, randomly downsample the data.
- Limit the max resolution of samples (if sample 'n' is <1 second after sample 'n-2', delete sample 'n')

Defining a waterline:
- Assume that everything is logged to the database within 1 second
    - So we must only query samples with 't < now - 1 second'
    - We will want to have logging to verify that we never insert data below the waterline.
- If alignment is required, we must

*/

/// Maximum number of samples we will insert into the database in one
/// batch.
const WRITE_BATCH_SIZE: usize = 128;

///
const WRITE_BATCH_TIMEOUT: Duration = Duration::from_millis(100);

const MAX_UNCOLLECTED_PER_METRIC: usize = 128;

/// The maximum amount of time that it takes from a sample to be stored in our
/// metrics database (measured from the sample collection timestamp).
///
/// At any point in time 't', we assume that we have all data up to
/// 't - MAX_DATA_STALENESS'.
const MAX_DATA_STALENESS: Duration = Duration::from_millis(1000);

pub struct MetricStore {
    shared: Arc<Shared>,
    task: TaskResource,
}

impl_resource_passthrough!(MetricStore, task);

struct Shared {
    db: Arc<ProtobufDB>,
    state: AsyncMutex<State>,
}

#[derive(Default)]
struct State {
    data: HashMap<u64, StreamData, SumHasherBuilder>,
}

#[derive(Default)]
struct StreamData {
    samples: VecDeque<(u64, f32)>,
}

impl MetricStore {
    pub fn new(db: Arc<ProtobufDB>) -> Self {
        let shared = Arc::new(Shared {
            db,
            state: AsyncMutex::default(),
        });

        let task =
            TaskResource::spawn_interruptable("MetricStore", Self::collection_task(shared.clone()));

        Self { shared, task }
    }

    /// NOTE: This can be called any amount of times with the same
    /// MetricResource.
    ///
    /// TODO: Disallow calling multiple times if we have counter style metrics.
    pub async fn stream(&self, resource: &MetricResource) -> Result<MetricStream> {
        let resource_key = {
            // NOTE: We assume that this is deterministic and stable over time.
            let data = resource.serialize()?;

            let mut hasher = SipHasher::default_rounds_with_key_halves(0, 0);
            hasher.update(&data);
            hasher.finish_u64()
        };

        lock!(state <= self.shared.state.lock().await?, {
            state.data.entry(resource_key).or_default();
        });

        Ok(MetricStream {
            shared: self.shared.clone(),
            resource_key,
        })
    }

    // TODO: Accept a cancellation token and ensure that all metrics (at least
    // before cancellation was started) are collected.
    async fn collection_task(shared: Arc<Shared>) -> Result<()> {
        loop {
            // TODO: Do more batching. Only write if we hit the MAX_SAMPLES_PER_WRITE or 1
            // second has elapsed.

            let mut txn = shared.db.new_transaction();

            let mut i = 0;
            lock!(state <= shared.state.lock().await?, {
                for (resource_key, entry) in &mut state.data {
                    while i < WRITE_BATCH_SIZE {
                        match entry.samples.pop_front() {
                            Some((time, value)) => {
                                let mut sample = MetricSample::default();
                                sample.set_resource_key(*resource_key);
                                sample.set_timestamp(time);
                                sample.set_float_value(value);
                                txn.insert::<MetricSampleTable>(&sample)?;

                                i += 1;
                            }
                            None => {
                                break;
                            }
                        }
                    }

                    if i >= WRITE_BATCH_SIZE {
                        break;
                    }
                }

                Ok::<_, Error>(())
            })?;

            if i > 0 {
                txn.commit().await?;
            }

            executor::sleep(Duration::from_millis(100)).await?;
        }
    }
}

pub struct MetricStream {
    shared: Arc<Shared>,
    resource_key: u64,
}

pub struct MetricQueryResponse {
    /// Actual end time of data queried. May be smaller than the end_time given
    /// to MetricStream::query if end_time was beyond the collection waterline.
    pub adjusted_end_time: SystemTime,

    pub samples: Vec<MetricSample>,
}

impl MetricStream {
    /// NOTE: This is expected to always be a fast method.
    ///
    /// TODO: Need to switch to monotonic timestamps.
    pub async fn record(&self, time: SystemTime, value: f32) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            let entry = state
                .data
                .get_mut(&self.resource_key)
                .ok_or_else(|| err_msg("Missing metric data entry"))?;

            entry.samples.push_back((Self::time_to_u64(time), value));

            Ok(())
        })
    }

    /*
    Some rendering quirks:
    - I want the end of the rendering window to ideally always be at the last point so that it doesn't show a random gap in the

    */

    /// Queries data in this stream between the time range [start_time,
    /// end_time).
    ///
    /// Note that the given time range is the range on the raw data (before
    /// aligning timestamps).
    pub async fn query(
        &self,
        mut start_time: SystemTime,
        mut end_time: SystemTime,
        alignment: Option<Duration>,
    ) -> Result<Vec<MetricSample>> {
        let mut waterline = SystemTime::now() - MAX_DATA_STALENESS;

        // if let Some(alignment) = alignment {
        //     waterline -= alignment / 2;
        // }

        if end_time > waterline {
            end_time = waterline;
        }

        // if let Some( )

        if start_time > end_time {
            start_time = end_time;
        }

        let mut query = Query::default();

        let start_time = Self::time_to_u64(start_time);
        let end_time = Self::time_to_u64(end_time);

        let mut query_a = QueryAllOf::default();
        query_a
            .and(
                MetricSample::RESOURCE_KEY_FIELD_NUM.raw(),
                QueryOperation::Eq(QueryValue::U64(self.resource_key)),
            )
            .and(
                MetricSample::TIMESTAMP_FIELD_NUM.raw(),
                QueryOperation::GreaterThanOrEqual(QueryValue::U64(start_time)),
            )
            .and(
                MetricSample::TIMESTAMP_FIELD_NUM.raw(),
                QueryOperation::LessThan(QueryValue::U64(end_time)),
            );
        query.or(query_a);

        // NOTE: These are in descending timestamp order.
        let mut samples = self.shared.db.query::<MetricSampleTable>(&query).await?;

        // Do alignment
        if let Some(alignment) = alignment {
            let alignment = alignment.as_micros() as u64;
            let max_distance = (alignment / 2) + 1;
            let mut aligned_samples = vec![];

            let mut current_time = end_time - (end_time % alignment);
            let mut current_best: Option<(MetricSample, u64)> = None;

            // TODO: for very large time alignment windows, it may make sense to seek around
            // to skip unneeded data (way more efficient if we just accept any sample with
            // distance < max_distance rather than the nearest).

            // NOTE: This algorithm is only correct when one point can't be assigned to two
            // different aligned time points.
            for sample in samples {
                while current_time >= start_time {
                    let distance = (sample.timestamp() as i64) - (current_time as i64);
                    let distance_abs = distance.abs() as u64;

                    if distance_abs < max_distance {
                        if let Some((_, current_best_distnace)) = &current_best {
                            if distance_abs < *current_best_distnace {
                                current_best = Some((sample, distance_abs));
                            }
                        } else {
                            current_best = Some((sample, distance_abs));
                        }

                        break;
                    }

                    if distance > 0 {
                        // Current point is later than the current time. Skip to the next sample
                        // which should be at an earlier time.
                        break;
                    } else {
                        // Current point is earlier than the current time. Decrement current_time
                        // and retry.

                        if let Some((s, _)) = current_best.take() {
                            // TODO: Adjust the timestamp to be aligned.

                            aligned_samples.push(s);
                        }

                        current_time -= alignment;
                    }
                }
            }

            if let Some((s, _)) = current_best {
                // TODO: Adjust the timestamp to be aligned.
                aligned_samples.push(s);
            }

            samples = aligned_samples;
        }

        Ok(samples)
    }

    fn time_to_u64(time: SystemTime) -> u64 {
        time.duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }
}
