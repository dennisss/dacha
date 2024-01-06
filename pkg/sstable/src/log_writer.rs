use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use common::io::Writeable;
use executor::SyncRange;
use executor::{child_task::ChildTask, sync::Mutex, Condvar};
use file::{LocalFile, LocalPath};

#[derive(Defaultable, Clone, Debug)]
pub struct LogWriterOptions {
    /// Preferred alignment for writes. Should be set to the page/block size of
    /// the file system. We will try to avoid triggering flushing not
    /// aligned to the multiple of bytes from the start of the file.
    #[default(4096)]
    pub write_alignment: u64,

    /// Prefered number of bytes to enqueue before we trigger a flush.
    ///
    /// Should be high enough that we overcome per-transaction overheads with
    /// writing to disk.
    #[default(10 * 1024 * 1024)]
    pub flush_bytes_threshold: u64,

    /// Timeout after which bytes will be flushed regardless of whether the
    /// bytes threshold is hit.
    ///
    /// This is meant to amortize the cost of flushing a sub-optimal number of
    /// bytes. Should be tuned based on disk speed and flush_bytes_threshold.
    ///
    /// e.g. for a 1000 MiB/s write speed SSD, the default flush_bytes_threshold
    /// of 10 MiB will be written in 10ms so we set that as the timeout here.
    #[default(Duration::from_millis(10))]
    pub flush_timeout_threshold: Duration,
}

/// Writer for appending data to a WAL file while continuously flushing old
/// writes to disk.
///
/// TODOs:
/// - For local file I/O, implementing an in-process page cache and using
///   O_DIRECT would probably be more efficient (but need to ensure the reader
///   files are also opened with O_DIRECT to avoid read/write inconsistencies).
/// - In the future when we use more networked file systems, ideally we'd have
///   those natively give back async flush notifications rather than needing us
///   to continously poll for them.
///
/// NOTE: We don't do any buffering of data in this class as we assume the inner
/// writer has some buffering (in the case of a linux file, the kernel page
/// cache)
pub struct LogWriter {
    shared: Arc<Shared>,

    /// Background thread for running fsync.
    thread: ChildTask,
}

struct Shared {
    options: LogWriterOptions,
    file: LocalFile,
    state: Condvar<State>,
}

struct State {
    /// Next position at which we will write.
    offset: u64,

    /// Highest offset up to which we know that the file has been fully flushed.
    flushed_offset: u64,

    /// Highest threshold at which the user has requested everything be flushed.
    /// We won't wait for timeouts up to this point.
    force_offset: u64,

    /// Time at which the first unflushed byte was written.
    first_pending_byte_time: Option<Instant>,

    /// Time at which the first byte was written in the last block that was
    /// written.
    last_block_start_time: Option<Instant>,

    flush_error: LatchingError,
}

impl LogWriter {
    /// NOTE: Writes will start at the current position in the file.
    pub async fn create(options: LogWriterOptions, file: LocalFile) -> Result<Self> {
        let offset = file.current_position();
        if offset != file.metadata().await?.len() {
            return Err(err_msg("Log file not seeked to end"));
        }

        let shared = Arc::new(Shared {
            options,
            file,
            state: Condvar::new(State {
                offset,
                flushed_offset: 0,
                force_offset: 0,
                first_pending_byte_time: None,
                last_block_start_time: None,
                flush_error: LatchingError::default(),
            }),
        });

        let thread = ChildTask::spawn(Self::background_thread_fn(shared.clone()));

        Ok(Self { shared, thread })
    }

    pub fn path(&self) -> &LocalPath {
        self.shared.file.path()
    }

    pub fn new_flush_subscriber(&self) -> LogFlushSubscriber {
        LogFlushSubscriber::create(self.shared.clone())
    }

    async fn background_thread_fn(shared: Arc<Shared>) {
        loop {
            let mut state = shared.state.lock().await;

            let now = Instant::now();
            let flushed_offset = state.flushed_offset;

            let (next_flush_offset, next_flush_time) =
                Self::check_ready_to_flush(&shared, &state, now);

            let next_flush_offset = match next_flush_offset {
                Some(v) => v,
                None => {
                    let waiter = state.wait(());

                    if let Some(time) = next_flush_time {
                        assert!(time > now);
                        let _ = executor::timeout(time - now, waiter).await;
                    } else {
                        waiter.await;
                    }

                    continue;
                }
            };

            if next_flush_offset == state.offset {
                state.last_block_start_time = None;
            }

            drop(state);

            assert!(next_flush_offset > flushed_offset);

            // TODO: Allow up to two flushes to be running at once.
            let res = shared
                .file
                .sync(
                    true,
                    Some(SyncRange {
                        start: flushed_offset,
                        end: next_flush_offset,
                    }),
                )
                .await;

            let mut state = shared.state.lock().await;
            let is_error = res.is_err();
            match res {
                Ok(()) => {
                    state.flushed_offset = next_flush_offset;
                    state.first_pending_byte_time = state.last_block_start_time.clone();
                }
                Err(e) => {
                    state.flush_error.set(e);
                }
            }

            state.notify_all();

            if is_error {
                break;
            }
        }
    }

    fn check_ready_to_flush(
        shared: &Shared,
        state: &State,
        now: Instant,
    ) -> (Option<u64>, Option<Instant>) {
        let block_size = shared.options.write_alignment;

        let mut start = (state.flushed_offset / block_size) * block_size;
        let mut end = (state.offset / block_size) * block_size;

        let mut need_flush = false;
        let mut next_time = None;

        // Check if we should flush the complete blocks of data.
        if end > state.flushed_offset {
            need_flush |= end - start > shared.options.flush_bytes_threshold;

            if let Some(time) = &state.first_pending_byte_time {
                let t = *time + shared.options.flush_timeout_threshold;
                next_time = Some(t);

                if t <= now {
                    need_flush = true;
                }
            }
        }

        // Check if we should flush the final partially complete block.
        if let Some(time) = &state.last_block_start_time {
            let t = *time + shared.options.flush_timeout_threshold;
            next_time = core::cmp::max(next_time, Some(t));

            if t <= now {
                end = state.offset;
                need_flush = true;
            }
        }

        need_flush |= state.force_offset > state.flushed_offset;
        if end < state.force_offset {
            end = state.offset;
        }

        (if need_flush { Some(end) } else { None }, next_time)
    }
}

// TODO: Document this subscriber pattern and standardize on using something
// like this more often.
#[derive(Clone)]
pub struct LogFlushSubscriber {
    last_flushed_offset: u64,

    // TODO: Allow the LogWriter to be dropped if we have a reference here.
    shared: Arc<Shared>,
}

impl LogFlushSubscriber {
    fn create(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            last_flushed_offset: 0,
        }
    }

    /// NOTE: This won't return any data as we don't want data to be lost if the
    /// blocking future is pre-empted.
    pub async fn wait_for_flush(&mut self) {
        loop {
            let mut state = self.shared.state.lock().await;
            if state.flushed_offset > self.last_flushed_offset {
                self.last_flushed_offset = state.flushed_offset;
                break;
            }

            if state.flush_error.is_err() {
                break;
            }

            state.wait(()).await;
        }
    }

    pub async fn last_flushed_offset(&mut self) -> Result<u64> {
        let mut state = self.shared.state.lock().await;
        self.last_flushed_offset = state.flushed_offset;
        state.flush_error.get()?;
        Ok(self.last_flushed_offset)
    }
}

#[async_trait]
impl Writeable for LogWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let mut state = self.shared.state.lock().await;

        // TODO: Stop accepting writes if syncing is in a failing state.

        let n = self.shared.file.write_at(state.offset, buf).await?;

        if n != 0 {
            if state.offset == state.flushed_offset {
                state.first_pending_byte_time = Some(Instant::now());
            }

            let old_block_index = state.offset / self.shared.options.write_alignment;
            state.offset += n as u64;
            let new_block_index = state.offset / self.shared.options.write_alignment;

            if (old_block_index != new_block_index || state.last_block_start_time.is_none()) {
                if state.offset % self.shared.options.write_alignment == 0 {
                    // Currently at the start of a 0 length block.
                    state.last_block_start_time = None;
                } else {
                    state.last_block_start_time = Some(Instant::now());
                }
            }

            // TODO: Filter to all call if we hit the byte threshold.
            state.notify_all();
        }

        Ok(n)
    }

    // Want to immediately force flushing.
    async fn flush(&mut self) -> Result<()> {
        let mut state = self.shared.state.lock().await;
        let target_offset = state.offset;
        state.force_offset = target_offset;
        state.notify_all();
        drop(state);

        loop {
            let mut state = self.shared.state.lock().await;

            if state.flushed_offset >= target_offset {
                break;
            }

            state.flush_error.get()?;

            state.wait(()).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use file::LocalFileOpenOptions;

    use super::*;

    #[testcase]
    async fn test_log_writer() -> Result<()> {
        // If correct, this test should take less than 100ms to run.

        let temp_dir = file::temp::TempDir::create()?;
        let log_path = temp_dir.path().join("log");

        let mut options = LogWriterOptions::default();
        options.flush_timeout_threshold = Duration::from_secs(10);

        let file = file::LocalFile::open_with_options(
            log_path,
            &LocalFileOpenOptions::new().create_new(true).write(true),
        )?;
        let mut writer = LogWriter::create(options, file).await?;

        writer.write_all(&[1, 2, 3]).await?;
        writer.flush().await?;

        writer.write_all(&[4, 5, 6]).await?;
        writer.flush().await?;

        Ok(())
    }
}
