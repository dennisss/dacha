use common::errors::*;

pub const STATUS_SIZE: usize = core::mem::size_of::<Status>();

#[derive(Debug)]
#[repr(C)]
pub struct Status {
    /// Always 0x80
    print_head_mark: u8,

    /// Size of this struct. Always 0x20
    size: u8,

    /// Always "B" (0x42)
    brother_code: u8,

    /// Always "0" (0x30)
    series_code: u8,

    pub model_code: ModelCode,

    /// Always "0" (0x30)
    country_code: u8,

    /// Always 0x00
    reserved1: u8,

    /// Always 0x00
    reserved2: u8,

    pub error_info1: ErrorInfo1,

    pub error_info2: ErrorInfo2,

    /// Width of the inserted media in millimeters.
    pub media_width: u8,

    pub media_type: MediaType,

    /// Always 0x00
    num_colors: u8,

    /// Always 0x00
    fonts: u8,

    /// Always 0x00
    japenese_fonts: u8,

    pub mode: u8,

    /// Always 0x00
    density: u8,

    /// Always 0x00
    media_length: u8,

    pub status_type: StatusType,

    pub phase_type: PhaseType,

    /// TODO: Interpret these.
    pub phase_number_high: u8,
    pub phase_number_low: u8,

    pub notification_number: u8,

    /// Always 0x00.
    expansion_area: u8,

    pub tape_color: TapeColor,

    pub text_color: TextColor,

    hardware_settings: [u8; 4],

    /// Always 0x00
    reserved3: u8,

    /// Always 0x00
    reserved4: u8,
}

impl Status {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() != STATUS_SIZE {
            return Err(err_msg("Status buffer is the wrong size"));
        }

        let mut inst = Self {
            print_head_mark: 0,
            size: 0,
            brother_code: 0,
            series_code: 0,
            model_code: 0.into(),
            country_code: 0,
            reserved1: 0,
            reserved2: 0,
            error_info1: 0.into(),
            error_info2: 0.into(),
            media_width: 0,
            media_type: 0.into(),
            num_colors: 0,
            fonts: 0,
            japenese_fonts: 0,
            mode: 0,
            density: 0,
            media_length: 0,
            status_type: 0.into(),
            phase_type: 0.into(),
            phase_number_high: 0,
            phase_number_low: 0,
            notification_number: 0,
            expansion_area: 0,
            tape_color: 0.into(),
            text_color: 0.into(),
            hardware_settings: [0u8; 4],
            reserved3: 0,
            reserved4: 0,
        };
        unsafe { common::struct_bytes::struct_bytes_mut(&mut inst).copy_from_slice(data) };

        if inst.print_head_mark != 0x80
            || inst.size != (STATUS_SIZE as u8)
            || inst.brother_code != 0x42
            || inst.series_code != 0x30
            || inst.country_code != 0x30
            || inst.reserved1 != 0
            || inst.reserved2 != 0
            || inst.num_colors != 0
            || inst.fonts != 0
            || inst.japenese_fonts != 0
            || inst.density != 0
            || inst.media_length != 0
            || inst.expansion_area != 0
            || inst.reserved3 != 0
            || inst.reserved4 != 0
        {
            println!("BAD STATUS: {:#?}", inst);
            return Err(err_msg("Fixed or reserved status bytes have wrong values"));
        }

        Ok(inst)
    }

    pub fn check_for_errors(&self) -> Result<()> {
        if self.error_info1 != 0.into() {
            return Err(format_err!(
                "Can't print due to errors: {:?}",
                self.error_info1
            ));
        }

        if self.error_info2 != 0.into() {
            return Err(format_err!(
                "Can't print due to errors: {:?}",
                self.error_info1
            ));
        }

        if self.status_type == StatusType::ErrorOccured {
            return Err(err_msg(
                "An error occured but it wasn't recorded in the status",
            ));
        }

        Ok(())
    }

    /// Tests if based on this current status if we are able to begin sending
    /// print data.
    pub fn check_can_start_printing(&self) -> Result<()> {
        self.check_for_errors()?;

        if self.media_type == MediaType::NO_MEDIA || self.media_type == MediaType::INCOMPATIBLE_TAPE
        {
            return Err(err_msg("No or incompatible tape media inserted"));
        }

        if self.phase_type != PhaseType::EditingStatus {
            return Err(err_msg(
                "Printer not currently in a state which can receive commands",
            ));
        }

        Ok(())
    }
}

define_transparent_enum!(ModelCode u8 =>
    PT_H500 = 0x64,
    PT_E500 = 0x65,
    PT_E550W = 0x66,
    PT_P700 = 0x67,
    PT_P750W = 0x68
);

define_bit_flags!(ErrorInfo1 u8 {
    NO_MEDIA = 1 << 0,
    CUTTER_JAM = 1 << 2,
    WEAK_BATTERIES = 1 << 3,
    HIGH_VOLTAGE_ADAPTER = 1 << 6
});

define_bit_flags!(ErrorInfo2 u8 {
    REPLACE_MEDIA = 1 << 0,
    COVER_OPEN = 1 << 4,
    OVERHEATING = 1 << 5
});

define_transparent_enum!(MediaType u8 =>
    NO_MEDIA = 0,
    LAMINATED_TAPE = 0x01,
    NON_LAMINATED_TAPE = 0x03,
    HEAT_SHRINK_TUBE_2_TO_1 = 0x11,
    HEAT_SHRINK_TUBE_3_TO_1 = 0x17,
    INCOMPATIBLE_TAPE = 0xFF
);

define_transparent_enum!(StatusType u8 =>
    ReplyToStatusRequest = 0x00,
    PrintingComplete = 0x01,
    ErrorOccured = 0x02,
    ExitIFMode = 0x03,
    TurnedOff = 0x04,
    Notification = 0x05,
    PhaseChange = 0x06
);

define_transparent_enum!(PhaseType u8 =>
    // In this state, the printer may receive additional packets
    EditingStatus = 0x00,

    PrintingState = 0x01
);

define_transparent_enum!(TapeColor u8 =>
    White = 0x01,
    Other = 0x02,
    Clear = 0x03,
    Red = 0x04,
    Blue = 0x05,
    Yellow = 0x06,
    Green = 0x07,
    Black = 0x08,
    ClearWhiteText = 0x09,
    MatteWhite = 0x20,
    MatteClear = 0x21,
    MatteSilver = 0x22,
    SatinGold = 0x23,
    SatinSilver = 0x24,
    BlueD = 0x30,
    RedD = 0x31,
    FluorescentOrange = 0x40,
    FluorescentYellow = 0x41,
    BerryPinkS = 0x50,
    LightGrayS = 0x51,
    LimeGreenS = 0x52,
    YellowF = 0x60,
    PinkF = 0x61,
    BlueF = 0x62,
    WhiteHeatShrinkTube = 0x70,
    WhiteFlexID = 0x90
);

define_transparent_enum!(TextColor u8 =>
    White = 0x01,
    Red = 0x04,
    Blue = 0x05,
    Black = 0x08,
    Gold = 0x0A,
    BlueF = 0x62,
    Clearning = 0xF0,
    Stencil = 0xF1,
    Other = 0x02,
    Incompatible = 0xFF
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_size_test() {
        assert_eq!(STATUS_SIZE, 32);
    }
}
