///
///
/// Data format:
/// - Byte 0: Modifiers pressed. Each bit corresponds to:
///   - 0 LEFT CTRL
///   - 1 LEFT SHIFT
///   - 2 LEFT ALT
///   - 3 LEFT GUI
///   - 4 RIGHT CTRL
///   - 5 RIGHT SHIFT
///   - 6 RIGHT ALT
///   - 7 RIGHT GUI
/// - Byte 1: Reserved
/// - Byte 2..7: Indices of the pressed keys in the array defined in the input
///   descriptor. The standard report descriptor directly maps these indices to
///   usages 0-101 of the Keyboard page. Unpressed keys are assigned an index of
///   0 (which the host would interpret as a usage of 0).
#[derive(Default)]
pub struct StandardKeyboardInputReport {
    data: [u8; 8],
    num_keys: usize,
}

impl StandardKeyboardInputReport {
    pub fn add_pressed_key(&mut self, usage: KeyCodeUsage) {
        let id = usage.to_value();
        if id >= 0xE0 && id <= 0xE7 {
            let bit = id - 0xE0;
            self.data[0] |= 1 << bit;
            return;
        }

        // TODO: Verify within max usage min-max range (0, 101).

        if self.num_keys < 6 {
            self.data[2 + self.num_keys] = id;
        }

        self.num_keys += 1;
    }
}

impl AsRef<[u8]> for StandardKeyboardInputReport {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

enum_def_with_unknown!(KeyCodeUsage u8 =>
    Reserved = 0x00,
    ErrorRollOver = 0x01,
    POSTFail = 0x02,
    ErrorUndefined = 0x03,
    KeyboardA = 0x04, // and 'a'
    KeyboardB = 0x05, // and 'b'
    KeyboardC = 0x06, // and 'c'
    KeyboardD = 0x07, // and 'd'
    KeyboardE = 0x08, // and 'e'
    KeyboardF = 0x09, // and 'f'
    KeyboardG = 0x0A, // and 'g'
    KeyboardH = 0x0B, // and 'h'
    KeyboardI = 0x0C, // and 'i'
    KeyboardJ = 0x0D, // and 'j'
    KeyboardK = 0x0E, // and 'k'
    KeyboardL = 0x0F, // and 'l'
    KeyboardM = 0x10, // and 'm'
    KeyboardN = 0x11, // and 'n'
    KeyboardO = 0x12, // and 'o'
    KeyboardP = 0x13, // and 'p'
    KeyboardQ = 0x14, // and 'q'
    KeyboardR = 0x15, // and 'r'
    KeyboardS = 0x16, // and 's'
    KeyboardT = 0x17, // and 't'
    KeyboardU = 0x18, // and 'u'
    KeyboardV = 0x19, // and 'v'
    KeyboardW = 0x1A, // and 'w'
    KeyboardX = 0x1B, // and 'x'
    KeyboardY = 0x1C, // and 'y'
    KeyboardZ = 0x1D, // and 'z'

    Keyboard1 = 0x1E, // and !
    Keyboard2 = 0x1F, // and @
    Keyboard3 = 0x20, // and #
    Keyboard4 = 0x21, // and $
    Keyboard5 = 0x22, // and %
    Keyboard6 = 0x23, // and ^
    Keyboard7 = 0x24, // and &
    Keyboard8 = 0x25, // and *
    Keyboard9 = 0x26, // and (
    Keyboard0 = 0x27, // and )
    KeyboardReturn = 0x28, // (Enter)
    KeyboardEscape = 0x29,
    KeyboardBackspace = 0x2A,
    KeyboardTab = 0x2B,
    KeyboardSpacebar = 0x2C,
    KeyboardMinus = 0x2D, // - and (underscore)
    KeyboardEqual = 0x2E, // = and +
    KeyboardLeftBracket = 0x2F, // [ and {
    KeyboardRightBracket = 0x30, // ] and }
    KeyboardBackSlash = 0x31, // \ and |
    // Keyboard Non-US # and ˜ = 0x32,
    KeyboardColon = 0x33, //  ; and :
    KeyboardQuote = 0x34, // ‘ and “
    KeyboardTilde = 0x35, // Grave Accent and Tilde
    KeyboardComma = 0x36, // , and <
    KeyboardPeriod = 0x37, // . and >
    KeyboardForwardSlash = 0x38, // / and ?
    KeyboardCapsLock = 0x39,
    KeyboardF1 = 0x3A,
    KeyboardF2 = 0x3B,
    KeyboardF3 = 0x3C,
    KeyboardF4 = 0x3D,
    KeyboardF5 = 0x3E,
    KeyboardF6 = 0x3F,
    KeyboardF7 = 0x40,
    KeyboardF8 = 0x41,
    KeyboardF9 = 0x42,
    KeyboardF10 = 0x43,
    KeyboardF11 = 0x44,
    KeyboardF12 = 0x45,
    KeyboardPrintScreen = 0x46,
    KeyboardScrollLock = 0x47,
    KeyboardPause = 0x48,
    KeyboardInsert = 0x49,
    KeyboardHome = 0x4A,
    KeyboardPageUp = 0x4B,
    KeyboardDeleteForward = 0x4C,
    KeyboardEnd = 0x4D,
    KeyboardPageDown = 0x4E,
    KeyboardRightArrow = 0x4F,
    KeyboardLeftArrow = 0x50,
    KeyboardDownArrow = 0x51,
    KeyboardUpArrow = 0x52,
    // Keypad Num Lock and Clear = 0x53,
    // Keypad / = 0x54,
    // Keypad * = 0x55,
    // Keypad - = 0x56,
    // Keypad + = 0x57,
    // Keypad ENTER = 0x58,
    // Keypad 1 and End = 0x59,
    // Keypad 2 and Down Arrow = 0x5A,
    // Keypad 3 and PageDn = 0x5B,
    // Keypad 4 and Left Arrow = 0x5C,
    // Keypad 5 = 0x5D,
    // Keypad 6 and Right Arrow = 0x5E,
    // Keypad 7 and Home = 0x5F,
    // Keypad 8 and Up Arrow = 0x60,
    // Keypad 9 and PageUp = 0x61,
    // Keypad 0 and Insert = 0x62,
    // Keypad . and Delete = 0x63,
    // Keyboard Non-US \and | = 0x64,
    // Keyboard Application = 0x65,
    // Keyboard Power = 0x66,
    // Keypad = = 0x67,
    KeyboardF13 = 0x68,
    KeyboardF14 = 0x69,
    KeyboardF15 = 0x6A,
    KeyboardF16 = 0x6B,
    KeyboardF17 = 0x6C,
    KeyboardF18 = 0x6D,
    KeyboardF19 = 0x6E,
    KeyboardF20 = 0x6F,
    KeyboardF21 = 0x70,
    KeyboardF22 = 0x71,
    KeyboardF23 = 0x72,
    KeyboardF24 = 0x73,
    KeyboardExecute = 0x74,
    KeyboardHelp = 0x75,
    KeyboardMenu = 0x76,
    KeyboardSelect = 0x77,
    KeyboardStop = 0x78,
    KeyboardAgain = 0x79,
    KeyboardUndo = 0x7A,
    KeyboardCut = 0x7B,
    KeyboardCopy = 0x7C,
    KeyboardPaste = 0x7D,
    KeyboardFind = 0x7E,
    KeyboardMute = 0x7F,
    // Keyboard Volume Up = 0x80,
    // Keyboard Volume Down = 0x81,
    // 12 Keyboard Locking Caps Lock = 0x82,
    // Keyboard Locking Num Lock = 0x83,
    // Keyboard Locking Scroll Lock = 0x84,
    // Keypad Comma = 0x85,
    // Keypad Equal Sign = 0x86,
    // Keyboard International1 = 0x87,
    // Keyboard International2 = 0x88,
    // Keyboard International3 = 0x89,
    // Keyboard International4 = 0x8A,
    KeyboardInternational5 = 0x8B,
    KeyboardInternational6 = 0x8C,
    KeyboardInternational7 = 0x8D,
    KeyboardInternational8 = 0x8E,
    KeyboardInternational9 = 0x8F,
    KeyboardLANG1 = 0x90,
    KeyboardLANG2 = 0x91,
    KeyboardLANG3 = 0x92,
    KeyboardLANG4 = 0x93,
    KeyboardLANG5 = 0x94,
    KeyboardLANG6 = 0x95,
    KeyboardLANG7 = 0x96,
    KeyboardLANG8 = 0x97,
    KeyboardLANG9 = 0x98,
    // Keyboard Alternate Erase = 0x99,
    // Keyboard SysReq/Attention = 0x9A,
    // Keyboard Cancel = 0x9B,
    // Keyboard Clear = 0x9C,
    // Keyboard Prior = 0x9D,
    // Keyboard Return = 0x9E,
    // Keyboard Separator = 0x9F,
    // Keyboard Out = 0xA0,
    // Keyboard Oper = 0xA1,
    // Keyboard Clear/Again = 0xA2,
    // Keyboard CrSel/Props = 0xA3,
    // Keyboard ExSel = 0xA4,
    // Keypad 00 = 0xB0,
    // Keypad 000 = 0xB1,
    // Thousands Separator = 0xB2,
    // Decimal Separator = 0xB3,
    // Currency Unit = 0xB4,
    // Currency Sub-unit = 0xB5,
    // Keypad ( = 0xB6,
    // Keypad ) = 0xB7,
    // Keypad { = 0xB8,
    // Keypad } = 0xB9,
    // Keypad Tab = 0xBA,
    // Keypad Backspace = 0xBB,
    KeypadA = 0xBC,
    KeypadB = 0xBD,
    KeypadC = 0xBE,
    KeypadD = 0xBF,
    KeypadE = 0xC0,
    KeypadF = 0xC1,
    KeypadXOR = 0xC2,
    // Keypad = 0xC3,
    // Keypad % = 0xC4,
    // Keypad < = 0xC5,
    // Keypad > = 0xC6,
    // Keypad & = 0xC7,
    // Keypad && = 0xC8,
    // Keypad | = 0xC9,
    // Keypad || = 0xCA,
    // Keypad : = 0xCB,
    // Keypad # = 0xCC,
    // Keypad Space = 0xCD,
    // Keypad @ = 0xCE,
    // Keypad ! = 0xCF,
    // Keypad Memory Store = 0xD0,
    // Keypad Memory Recall = 0xD1,
    // Keypad Memory Clear = 0xD2,
    // Keypad Memory Add = 0xD3,
    // Keypad Memory Subtract = 0xD4,
    // Keypad Memory Multiply = 0xD5,
    // Keypad Memory Divide = 0xD6,
    // Keypad +/- = 0xD7,
    // Keypad Clear = 0xD8,
    // Keypad Clear Entry = 0xD9,
    // Keypad Binary = 0xDA,
    // Keypad Octal = 0xDB,
    // Keypad Decimal = 0xDC,
    // Keypad Hexadecimal = 0xDD,
    KeyboardLeftControl = 0xE0,
    KeyboardLeftShift = 0xE1,
    KeyboardLeftAlt = 0xE2,
    KeyboardLeftGUI = 0xE3,
    KeyboardRightControl = 0xE4,
    KeyboardRightShift = 0xE5,
    KeyboardRightAlt = 0xE6,
    KeyboardRightGUI = 0xE7
);
