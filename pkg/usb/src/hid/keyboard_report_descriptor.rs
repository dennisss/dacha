use alloc::vec::Vec;

use crate::hid::item::*;

/*
A Usage is a 32-bit number where:
    Top 16-bits are the page
    Lower 16-bits are the usage id.

*/

enum_def_with_unknown!(UsagePage u32 =>
    Undefined = 0x00,
    GenericDesktop = 0x01,
    KeyCode = 0x07,
    LED = 0x08
);

enum_def_with_unknown!(GenericDesktopUsage u32 =>
    Keyboard = 0x06
);

/// Creates a standard HID report descriptor for a boot compatible keyboard.
/// (From HID 1.1 Spec: B.1 or E.6)
pub fn standard_keyboard_report_descriptor() -> Vec<u8> {
    let items = &[
        Item::Global {
            tag: GlobalItemTag::UsagePage,
            value: UsagePage::GenericDesktop.to_value(),
        },
        Item::Local {
            tag: LocalItemTag::Usage,
            value: GenericDesktopUsage::Keyboard.to_value(),
        },
        Item::BeginCollection {
            typ: CollectionItemType::Application,
        },
        // BEGIN Modifiers Input
        Item::Global {
            tag: GlobalItemTag::ReportSize,
            value: 1,
        },
        Item::Global {
            tag: GlobalItemTag::ReportCount,
            value: 8,
        },
        Item::Global {
            tag: GlobalItemTag::UsagePage,
            value: UsagePage::KeyCode.to_value(),
        },
        Item::Local {
            tag: LocalItemTag::UsageMin,
            value: 224,
        },
        Item::Local {
            tag: LocalItemTag::UsageMax,
            value: 231,
        },
        Item::Global {
            tag: GlobalItemTag::LogicalMin,
            value: 0,
        },
        Item::Global {
            tag: GlobalItemTag::LogicalMax,
            value: 1,
        },
        Item::Input(
            ValueFlags::empty()
                .set(ValueFlags::DATA)
                .set(ValueFlags::VARIABLE)
                .set(ValueFlags::ABSOLUTE),
        ),
        // END Modifiers Input

        // BEGIN Reserved Byte Input
        Item::Global {
            tag: GlobalItemTag::ReportCount,
            value: 1,
        },
        Item::Global {
            tag: GlobalItemTag::ReportSize,
            value: 8,
        },
        Item::Input(ValueFlags::empty().set(ValueFlags::CONSTANT)),
        // END ??? Input

        // BEGIN LED Output
        Item::Global {
            tag: GlobalItemTag::ReportCount,
            value: 5,
        },
        Item::Global {
            tag: GlobalItemTag::ReportSize,
            value: 1,
        },
        Item::Global {
            tag: GlobalItemTag::UsagePage,
            value: UsagePage::LED.to_value(),
        },
        Item::Local {
            tag: LocalItemTag::UsageMin,
            value: 1,
        },
        Item::Local {
            tag: LocalItemTag::UsageMax,
            value: 5,
        },
        Item::Output(
            ValueFlags::empty()
                .set(ValueFlags::DATA)
                .set(ValueFlags::VARIABLE)
                .set(ValueFlags::ABSOLUTE),
        ),
        // END LED Output

        // BEGIN LED Padding Output
        Item::Global {
            tag: GlobalItemTag::ReportCount,
            value: 1,
        },
        Item::Global {
            tag: GlobalItemTag::ReportSize,
            value: 3,
        },
        Item::Output(ValueFlags::empty().set(ValueFlags::CONSTANT)),
        // END ??? Output

        // BEGIN Key Codes Input
        Item::Global {
            tag: GlobalItemTag::ReportCount,
            value: 6,
        },
        Item::Global {
            tag: GlobalItemTag::ReportSize,
            value: 8,
        },
        Item::Global {
            tag: GlobalItemTag::LogicalMin,
            value: 0,
        },
        Item::Global {
            tag: GlobalItemTag::LogicalMax,
            value: 101,
        },
        Item::Global {
            tag: GlobalItemTag::UsagePage,
            value: UsagePage::KeyCode.to_value(),
        },
        Item::Local {
            tag: LocalItemTag::UsageMin,
            value: 0,
        },
        Item::Local {
            tag: LocalItemTag::UsageMax,
            value: 101,
        },
        Item::Input(
            ValueFlags::empty()
                .set(ValueFlags::DATA)
                .set(ValueFlags::ARRAY),
        ),
        // END Key Codes Input
        Item::EndCollection,
    ];

    let mut out = vec![];
    for item in items {
        serialize_item(item, &mut out);
    }

    out
}
