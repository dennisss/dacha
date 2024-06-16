use std::collections::VecDeque;
use std::time::Instant;

use base_error::*;
use cnc_monitor_proto::cnc::ReadSerialLogResponse_LineKind;
use common::bytes::Bytes;
use executor::lock;
use executor::sync::AsyncMutex;

/// Maximum allowed size of one line being
const MAX_LINE_LENGTH: usize = 256;

/// Number of previous lines read from the serial port that we will retain.
///
/// Must be >> READ_BUFFER_SIZE to ensure that no new lines are not truncated
/// before that are parsed in case the read returns many short lines.
const READ_HISTORY_SIZE: usize = 2048; // ~512KiB

/// Buffer for storing all lines received from the remote machine.
///
/// TODO: Optimize this to use one big cyclic buffer instead of separate buffers
/// per line.
#[derive(Default)]
pub struct SerialReceiverBuffer {
    state: AsyncMutex<SerialReceiverBufferState>,
}

#[derive(Clone, Debug)]
pub struct ReceivedLine {
    pub data: Bytes,
    pub time: Instant,
    pub kind: ReadSerialLogResponse_LineKind,
}

#[derive(Default)]
struct SerialReceiverBufferState {
    /// History of received lines.
    ///
    /// TODO: Preserve the history across failures so that we can read out
    /// problematic responses.
    lines: VecDeque<ReceivedLine>,

    /// Absolute index of lines[0] where a value of 0 is only given to the first
    /// line ever received.
    first_line_index: u64,

    /// Latest incomplete line which hasn't received a "\n" yet.
    current_line: Vec<u8>,
}

impl SerialReceiverBuffer {
    pub async fn first_line_offset(&self) -> Result<u64> {
        lock!(state <= self.state.lock().await?, {
            Ok(state.first_line_index)
        })
    }

    pub async fn last_line_offset(&self) -> Result<u64> {
        lock!(state <= self.state.lock().await?, {
            Ok(state.first_line_index + (state.lines.len() as u64))
        })
    }

    pub async fn get_line(&self, offset: u64) -> Result<ReceivedLine> {
        lock!(state <= self.state.lock().await?, {
            if offset < state.first_line_index {
                return Err(err_msg("Read faster than could process lines"));
            }

            let i = (offset - state.first_line_index) as usize;

            Ok(state.lines[i].clone())
        })
    }

    pub async fn set_kind(&self, offset: u64, kind: ReadSerialLogResponse_LineKind) -> Result<()> {
        lock!(state <= self.state.lock().await?, {
            if offset < state.first_line_index {
                return Err(err_msg("Read faster than could process lines"));
            }

            let i = (offset - state.first_line_index) as usize;

            state.lines[i].kind = kind;

            Ok(())
        })
    }

    pub async fn append(&self, buf: &[u8], now: Instant) -> Result<()> {
        lock!(state <= self.state.lock().await?, {
            // Append the read data to the current line.
            let mut i = 0;
            let n = buf.len();
            while i < n {
                // TODO: Want to accept any of "\r\n", "\n", or "\r" (immediately emiting a line
                // when the first character is seen).
                let nl_index = buf[i..n].iter().position(|c| *c == b'\n').map(|j| i + j);

                let j = nl_index.unwrap_or(n);
                state.current_line.extend_from_slice(&buf[i..j]);

                if state.current_line.len() > MAX_LINE_LENGTH {
                    // TODO: Allow skipping in case this is due to wire corruption.
                    return Err(err_msg("Very long line received"));
                }

                if nl_index.is_some() {
                    let line = state.current_line.split_off(0).into();
                    state.lines.push_back(ReceivedLine {
                        data: line,
                        time: now,
                        kind: ReadSerialLogResponse_LineKind::UNKNOWN,
                    });

                    // Skip the new line character.
                    i = j + 1;
                } else {
                    i = j;
                }
            }

            while state.lines.len() > READ_HISTORY_SIZE {
                state.lines.pop_front();
                state.first_line_index += 1;
            }

            Ok(())
        })
    }
}
