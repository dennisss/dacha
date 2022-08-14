use std::collections::VecDeque;
use std::ops::DerefMut;
use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::errors::*;
use common::{async_std::path::Path, fs::sync::SyncedDirectory, futures::StreamExt};
use protobuf::{Message, StaticMessage};
use sstable::record_log::{RecordReader, RecordWriter};

use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::log::memory_log::MemoryLog;
use crate::proto::consensus::{LogEntry, LogPosition};
use crate::proto::ident::*;
use crate::proto::log::SegmentedLogRecord;

/*
    A log implementation based on the RecordIO format in the other format

    -> The log is implemented as one or more log files

    The latest one will have name 'log'
    - All older ones will have name 'history-X' where X starts at 1 and is larger than any other history-X file created before it

    - The total log can be reconstructed by looking at the full concatenated sequence:
        [ history-1 ... history-n, log ]

    - Calling snapshot() on the log will freeze the current log file and atomically append it to the list of all history files
        - Then a new log file will be created with a prev_log_index pointing to the last entry in the previous file
        -> This will then return the log index of the latest entry in the log

    - The log will also support a discard() operation
        - Given a log index, this may delete up to and including that index (but never more than it)
        - THis will always be effectively deleting some number of history files but will keep everything in the current log file

    - So in order to snapshot the database
        - We first snapshot the log
        - We then wait for at least that index to be applied to the state machine (or totally overriden by newer entries)
        - This complicates things

    - So we trigger a snapshot to start in the matcher
        - Interestingly we can still snapshot
        -

    TODO: Other considerations
    -> In some applications like a Kafka style thing, we may want to retain the tweaking parameters to optimize for a more disk-first log

    A similar discussion of Raft logs:
    - https://github.com/cockroachdb/cockroach/issues/7807
    - https://ayende.com/blog/174753/fast-transaction-log-linux?key=273aa566963445188a9c1c5ef3463311

    Because of truncations, the commit index sometimes can't be written before the regular entries
    - But without having the commit index, we can't apply changes before

    - We must limit the number of uncommited log entries visible in the log at any point in time
        - Such that if we get any

    - So discard may require an additional argument that specifies that next expected index (such that we never accept any future record with a different term)

    Can we coordinate

    Occasionally we will ask for metadata to be added to the log.
    -
*/

// TODO: We shouldn't immediately discard commited records from memory in case
// we need to retry sending them to some stragglers.

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
/// Discarding is implemented by simply deleting earlier
///
/// TODO: If we inline commit indexes into the log, then that could be used to
/// finalize part of the log early.
pub struct SegmentedLog {
    dir: SyncedDirectory,

    max_segment_size: u64,

    /// TODO: Use a MemoryLog implementation that doesn't internally use locking
    /// given that we already have our own lock for 'state'.
    memory_log: MemoryLog,

    state: Mutex<SegmentedLogState>,
}

struct SegmentedLogState {
    /// All contiguous segments still present in the log.
    /// The oldest log entry will be in the first segment and the newest
    /// segments will be in the last segment. This will always contain at least
    /// one segment.
    segments: VecDeque<Segment>,

    /// Writer for the final segment in 'segments'.
    writer: RecordWriter,

    last_flushed: LogSequence,
}

struct Segment {
    number: usize,
    last_position: LogPosition,
}

impl SegmentedLog {
    pub async fn open<P: AsRef<Path>>(dir: P, max_segment_size: u64) -> Result<Self> {
        let dir_path = dir.as_ref();

        if !dir_path.exists().await {
            common::async_std::fs::create_dir(dir_path).await?;
        }

        // List of files read from disk. Each entry is a tuple of (log_number, PathBuf)
        let mut files = vec![];
        {
            let mut entries = common::async_std::fs::read_dir(dir_path).await?;
            while let Some(entry) = entries.next().await {
                let entry = entry?;

                let log_number = entry
                    .file_name()
                    .to_str()
                    .ok_or_else(|| err_msg("File name not a valid string"))?
                    .parse::<usize>()?;

                files.push((log_number, entry.path()));
            }

            files.sort();
        }

        let memory_log = MemoryLog::new();

        let mut segments: VecDeque<Segment> = VecDeque::new();

        let mut last_sequence = LogSequence::zero();

        let mut last_log_path = None;

        for (log_number, log_path) in &files {
            let mut reader = RecordReader::open(log_path).await?;

            let header_record = match reader.read().await? {
                Some(data) => data,
                None => {
                    // This should only happen for the final log. The log file may have been created
                    // but the first record wasn't written out yet.
                    //
                    // TODO: Only delete if this is the last file.
                    common::async_std::fs::remove_file(log_path).await?;
                    continue;
                }
            };

            let header = SegmentedLogRecord::parse(&header_record)?;

            if let Some(last_segment) = segments.back() {
                if &last_segment.last_position != header.prev()
                    || last_segment.number + 1 != *log_number
                {
                    return Err(err_msg("Discontinuity in log segments"));
                }
            } else {
                memory_log.discard(header.prev().clone()).await?;
            }

            let mut last_position = header.prev().clone();

            while let Some(body_record) = reader.read().await? {
                let record = SegmentedLogRecord::parse(&body_record)?;

                if record.has_prev() {
                    memory_log.discard(record.prev().clone()).await?;
                }

                if record.has_entry() {
                    let sequence = last_sequence.next();
                    last_sequence = sequence;

                    let entry = record.entry();
                    last_position = entry.pos().clone();

                    memory_log.append(entry.clone(), sequence).await?;
                }
            }

            last_log_path = Some(log_path.as_path());
            segments.push_back(Segment {
                number: *log_number,
                last_position,
            });
        }

        let dir = SyncedDirectory::open(dir_path)?;

        let writer = {
            if let Some(last_path) = last_log_path {
                // TODO: Use the SyncedDirectory object.
                RecordWriter::open(last_path).await?
            } else {
                let number = 1;
                let last_position = LogPosition::zero();

                let mut writer =
                    RecordWriter::open_with(dir.path(Self::log_file_name(number))?).await?;
                let mut header = SegmentedLogRecord::default();
                header.set_prev(last_position.clone());
                writer.append(&header.serialize()?).await?;

                segments.push_back(Segment {
                    number,
                    last_position,
                });

                writer
            }
        };

        Ok(Self {
            dir,
            max_segment_size,
            memory_log,
            state: Mutex::new(SegmentedLogState {
                segments,
                writer,

                // NOTE: Until we fully flush the logs, we aren't sure if the existing data has been
                // persisted.
                last_flushed: LogSequence::zero(),
            }),
        })
    }

    fn log_file_name(number: usize) -> String {
        format!("{:08}", number)
    }
}

#[async_trait]
impl Log for SegmentedLog {
    async fn term(&self, index: LogIndex) -> Option<Term> {
        self.memory_log.term(index).await
    }

    async fn prev(&self) -> LogPosition {
        // NOTE: We may have more entries on disk, but we currently only support using
        // the ones that are still in memory.
        self.memory_log.prev().await
    }

    async fn last_index(&self) -> LogIndex {
        self.memory_log.last_index().await
    }

    async fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSequence)> {
        self.memory_log.entry(index).await
    }

    async fn append(&self, entry: LogEntry, sequence: LogSequence) -> Result<()> {
        let mut guard = self.state.lock().await;
        let state = guard.deref_mut();

        let mut record = SegmentedLogRecord::default();
        record.set_entry(entry.clone());

        state.writer.append(&record.serialize()?).await?;

        let segment = state.segments.back_mut().unwrap();
        segment.last_position = entry.pos().clone();

        if state.writer.current_size().await? >= self.max_segment_size {
            // TODO: Update the flushed sequence here.
            state.writer.flush().await?;

            // Create a new log.
            let number = segment.number + 1;
            let last_position = segment.last_position.clone();

            // TODO: Deduplicate with the other code for initializing a file.
            let mut writer =
                RecordWriter::open_with(self.dir.path(Self::log_file_name(number))?).await?;
            let mut header = SegmentedLogRecord::default();
            header.set_prev(last_position.clone());
            writer.append(&header.serialize()?).await?;

            state.segments.push_back(Segment {
                number,
                last_position,
            });

            state.writer = writer;
        }

        self.memory_log.append(entry, sequence).await?;
        Ok(())
    }

    async fn discard(&self, pos: LogPosition) -> Result<()> {
        // TODO: Must support discarding beyond the end of the log. In particular, we
        // need to support discontinuities in log indexes as we may receive a state
        // machine snapshot.

        let mut state = self.state.lock().await;

        // NOTE: We will never discard the last segment (as it is being used for
        // writes). TODO: Save the discard position for later and check if we
        // can discard entries
        while state.segments.len() > 1 {
            let last_pos = &state.segments[0].last_position;
            if pos.term() > last_pos.term() || pos.index() >= last_pos.index() {
                let seg = state.segments.pop_front().unwrap();

                // Delete the discarded log file.
                // NOTE: We don't care about fsyncing this as there is no problem with having
                // too many
                common::async_std::fs::remove_file(
                    self.dir.path(Self::log_file_name(seg.number))?.read_path(),
                )
                .await?;
            } else {
                break;
            }
        }

        let mut record = SegmentedLogRecord::default();
        record.set_prev(pos.clone());
        state.writer.append(&record.serialize()?).await?;
        state.segments.back_mut().unwrap().last_position = pos.clone();

        self.memory_log.discard(pos).await?;

        Ok(())
    }

    async fn last_flushed(&self) -> LogSequence {
        let state = self.state.lock().await;
        state.last_flushed.clone()
    }

    async fn flush(&self) -> Result<()> {
        let mut state = self.state.lock().await;

        self.memory_log.flush().await?;
        state.writer.flush().await?;
        state.last_flushed = self.memory_log.last_flushed().await;

        Ok(())
    }
}
