//! Code for performing logging of the stdout/stderr pipes of the container to
//! durable storage.

use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use common::errors::*;
use common::io::Readable;
use executor::lock_async;
use executor::sync::AsyncMutex;
use file::LocalFile;
use file::LocalPath;
use protobuf::{Message, StaticMessage};
use sstable::record_log::{RecordReader, RecordWriter};

use crate::proto::*;

const MAX_LINE_SIZE: usize = 1024 * 8;

pub struct FileLogReader {
    log: RecordReader,
}

impl FileLogReader {
    pub async fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        Ok(Self {
            log: RecordReader::open(path.as_ref()).await?,
        })
    }

    // TODO: Useful semantics would be to have this always retry from the same start
    // position if we need to retry.
    pub async fn read(&mut self) -> Result<Option<LogEntry>> {
        let data = self.log.read().await?;
        if let Some(data) = data {
            Ok(Some(LogEntry::parse(&data)?))
        } else {
            Ok(None)
        }
    }
}

pub struct FileLogWriterOptions {
    pub split_on_line: bool,
    pub max_record_size: usize,

    pub flush_on_line: bool,
    pub flush_on_timeout: Duration,
}

/// Writer for appending to a file based process output log.
///
/// The log is internally an ordered list of LogEntry proto records.
pub struct FileLogWriter {
    log: AsyncMutex<RecordWriter>,
}

impl FileLogWriter {
    /// Creates a new writer which will write to the file located at 'path'.
    pub async fn create(path: &LocalPath) -> Result<Self> {
        Ok(Self {
            log: AsyncMutex::new(RecordWriter::create_new(path).await?),
        })
    }

    /// Reads all the contents of 'file' and writes it to the log.
    ///
    /// - 'stream': Tag to use when creating log entries read from the file.
    pub async fn write_stream(&self, file: LocalFile, stream: LogStream) -> Result<()> {
        FileLogStreamWriter {
            log: &self.log,
            stream,
        }
        .write(file)
        .await
    }
}

struct FileLogStreamWriter<'a> {
    log: &'a AsyncMutex<RecordWriter>,
    stream: LogStream,
}

impl<'a> FileLogStreamWriter<'a> {
    /*
    Want to support not waiting for lines to be flushed.
    - Need to explicitly flus once done

    Also want to support waiting for at least N bytes
    */

    async fn write(&self, mut file: LocalFile) -> Result<()> {
        let mut buffer = vec![0u8; MAX_LINE_SIZE];
        let mut buffer_size = 0;

        loop {
            let nread = file.read(&mut buffer[buffer_size..]).await?;
            if nread == 0 {
                break;
            }

            let time = SystemTime::now();

            // Index imediately after the '\n' in the last line.
            let mut last_line_end = 0;

            for i in buffer_size..(buffer_size + nread) {
                if buffer[i] == b'\n' {
                    let line = &buffer[last_line_end..(i + 1)];
                    last_line_end = i + 1;

                    self.write_line(line, time, false).await?;
                }
            }

            buffer_size += nread;

            if (true || buffer_size == buffer.len()) && last_line_end == 0 {
                // In this case, the line is longer that our buffer size so we'll just write a
                // non-terminated line consisting on the entire buffer.
                let line = &buffer[0..buffer_size];
                last_line_end = buffer_size;

                self.write_line(line, time, false).await?;
            }

            // Shift all remaining data to the start of the buffer
            // Basically do buffer[0..m] = buffer[n..(n+m)]
            let remaining_size = buffer_size - last_line_end;
            let remaining_start = buffer_size - remaining_size;
            for i in 0..remaining_size {
                buffer[i] = buffer[remaining_start + i];
            }

            buffer_size = remaining_size;

            // TODO: Don't need this to fsync. Just need to make it far enough into the OS
            // for other users to be able to read it.
            lock_async!(log <= self.log.lock().await?, { log.flush().await })?;
        }

        // Always write a final entry with any remaining data to mark that the stream
        // has now been closed.
        let time = SystemTime::now();
        let line = &buffer[0..buffer_size];
        self.write_line(line, time, true).await?;
        lock_async!(log <= self.log.lock().await?, { log.flush().await })?;

        Ok(())
    }

    async fn write_line(&self, line: &[u8], time: SystemTime, end_stream: bool) -> Result<()> {
        let mut log_entry = LogEntry::default();
        log_entry.set_stream(self.stream);
        log_entry.set_timestamp(time);
        log_entry.set_end_stream(end_stream);
        log_entry.value_mut().extend_from_slice(line);

        let mut log = self.log.lock().await;
        lock_async!(log <= self.log.lock().await?, {
            log.append(&log_entry.serialize()?).await
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crypto::random::SharedRng;

    use super::*;

    #[testcase]
    async fn test_read_in() -> Result<()> {
        let mut data = vec![];
        data.resize(60000, 0);
        crypto::random::global_rng().generate_bytes(&mut data).await;

        // TODO: Must improve the RecordWriter.
        let mut writer = RecordWriter::create_new(&LocalPath::new("/tmp/log")).await?;
        writer.append(&data).await?;
        drop(writer);

        let mut reader = RecordReader::open(&LocalPath::new("/tmp/log")).await?;

        let data_read = reader.read().await?;
        assert_eq!(data_read, Some(data));

        Ok(())
    }
}
