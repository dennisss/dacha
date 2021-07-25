// Code for performing logging of the stdout/stderr pipes of the container to durable storage.

use std::time::SystemTime;
use std::sync::Arc;
use std::path::Path;

use common::async_std::fs::File;
use common::errors::*;
use common::async_std::sync::Mutex;
use common::async_std::io::ReadExt;
use sstable::record_log::{RecordWriter, RecordReader};
use protobuf::Message;

use crate::proto::log::*;


const MAX_LINE_SIZE: usize = 1024*8;

pub struct FileLogReader {
    log: RecordReader
}

impl FileLogReader {
    pub async fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            log: RecordReader::open(path).await?
        })
    }

    // TODO: Useful semantics would be to have this always retry from the same start position if 
    // we need to retry.
    pub async fn read(&mut self) -> Result<Option<LogEntry>> {
        let data = self.log.read().await?;
        if let Some(data) = data {
            Ok(Some(LogEntry::parse(&data)?))
        } else {
            Ok(None)
        }
    }
}



pub struct FileLogWriter {
    log: Mutex<RecordWriter>
}

impl FileLogWriter {
    pub async fn create(path: &Path) -> Result<Self> {
        Ok(Self {
            log: Mutex::new(RecordWriter::open(path).await?)
        })
    }

    pub async fn write_stream(&self, file: File, stream: LogStream) -> Result<()> {
        FileLogStreamWriter { log: &self.log, stream }.write(file).await
    }
}

struct FileLogStreamWriter<'a> {
    log: &'a Mutex<RecordWriter>,
    stream: LogStream,
}

impl<'a> FileLogStreamWriter<'a> {
    async fn write(&self, mut file: File) -> Result<()> {
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
    
            if buffer_size == buffer.len() && last_line_end == 0 {
                // In this case, the line is longer that our buffer size so we'll just write a
                // non-terminated line consisting on the entire buffer.
                let line = &buffer;
                last_line_end = buffer_size;
    
                self.write_line(line, time, false).await?;
            }
    
            let remaining_size = buffer_size - last_line_end;
            let remaining_start = buffer_size - remaining_size;
            for i in 0..remaining_size {
                buffer[i] = buffer[remaining_start + i];
            }
    
            buffer_size = remaining_size;
        }
    
        // Always write a final entry with any remaining data to mark that the stream has now been
        // closed.
        let time = SystemTime::now();    
        let line = &buffer[0..buffer_size];
        self.write_line(line, time, true).await?;
    
        Ok(())
    }

    async fn write_line(&self, line: &[u8], time: SystemTime, end_stream: bool) -> Result<()> {
        let mut log_entry = LogEntry::default();
        log_entry.set_stream(self.stream);
        log_entry.set_timestamp(time);
        log_entry.set_end_stream(end_stream);
        log_entry.value_mut().extend_from_slice(line);

        let mut log = self.log.lock().await;
        log.append(&log_entry.serialize()?).await?;
        Ok(())
    } 

}
