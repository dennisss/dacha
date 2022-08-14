use core::convert::AsRef;

use common::{fixed::vec::FixedVec, segmented_buffer::SegmentedBuffer};
use executor::channel::Channel;
use executor::mutex::Mutex;
use protobuf::Message;

use nordic_proto::proto::log::*;

const LOG_BUFFER_SIZE: usize = 256;

static LOGGER: Logger = Logger::new();

pub struct Logger {
    // entries_written: Channel<()>,
    state: Mutex<LoggerState>,
}

struct LoggerState {
    entries: SegmentedBuffer<[u8; LOG_BUFFER_SIZE]>,
    next_index: u32,
}

impl Logger {
    pub fn global() -> &'static Self {
        &LOGGER
    }

    const fn new() -> Self {
        Self {
            // entries_written: Channel::new(),
            state: Mutex::new(LoggerState {
                entries: SegmentedBuffer::new([0u8; LOG_BUFFER_SIZE]),
                next_index: 0,
            }),
        }
    }

    pub async fn write(&self, mut entry: LogEntry) {
        let mut state = self.state.lock().await;

        entry.set_index(state.next_index);
        state.next_index += 1;

        let mut buffer = FixedVec::<u8, 128>::new();
        entry.serialize_to(&mut buffer).unwrap();

        // let is_first = state.entries.is_empty();

        state.entries.write(buffer.as_ref());
        drop(state);

        // if is_first {
        //     let _ = self.entries_written.try_send(()).await;
        // }
    }

    /// Returns the number of bytes read or None if the buffer is empty or the
    /// next message doesn't fit in the given buffer.
    pub async fn try_read(&self, out: &mut [u8]) -> Option<usize> {
        let mut state = self.state.lock().await;

        if let Some(len) = state.entries.peek() {
            if len > out.len() {
                return None;
            }
        } else {
            return None;
        }

        state.entries.read(out)
    }
}

/// NOTE: A single call to log! records a single atomic message. So you likely
/// don't need to add new-line suffixes to mesages.
#[macro_export]
macro_rules! log {
    ($($s:expr),*) => {{
        use $crate::nordic_proto::proto::log::*;
        use $crate::logger::*;

        let mut entry = LogEntry::default();

        $(
        $s.log_text(&mut entry);
        )*

        Logger::global().write(entry).await;
    }};
}

pub trait LoggableValue {
    fn log_text(&self, entry: &mut LogEntry);
}

impl LoggableValue for &str {
    fn log_text(&self, entry: &mut LogEntry) {
        entry.text_mut().push_str(self);
    }
}

impl LoggableValue for u32 {
    fn log_text(&self, entry: &mut LogEntry) {
        let s = num_to_slice(*self);
        entry.text_mut().push_str(s.as_ref());
    }
}

impl LoggableValue for usize {
    fn log_text(&self, entry: &mut LogEntry) {
        (*self as u32).log_text(entry)
    }
}

pub struct NumberSlice {
    buf: [u8; 10],
    len: usize,
}

impl AsRef<[u8]> for NumberSlice {
    fn as_ref(&self) -> &[u8] {
        &self.buf[(self.buf.len() - self.len)..]
    }
}

impl AsRef<str> for NumberSlice {
    fn as_ref(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(self.as_ref()) }
    }
}

pub fn num_to_slice(mut num: u32) -> NumberSlice {
    // A u32 has a maximum length of 10 base-10 digits
    let mut buf: [u8; 10] = [0; 10];
    let mut num_digits = 0;
    while num > 0 {
        // TODO: perform this as one operation?
        let r = (num % 10) as u8;
        num /= 10;

        num_digits += 1;

        buf[buf.len() - num_digits] = ('0' as u8) + r;
    }

    if num_digits == 0 {
        num_digits = 1;
        buf[buf.len() - 1] = '0' as u8;
    }

    NumberSlice {
        buf,
        len: num_digits,
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn works() {
        common::async_std::task::block_on(async move {
            log!(0u32);

            let mut buf = [0u8; 256];
            let n = Logger::global().try_read(&mut buf).await;
            assert_eq!(n, Some(2));
        })
    }

}
