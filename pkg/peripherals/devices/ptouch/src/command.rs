use crate::status::*;

/// Buffer used to construct a sequence of commands that should be send to the
/// printer.
///
/// Use the methods on this struct to append commands and eventually use
/// as_ref() to get the data which can be transfered in one bulk transfer.
pub struct CommandBuffer {
    data: Vec<u8>,
}

impl CommandBuffer {
    pub fn new() -> Self {
        Self { data: vec![] }
    }

    pub fn as_ref(&self) -> &[u8] {
        &self.data
    }

    /// Resets the printer back to an uninitialized state.
    /// Send initialize() after this to cancel a print.
    pub fn invalidate(&mut self) -> &mut Self {
        self.data.resize(self.data.len() + 100, 0);
        self
    }

    /// Initializes mode settings or cancels printing
    pub fn initialize(&mut self) -> &mut Self {
        self.data.extend_from_slice(&[0x1B, 0x40]);
        self
    }

    /// NOTE: This must be used to switch to raster mode before sending raster
    /// data.
    pub fn set_command_mode(&mut self, command_mode: CommandMode) -> &mut Self {
        self.data.extend_from_slice(&[0x1B, 0x69, 0x61]);
        self.data.push(command_mode.value);
        self
    }

    /// Request to receive a 32 byte status report.
    pub fn request_status(&mut self) -> &mut Self {
        self.data.extend_from_slice(&[0x1b, 0x69, 0x53]);
        self
    }

    pub fn set_print_info(
        &mut self,
        media_type: Option<MediaType>,
        media_width: Option<u8>,
        media_length: Option<u8>,
        raster_number: usize,
        starting_page: bool,
    ) -> &mut Self {
        self.data.extend_from_slice(&[0x1B, 0x69, 0x7A]);

        let mut buf = [0u8; 10];

        let mut valid = ValidFlag::PI_RECOVERY | ValidFlag::PI_QUALITY;

        if let Some(v) = media_type {
            buf[1] = v.to_raw();
            valid = valid | ValidFlag::PI_KIND;
        }

        if let Some(v) = media_width {
            buf[2] = v;
            valid = valid | ValidFlag::PI_WIDTH;
        }

        if let Some(v) = media_length {
            buf[3] = v;
            valid = valid | ValidFlag::PI_LENGTH;
        }

        buf[4..8].copy_from_slice(&(raster_number as u32).to_le_bytes());

        buf[8] = if starting_page { 0 } else { 1 };

        buf[9] = 0;

        buf[0] = valid.to_raw();

        self.data.extend_from_slice(&buf);

        self
    }

    pub fn set_various_mode_settings(&mut self, value: VariousModeSettings) -> &mut Self {
        self.data
            .extend_from_slice(&[0x1B, 0x69, 0x4D, value.to_raw()]);
        self
    }

    pub fn set_advanced_mode_settings(&mut self, value: AdvancedModeSettings) -> &mut Self {
        self.data
            .extend_from_slice(&[0x1B, 0x69, 0x4B, value.to_raw()]);
        self
    }

    /// Sets the margin in dot units for the feed axis.
    ///
    /// This defines how much of the label is fed before starting to print and
    /// how much is fed after printing is done before cutting.
    pub fn set_feed_margin(&mut self, margin: u16) -> &mut Self {
        self.data.extend_from_slice(&[0x1B, 0x69, 0x64]);
        self.data.extend_from_slice(&margin.to_le_bytes());
        self
    }

    /// NOTE: Compression only available in raster mode.
    pub fn set_compression_mode(&mut self, mode: CompressionMode) -> &mut Self {
        self.data.extend_from_slice(&[0x4D, mode.to_raw()]);
        self
    }

    /// Sets a value from 1-99 which specifies how many pages should be printed
    /// before cutting in auto-cut mode.
    pub fn set_cut_interval(&mut self, num_pages: usize) -> &mut Self {
        self.data
            .extend_from_slice(&[0x1B, 0x69, 0x41, num_pages as u8]);
        self
    }

    /// Set whether or not status notifications are automaticall sent while the
    /// printer is printing. Defaults to on.
    pub fn set_auto_notify(&mut self, on: bool) -> &mut Self {
        self.data
            .extend_from_slice(&[0x1B, 0x69, 0x21, if on { 0 } else { 1 }]);
        self
    }

    pub fn raster_transfer(&mut self, data: &[u8]) -> &mut Self {
        // TODO: Some command references specify to use either b'g' or b'G' as the first
        // byte of the command here.
        self.data.push(0x47);
        self.data
            .extend_from_slice(&(data.len() as u16).to_le_bytes());
        self.data.extend_from_slice(data);
        self
    }

    /// Fills an entire raster line with zeros.
    pub fn raster_zero(&mut self) -> &mut Self {
        self.data.push(0x5A);
        self
    }

    /// Last command to send on pages other than the last page.
    pub fn print(&mut self) -> &mut Self {
        self.data.push(0x0C);
        self
    }

    /// Print command to use at the end of the last page.
    pub fn print_with_feeding(&mut self) -> &mut Self {
        self.data.push(0x1A);
        self
    }
}

define_transparent_enum!(CommandMode u8 {
    ESC_P = 0,
    RASTER_MODE = 1,
    TEMPLATE_MODE = 3,
    UNKNOWN_FF = 0xff
});

define_bit_flags!(VariousModeSettings u8 {
    AUTO_CUT = 1 << 6,
    MIRROR_PRINTING = 1 << 7
});

define_bit_flags!(AdvancedModeSettings u8 {
    NO_CHAIN_PRINTING = 1 << 3,
    SPECIAL_TAPE = 1 << 4,
    NO_BUFFER_CLEARING_WHEN_PRINTING = 1 << 7

});

define_bit_flags!(ValidFlag u8 {
    PI_KIND = 0x02,
    PI_WIDTH = 0x04,
    PI_LENGTH = 0x08,
    PI_QUALITY = 0x40,
    PI_RECOVERY = 0x80
});

define_transparent_enum!(CompressionMode u8 {
    NO_COMPRESSION = 0,
    TIFF = 2
});
