use std::collections::VecDeque;
use std::ops::DerefMut;
use std::sync::Arc;

use common::errors::*;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::{AsyncMutex, AsyncVariable};
use file::{LocalPath, LocalPathBuf};
use protobuf::{Message, StaticMessage};
use sstable::log_writer::LogFlushSubscriber;
use sstable::record_log::{RecordReader, RecordWriter};

use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::log::memory_log::MemoryLogSync;
use crate::proto::*;

#[derive(Defaultable)]
pub struct SegmentedLogOptions {
    /// Once the latest segment gets this large, we will start transitioning
    /// writes to a new file.
    #[default(32 * 1024 * 1024)]
    pub target_segment_size: u64,

    #[default(64 * 1024 * 1024)]
    pub max_segment_size: u64,
}

/// Implementation of the Raft log as a chain of append-only record files.
///
/// All log files are stored in a separate directory with each file being named
/// with a number of the form '{:08d}'. The first log starts with number 1 and
/// each following log after log file 'i' has a number 'i+1'.
///
/// All files are append only. Truncations are implemented by appending a new
/// log entry with an index <= a previous one. This means that the entire log
/// must be read to the end before it can be used.
///
/// Discarding is implemented by simply deleting earlier log files. Currently we
/// always retain the live and one old log file. This ensures that for usecases
/// like Raft, we retain old commited entries in case need them later for
/// followers that were offline or falling behind.
///
/// TODO: Have a time bound on how long log entries stick around after being
/// discarded as we want to eventually support TTLed data.
///
/// TODO: If we inline commit indexes into the log, then that could be used to
/// finalize part of the log early.
///
/// TODO: Implement limiting the max size of the log (though will need to ensure
/// that we can definately perform a state machine snapshot that will allow us
/// to discard the log)
pub struct SegmentedLog {
    shared: Arc<Shared>,
    thread: ChildTask,
}

struct Shared {
    // TODO: Use a synced path here.
    dir: LocalPathBuf,

    options: SegmentedLogOptions,
    state: AsyncVariable<SegmentedLogState>,
}

struct SegmentedLogState {
    memory_log: MemoryLogSync,

    /// All contiguous segments still present in the log.
    /// The oldest log entry will be in the first segment and the newest
    /// segments will be in the last segment. This will always contain at least
    /// one segment.
    segments: VecDeque<Segment>,

    /// Last position written to the final/newest segment.
    last_position: LogPosition,

    /// Last sequence that has been confirmed to be fully flushed to persistent
    /// storage.
    last_flushed: LogSequence,

    flush_error: LatchingError,

    /// Tracker for wait_for_flushed().
    last_observation: Option<(LogPosition, LogSequence)>,
}

struct Segment {
    /// Number that makes the file name of this segment.
    number: usize,

    /// Range of log indexes in the current log that are backed by this segment.
    ///
    /// The range is of the form '[first_index, last_index)'
    index_range: Option<(LogIndex, LogIndex)>,

    /// If non-None, then this segment is still open for writing.
    open: Option<OpenSegment>,

    /// Whether or not this segment has been discarded due to a recent discard()
    /// call.
    discarded: bool,
}

struct OpenSegment {
    /// Writer for this segment.
    writer: RecordWriter,

    /// End offset in the current segment at which each each
    ///
    /// TODO: Compress out of the LogSequence storage as these will usually be
    /// consecutive.
    entry_end_offsets: VecDeque<(LogSequence, u64)>,
}

impl SegmentedLog {
    // TODO: Pass in an initial discard position to perform initial discarding (to
    // avoid storing the full log in memory inf not needed.)
    pub async fn open<P: AsRef<LocalPath>>(dir: P, options: SegmentedLogOptions) -> Result<Self> {
        let dir_path = dir.as_ref();

        if !file::exists(dir_path).await? {
            file::create_dir(dir_path).await?;
        }

        // List of files read from disk. Each entry is a tuple of (log_number, PathBuf)
        let mut files = vec![];
        {
            for entry in file::read_dir(dir_path)? {
                let log_number = entry.name().parse::<usize>()?;

                files.push((log_number, dir_path.join(entry.name())));
            }

            files.sort();
        }

        let mut state = SegmentedLogState {
            memory_log: MemoryLogSync::new(),

            segments: VecDeque::new(),

            last_position: LogPosition::zero(),

            // NOTE: Until we fully flush the logs, we aren't sure if the existing data has been
            // persisted.
            //
            // TODO: Need some initialization
            last_flushed: LogSequence::zero(),

            flush_error: LatchingError::default(),

            last_observation: None,
        };

        let mut last_sequence = LogSequence::zero();

        let mut last_log_reader = None;

        // TODO: Ensure that all existing logs are fsync'ed already in their initial
        // state.

        for (log_number, log_path) in &files {
            let mut reader = RecordReader::open(log_path).await?;

            let header_record = match reader.read().await? {
                Some(data) => data,
                None => {
                    // This should only happen for the final log. The log file may have been created
                    // but the first record wasn't written out yet. May happen with intermediate
                    // files if discards did not fully fsync.
                    file::remove_file(log_path).await?;
                    continue;
                }
            };

            let header = SegmentedLogRecord::parse(&header_record)?;

            if !header.has_prev() || header.has_entry() {
                return Err(err_msg("Malformed header entry in log"));
            }

            // NOTE: We don't validate continuity of 'prev' or log numbers between
            // consecutive segments as discontinuities may show up during log
            // truncations/discards.
            if state.last_position.index() != header.prev().index() {
                state.memory_log.discard(header.prev().clone())?;
                state.last_position = header.prev().clone();

                for segment in &mut state.segments {
                    segment.discarded = true;
                    segment.index_range = None;
                }
            }

            state.segments.push_back(Segment {
                number: *log_number,
                index_range: None,
                open: None,
                discarded: false,
            });

            while let Some(body_record) = reader.read().await? {
                let record = SegmentedLogRecord::parse(&body_record)?;

                if record.has_prev() {
                    return Err(err_msg(
                        "Only expecting 'prev' in the header entry in each log segment",
                    ));
                }

                if record.has_entry() {
                    // NOTE: We assume that all read data is already flushed as RecordReader
                    // performs an fsync before
                    let sequence = state.last_flushed.next();
                    state.last_flushed = sequence;

                    Self::append_in_memory_impl(record.entry(), sequence, &mut state)?;
                }
            }

            last_log_reader = Some(reader);
        }

        let dir = dir_path.to_owned();

        let last_writer = {
            if let Some(last_reader) = last_log_reader {
                last_reader.into_writer(true).await?.unwrap()
            } else {
                let number = 1;

                let mut writer =
                    RecordWriter::create_new(dir.join(Self::log_file_name(number))).await?;
                let mut header = SegmentedLogRecord::default();
                header.set_prev(state.last_position.clone());
                writer.append(&header.serialize()?).await?;

                state.segments.push_back(Segment {
                    number,
                    index_range: None,
                    open: None,
                    discarded: false,
                });

                writer
            }
        };

        let last_segment = state.segments.back_mut().unwrap();
        last_segment.open = Some(OpenSegment {
            writer: last_writer,
            entry_end_offsets: VecDeque::default(),
        });

        let shared = Arc::new(Shared {
            dir,
            options,
            state: AsyncVariable::new(state),
        });

        let thread = ChildTask::spawn(Self::flusher_thread(shared.clone()));

        Ok(Self { shared, thread })
    }

    /// Appends an already on disk entry to the in-memory state of the log.
    ///
    /// We assume that the given entry is going to be appended to the
    /// last segment in our state immediately after calling this.
    fn append_in_memory_impl(
        entry: &LogEntry,
        sequence: LogSequence,
        state: &mut SegmentedLogState,
    ) -> Result<()> {
        state.memory_log.append(entry.clone(), sequence)?;

        // Handle truncation.
        if entry.pos().index() <= state.last_position.index() {
            let num_segments = state.segments.len();

            for (segment_i, segment) in state.segments.iter_mut().enumerate() {
                let is_last = segment_i + 1 == num_segments;

                if let Some((s, e)) = &mut segment.index_range {
                    if entry.pos().index() < *e {
                        *e = entry.pos().index();

                        if *e <= *s {
                            segment.index_range = None;
                            segment.discarded = true;
                        }
                    }
                }
            }
        }

        let segment = state.segments.back_mut().unwrap();
        if let Some((s, e)) = &mut segment.index_range {
            *e = entry.pos().index() + 1;
        } else {
            segment.index_range = Some((entry.pos().index(), entry.pos().index() + 1));
        }

        // Even if we extended up truncating all the previous entries in the last
        // segment, we still assume the caller will still use it for writing. Also other
        // code is written under the assumption that the last segment is always valid.
        segment.discarded = false;

        state.last_position = entry.pos().clone();

        Ok(())
    }

    fn log_file_name(number: usize) -> String {
        format!("{:08}", number)
    }

    async fn flusher_thread(shared: Arc<Shared>) {
        if let Err(e) = Self::flusher_thread_impl(shared.clone()).await {
            lock!(state <= shared.state.lock().await.unwrap(), {
                state.flush_error.set(e);
                state.notify_all();
            });
        }
    }

    async fn flusher_thread_impl(shared: Arc<Shared>) -> Result<()> {
        loop {
            let mut logs_to_discard = vec![];
            let mut new_log_number = None;
            let mut pending_flusher = None;

            let mut guard = shared.state.lock().await?.enter();
            let state: &mut SegmentedLogState = &mut guard;

            let mut i = 0;
            while i < state.segments.len() {
                let is_last = i + 1 == state.segments.len();
                let is_empty = state.segments[i].index_range.is_none();
                let current_number = state.segments[i].number;

                // Check if we need to discard some the segment.
                if state.segments[i].discarded {
                    let mut s = state.segments.remove(i).unwrap();
                    s.open = None;
                    logs_to_discard.push(s);
                    continue;
                }

                if let Some(open) = &mut state.segments[i].open {
                    let mut flusher = open.writer.new_flush_subscriber();

                    // If all previous segments are fully flushed, then we can advance our flushed
                    // sequence based on flushed entries in the current segment.
                    if pending_flusher.is_none() {
                        let offset = flusher.last_flushed_offset().await?;

                        while !open.entry_end_offsets.is_empty()
                            && open.entry_end_offsets[0].1 <= offset
                        {
                            let (seq, _) = open.entry_end_offsets.pop_front().unwrap();
                            state.last_flushed = seq;
                        }
                    }

                    // Check if the last segment is getting big enough that we need to make a new
                    // one.
                    //
                    // NOTE: Every segment must have enough space for at least one entry.
                    if is_last {
                        let size = open.writer.current_size();
                        if !is_empty && size >= shared.options.target_segment_size {
                            new_log_number = Some(current_number + 1);
                        }
                    }

                    // Close segments which have been fully flushed.
                    if !is_last && open.entry_end_offsets.is_empty() {
                        state.segments[i].open = None;
                    } else {
                        // Still need to wait for more flushes for this segment.
                        pending_flusher = Some(flusher);
                    }
                }

                i += 1;
            }

            drop(state);
            guard.notify_all();

            // If we have nothing to do, wait for something to happen
            if logs_to_discard.is_empty() && new_log_number.is_none() {
                let state_change = guard.wait();

                race!(
                    state_change,
                    executor::futures::optional(
                        pending_flusher.map(|mut s| async move { s.wait_for_flush().await })
                    )
                )
                .await;
                continue;
            }

            guard.exit();

            // Below we do slow work while the state isn't locked.

            if let Some(log_number) = new_log_number {
                // TODO: Make sure this does some pre-allocation so that the future code is
                // quick.
                let mut writer =
                    RecordWriter::create_new(shared.dir.join(Self::log_file_name(log_number)))
                        .await?;

                let mut state = shared.state.lock().await?.enter();

                let mut header = SegmentedLogRecord::default();
                header.set_prev(state.last_position.clone());
                writer.append(&header.serialize()?).await?;

                state.segments.push_back(Segment {
                    number: log_number,
                    index_range: None,
                    discarded: false,
                    open: Some(OpenSegment {
                        writer,
                        entry_end_offsets: VecDeque::new(),
                    }),
                });

                state.notify_all();
                state.exit();
            }

            for segment in logs_to_discard {
                // Delete the discarded log file.
                // NOTE: We don't care about fsyncing this as there is no problem with having
                // too many
                file::remove_file(shared.dir.join(Self::log_file_name(segment.number))).await?;
            }
        }
    }

    async fn create_new_segment(
        shared: &Shared,
        log_number: usize,
        prev: LogPosition,
    ) -> Result<Segment> {
        let mut writer =
            RecordWriter::create_new(shared.dir.join(Self::log_file_name(log_number))).await?;
        let mut header = SegmentedLogRecord::default();
        header.set_prev(prev.clone());
        writer.append(&header.serialize()?).await?;

        Ok(Segment {
            number: log_number,
            index_range: None,
            discarded: false,
            open: Some(OpenSegment {
                writer,
                entry_end_offsets: VecDeque::new(),
            }),
        })
    }
}

#[async_trait]
impl Log for SegmentedLog {
    async fn term(&self, index: LogIndex) -> Option<Term> {
        self.shared
            .state
            .lock()
            .await
            .unwrap()
            .read_exclusive()
            .memory_log
            .term(index)
    }

    async fn prev(&self) -> LogPosition {
        // NOTE: We may have more entries on disk, but we currently only support using
        // the ones that are still in memory.
        self.shared
            .state
            .lock()
            .await
            .unwrap()
            .read_exclusive()
            .memory_log
            .prev()
    }

    async fn last_index(&self) -> LogIndex {
        self.shared
            .state
            .lock()
            .await
            .unwrap()
            .read_exclusive()
            .memory_log
            .last_index()
    }

    async fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSequence)> {
        self.shared
            .state
            .lock()
            .await
            .unwrap()
            .read_exclusive()
            .memory_log
            .entry(index)
    }

    async fn entries(
        &self,
        start_index: LogIndex,
        end_index: LogIndex,
    ) -> Option<(Vec<Arc<LogEntry>>, LogSequence)> {
        self.shared
            .state
            .lock()
            .await
            .unwrap()
            .read_exclusive()
            .memory_log
            .entries(start_index, end_index)
    }

    async fn append(&self, entry: LogEntry, sequence: LogSequence) -> Result<()> {
        // Acquire the state blocking until we have a segment that is small enough to
        // handle more writes.
        let mut state = {
            let mut state = self.shared.state.lock().await?.enter();

            loop {
                let last_segment = state.segments.back_mut().unwrap();

                // NOTE: Every segment must have enough space for at least one entry.
                if last_segment.index_range.is_some()
                    && last_segment.open.as_mut().unwrap().writer.current_size()
                        >= self.shared.options.max_segment_size
                {
                    state.wait().await;
                    state = self.shared.state.lock().await?.enter();
                    continue;
                }

                break;
            }

            state
        };

        Self::append_in_memory_impl(&entry, sequence, &mut state)?;

        let mut record = SegmentedLogRecord::default();
        record.set_entry(entry);

        let segment = state.segments.back_mut().unwrap();
        assert!(!segment.discarded);

        let open = segment.open.as_mut().unwrap();
        open.writer.append(&record.serialize()?).await?;
        open.entry_end_offsets
            .push_back((sequence, open.writer.current_size()));

        // We may need to roll over to a new log file if the current one is now too big.
        // TODO: Filter these notifications if we aren't close to the limit.
        state.notify_all();
        state.exit();

        Ok(())
    }

    async fn discard(&self, pos: LogPosition) -> Result<()> {
        // TODO: Must support discarding beyond the end of the log. In particular, we
        // need to support discontinuities in log indexes as we may receive a state
        // machine snapshot.

        let mut state = self.shared.state.lock().await?.enter();

        // Find the first segment that we want to keep.
        //
        // This will normally be the segment that contains 'pos.index + 1' (the first
        // entry we want to keep).
        //
        // But, we may also want to keep the last segment
        //
        let mut first_to_keep = None;
        for (i, segment) in state.segments.iter().enumerate() {
            let is_last = i == state.segments.len();

            // Special case for the last segment: If the last segment is empty, but 'prev'
            // == 'pos', we will keep the segment as there is no point in discarding it.
            if is_last
                && segment.index_range.is_none()
                && state.last_position.index() == pos.index()
            {
                first_to_keep = Some(i);
                break;
            }

            if let Some((s, e)) = segment.index_range {
                if
                // Commented out to support discarding before the start of the log.
                /* (pos.index() + 1) >= s && */
                (pos.index() + 1) < e {
                    first_to_keep = Some(i);
                    break;
                }
            }
        }

        if let Some(mut first_to_keep) = first_to_keep {
            // Scan back to find one more file that contains some data to keep.

            let mut i = first_to_keep;
            while i > 0 {
                i -= 1;

                if state.segments[i].index_range.is_some() {
                    first_to_keep = i;
                    break;
                }
            }

            // NOTE: index_range should only be None if we are in the last segment and it is
            // empty.
            let first_log_index = state.segments[first_to_keep]
                .index_range
                .map(|(s, _)| s)
                .unwrap_or(state.last_position.index());

            let mut prev = LogPosition::default();
            prev.set_index(first_log_index - 1);
            prev.set_term(state.memory_log.term(prev.index()).unwrap());

            state.memory_log.discard(prev)?;

            // Delete everything before the above file.
            for i in 0..first_to_keep {
                state.segments[i].index_range = None;
                state.segments[i].discarded = true;
            }
        } else {
            // Discarding beyond the end of the log.
            // Make a new segment then discard all old segments.

            // Make the new segment
            {
                let number = state.segments.back_mut().unwrap().number + 1;
                let segment = Self::create_new_segment(&self.shared, number, pos.clone()).await?;
                state.segments.push_back(segment);
            }

            // TODO: Deduplicate this with code in 'open()'.

            state.memory_log.discard(pos.clone())?;
            state.last_position = pos;

            // Discard all the old segments.
            for i in 0..(state.segments.len() - 1) {
                state.segments[i].index_range = None;
                state.segments[i].discarded = true;
            }
        }

        state.notify_all();
        state.exit();

        Ok(())
    }

    async fn last_flushed(&self) -> LogSequence {
        let state = self.shared.state.lock().await.unwrap().read_exclusive();
        state.last_flushed.clone()
    }

    async fn wait_for_flush(&self) -> Result<()> {
        loop {
            let mut state = self.shared.state.lock().await?.enter();
            state.flush_error.get()?;

            let next_observation = Some((state.memory_log.prev(), state.last_flushed.clone()));

            if state.last_observation == next_observation {
                state.wait().await;
                continue;
            }

            state.last_observation = next_observation;
            state.exit();
            break;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    async fn dir_contents(path: &LocalPath) -> Result<Vec<String>> {
        let mut out = vec![];

        for entry in file::read_dir(path)? {
            out.push(entry.name().to_string());
        }

        out.sort();

        Ok(out)
    }

    // TODO: Need to test this with some file mocking to simulate slow fsyncing.

    #[testcase]
    async fn works() -> Result<()> {
        let temp_dir = file::temp::TempDir::create()?;

        let mut options = SegmentedLogOptions::default();
        // 4 entries per log segment.
        options.target_segment_size = 4096;
        options.max_segment_size = 4096;

        let log = SegmentedLog::open(temp_dir.path(), options).await?;

        let mut last_seq = LogSequence::zero();

        let mut entry = LogEntry::default();
        entry.pos_mut().set_term(1);
        entry.pos_mut().set_index(1);
        entry.data_mut().set_command(vec![0u8; 1024]);

        for i in 1..16 {
            last_seq = last_seq.next();

            entry.pos_mut().set_index(i);
            log.append(entry.clone(), last_seq).await?;
        }

        assert_eq!(
            dir_contents(temp_dir.path()).await?,
            vec![
                "00000001".to_string(), // 1, 2, 3, 4
                "00000002".to_string(), // 5, 6, 7, 8
                "00000003".to_string(), // 9, 10, 11, 12
                "00000004".to_string(), // 13, 14, 15
            ]
        );

        while log.last_flushed().await != last_seq {
            log.wait_for_flush().await?;
        }

        // TODO: Re-open the log and verify we still have content.
        // (ideally we test with an without a re-open to verify the states from restore
        // and )

        {
            assert_eq!(log.prev().await, LogPosition::new(0, 0));

            log.discard(LogPosition::new(1, 0)).await?;
            executor::sleep(Duration::from_millis(100)).await;

            assert_eq!(
                dir_contents(temp_dir.path()).await?,
                vec![
                    "00000001".to_string(), // 1, 2, 3, 4
                    "00000002".to_string(), // 5, 6, 7, 8
                    "00000003".to_string(), // 9, 10, 11, 12
                    "00000004".to_string(), // 13, 14, 15
                ]
            );

            assert_eq!(log.prev().await, LogPosition::new(0, 0));
        }

        {
            let discard_pos = LogPosition::new(1, 13);
            log.discard(discard_pos).await?;

            executor::sleep(Duration::from_millis(100)).await;

            assert_eq!(
                dir_contents(temp_dir.path()).await?,
                vec![
                    "00000003".to_string(), // 9, 10, 11, 12
                    "00000004".to_string(), // 13, 14, 15
                ]
            );

            assert_eq!(log.prev().await, LogPosition::new(1, 8));
        }

        // Re-discard already discarded entries.
        {
            let discard_pos = LogPosition::new(1, 6);
            log.discard(discard_pos).await?;

            executor::sleep(Duration::from_millis(100)).await;

            assert_eq!(
                dir_contents(temp_dir.path()).await?,
                vec![
                    "00000003".to_string(), // 9, 10, 11, 12
                    "00000004".to_string(), // 13, 14, 15
                ]
            );

            assert_eq!(log.prev().await, LogPosition::new(1, 8));
        }

        // TODO: Re-open the log and verify we still have content.

        // Discard beyond the end of the log.
        {
            let discard_pos = LogPosition::new(1, 20);
            log.discard(discard_pos).await?;
            assert_eq!(log.prev().await, LogPosition::new(1, 20));

            executor::sleep(Duration::from_millis(100)).await;

            assert_eq!(
                dir_contents(temp_dir.path()).await?,
                vec!["00000005".to_string(),]
            );

            assert_eq!(log.prev().await, LogPosition::new(1, 20));
        }

        // TODO: Re-open the log and verify we still have content.

        // TODO: Re-open the log and verify that without any changes we eventually
        // report that all the contents are flushed to disk.

        Ok(())
    }
}
