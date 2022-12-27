use common::segmented_buffer::SegmentedBuffer;

use crate::constants::{RadioAddress, RADIO_ADDRESS_SIZE};

const BUFFER_SIZE: usize = 128;

const LENGTH_OFFSET: usize = 0;

const START_OF_PAYLOAD: usize = 1;

const REMOTE_ADDRESS_OFFSET: usize = 1;

const COUNTER_OFFSET: usize = 5;
const COUNTER_SIZE: usize = 4;

const DATA_OFFSET: usize = 9;

const TAG_SIZE: usize = 4;

/// Maximum number of bytes needed to store a packet in memory.
///
/// This is constrained by the NRF52 radio which limits the [S0, LENGTH, S1,
/// PAYLOAD] size to 258. Because we use a 1-byte length, the payload can only
/// be up to 255 bytes.
pub const MAX_PACKET_BUFFER_SIZE: usize = 256;

/// Inside a single packet, this is the maximum size of the data portion of that
/// packet (excluding routing and encryption overhead).
pub const MAX_PACKET_DATA_SIZE: usize = MAX_PACKET_BUFFER_SIZE - DATA_OFFSET - TAG_SIZE;

/// Buffer for storing packets that will be sent over the air.
///
/// In memory a packet is structured as:
/// - Byte 0:   LENGTH: Number of additional bytes used in this buffer.
/// - Payload:
///   - Byte [1, 5): REMOTE_ADDRESS
///   - Byte [5, 9): COUNTER
///   - Byte [9..(9+N)): DATA:
///   - Byte [(9+N)..(9+N+4)): MIC
pub struct PacketBuffer {
    buf: [u8; MAX_PACKET_BUFFER_SIZE],
}

impl PacketBuffer {
    /// Creates a new empty packet buffer.
    pub fn new() -> Self {
        let mut buf = [0u8; MAX_PACKET_BUFFER_SIZE];
        // Minimum packet length with zero data.
        buf[0] = ((DATA_OFFSET - START_OF_PAYLOAD) + TAG_SIZE) as u8;

        Self { buf }
    }

    pub fn remote_address(&self) -> &RadioAddress {
        array_ref![self.buf, REMOTE_ADDRESS_OFFSET, RADIO_ADDRESS_SIZE]
    }

    pub fn remote_address_mut(&mut self) -> &mut RadioAddress {
        array_mut_ref![self.buf, REMOTE_ADDRESS_OFFSET, RADIO_ADDRESS_SIZE]
    }

    pub fn counter(&self) -> u32 {
        u32::from_le_bytes(*array_ref![self.buf, COUNTER_OFFSET, COUNTER_SIZE])
    }

    pub fn set_counter(&mut self, value: u32) {
        *array_mut_ref![self.buf, COUNTER_OFFSET, COUNTER_SIZE] = value.to_le_bytes();
    }

    pub fn data_len(&self) -> usize {
        (self.buf[0] as usize) - (DATA_OFFSET - START_OF_PAYLOAD) - TAG_SIZE
    }

    pub fn resize_data(&mut self, new_length: usize) {
        self.buf[0] = (new_length + (DATA_OFFSET - START_OF_PAYLOAD) + TAG_SIZE) as u8;
    }

    pub fn max_data_len(&self) -> usize {
        MAX_PACKET_DATA_SIZE
    }

    pub fn data(&self) -> &[u8] {
        // Does not include the MIC
        &self.buf[DATA_OFFSET..(DATA_OFFSET + self.data_len())]
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        let start = DATA_OFFSET;
        let end = DATA_OFFSET + self.data_len();
        &mut self.buf[start..end]
    }

    /// Gets the payload which is encrypted in the packet including the MIC as
    /// the last 4 bytes.
    pub fn ciphertext_mut(&mut self) -> &mut [u8] {
        let start = DATA_OFFSET;
        let end = (DATA_OFFSET + self.data_len() + TAG_SIZE);
        &mut self.buf[start..end]
    }

    /// TODO: Don't write the MIC
    pub fn write_to<T: AsRef<[u8]> + AsMut<[u8]>>(&self, buffer: &mut SegmentedBuffer<T>) {
        buffer.write(&self.buf[1..(1 + self.buf[0] as usize)]);
    }

    pub fn read_from<T: AsRef<[u8]> + AsMut<[u8]>>(
        &mut self,
        buffer: &mut SegmentedBuffer<T>,
    ) -> bool {
        if let Some(count) = buffer.read(&mut self.buf[1..]) {
            self.buf[0] = count as u8;
            true
        } else {
            false
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[0..(START_OF_PAYLOAD + self.buf[0] as usize)]
    }

    /// Gets the under internal un-truncated packet buffer.
    /// The first byte of this will be the length of the remaining data.
    pub fn raw(&self) -> &[u8] {
        &self.buf[..]
    }

    pub fn raw_mut(&mut self) -> &mut [u8] {
        &mut self.buf[..]
    }
}

/*

pub struct PacketRef<'a> {
    buf: &'a mut PacketBuffer
}

impl<'a> PacketRef<'a> {
    pub fn init_new(buffer: &'a mut PacketBuffer) -> Self {


    }

}
*/
