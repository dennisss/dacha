use common::segmented_buffer::SegmentedBuffer;

const BUFFER_SIZE: usize = 128;

const LENGTH_OFFSET: usize = 0;

const REMOTE_ADDRESS_OFFSET: usize = 1;
const REMOTE_ADDRESS_SIZE: usize = 4;

const COUNTER_OFFSET: usize = 5;
const COUNTER_SIZE: usize = 4;

const DATA_OFFSET: usize = 9;

const TAG_SIZE: usize = 4;

/// Buffer for storing packets that will be sent over the air.
///
/// In memory a packet is structured as:
/// - Byte 0:   LENGTH: Length of all future bytes
/// - Byte [1, 5): ADDRESS
/// - Byte [5, 9): COUNTER
/// - Byte [9..(9+N)): DATA:
/// - Byte [(9+N)..(9+N+4)): MIC
pub struct PacketBuffer {
    buf: [u8; BUFFER_SIZE],
}

impl PacketBuffer {
    pub fn new() -> Self {
        let mut buf = [0u8; BUFFER_SIZE];
        // Minimum packet length with zero data.
        buf[0] = ((DATA_OFFSET - LENGTH_OFFSET) + TAG_SIZE) as u8;

        Self { buf }
    }

    pub fn remote_address(&self) -> &[u8; REMOTE_ADDRESS_SIZE] {
        array_ref![self.buf, REMOTE_ADDRESS_OFFSET, REMOTE_ADDRESS_SIZE]
    }

    pub fn remote_address_mut(&mut self) -> &mut [u8] {
        array_mut_ref![self.buf, REMOTE_ADDRESS_OFFSET, REMOTE_ADDRESS_SIZE]
    }

    pub fn counter(&self) -> u32 {
        u32::from_le_bytes(*array_ref![self.buf, COUNTER_OFFSET, COUNTER_SIZE])
    }

    pub fn set_counter(&mut self, value: u32) {
        *array_mut_ref![self.buf, COUNTER_OFFSET, COUNTER_SIZE] = value.to_le_bytes();
    }

    pub fn data_len(&self) -> usize {
        (self.buf[0] as usize) - (DATA_OFFSET - LENGTH_OFFSET) - TAG_SIZE
    }

    pub fn resize_data(&mut self, new_length: usize) {
        self.buf[0] = (new_length + (DATA_OFFSET - LENGTH_OFFSET) + TAG_SIZE) as u8;
    }

    pub fn max_data_len(&self) -> usize {
        self.buf.len() - DATA_OFFSET - TAG_SIZE
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
        &self.buf[0..(1 + self.buf[0] as usize)]
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
