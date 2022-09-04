use usb::hid::KeyCodeUsage;

pub const KEY_ROWS: usize = 6;
pub const KEY_COLS: usize = 16;

pub const KEY_COLUMN_ORDER: &'static [usize] =
    &[7, 6, 5, 4, 3, 2, 1, 0, 8, 15, 14, 13, 12, 11, 10, 9];

// Ids are the numbers starting at 1 which correspond to the silk screen on the
// PCB.
pub fn key_id_to_usage(id: usize) -> Option<KeyCodeUsage> {
    use KeyCodeUsage::*;

    Some(match id {
        1 => KeyboardEscape,
        2 => KeyboardF1,
        3 => KeyboardF2,
        4 => KeyboardF3,
        5 => KeyboardF4,
        6 => KeyboardF5,
        7 => KeyboardF6,
        8 => KeyboardF7,
        9 => KeyboardF8,
        10 => KeyboardF9,
        11 => KeyboardF10,
        12 => KeyboardF11,
        13 => KeyboardF12,
        14 => KeyboardPrintScreen,
        15 => KeyboardScrollLock,
        16 => KeyboardPause,
        17 => KeyboardTilde,
        18 => Keyboard1,
        19 => Keyboard2,
        20 => Keyboard3,
        21 => Keyboard4,
        22 => Keyboard5,
        23 => Keyboard6,
        24 => Keyboard7,
        25 => Keyboard8,
        26 => Keyboard9,
        27 => Keyboard0,
        28 => KeyboardMinus,
        29 => KeyboardEqual,
        30 => KeyboardBackspace,
        31 => KeyboardInsert,
        32 => KeyboardHome,
        64 => KeyboardPageUp,
        33 => KeyboardTab,
        34 => KeyboardQ,
        35 => KeyboardW,
        36 => KeyboardE,
        37 => KeyboardR,
        38 => KeyboardT,
        39 => KeyboardY,
        40 => KeyboardU,
        41 => KeyboardI,
        42 => KeyboardO,
        43 => KeyboardP,
        44 => KeyboardLeftBracket,
        45 => KeyboardRightBracket,
        46 => KeyboardBackSlash,
        47 => KeyboardDeleteForward,
        48 => KeyboardEnd,
        80 => KeyboardPageDown,
        49 => KeyboardCapsLock,
        50 => KeyboardA,
        51 => KeyboardS,
        52 => KeyboardD,
        53 => KeyboardF,
        54 => KeyboardG,
        55 => KeyboardH,
        56 => KeyboardJ,
        57 => KeyboardK,
        58 => KeyboardL,
        59 => KeyboardColon,
        60 => KeyboardQuote,
        61 => KeyboardReturn,
        // 62 => Fn
        // 63 => Wireless
        65 => KeyboardLeftShift,
        66 => KeyboardZ,
        67 => KeyboardX,
        68 => KeyboardC,
        69 => KeyboardV,
        70 => KeyboardB,
        71 => KeyboardN,
        72 => KeyboardM,
        73 => KeyboardComma,
        74 => KeyboardPeriod,
        75 => KeyboardForwardSlash,
        76 => KeyboardRightShift,
        79 => KeyboardUpArrow,
        81 => KeyboardLeftControl,
        82 => KeyboardLeftGUI,
        83 => KeyboardLeftAlt,
        86 => KeyboardSpacebar,
        90 => KeyboardRightAlt,
        91 => KeyboardRightGUI,
        // 92 => Menu
        93 => KeyboardRightControl,
        94 => KeyboardLeftArrow,
        95 => KeyboardDownArrow,
        96 => KeyboardRightArrow,
        _ => {
            return None;
        }
    })
}
