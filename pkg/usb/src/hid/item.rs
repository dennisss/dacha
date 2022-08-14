use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::collections::{HashMap, HashSet};

use common::async_std::task::current;
use common::errors::*;

use crate::descriptor_iter::Descriptor;
use crate::descriptors::{SetupPacket, StandardRequestType};
use crate::endpoint::is_in_endpoint;
use crate::linux::Device;

/*

5.3 Generic Item Format
- Used in Report descriptors

Byte 0:
    - 0-1: bSize
    - 2-3: bType
    - 4-7: bTag

Short data has 0, 1, 2 or 5 bytes

For long data
    (bSize == 2)

Byte 1:
    bDataSize
Byte 2:
    bLongItemTag
...


Item types and tags in 6.2.2.1


NOTE: There can be padding at the end by using a main tag without a usage.
*/

macro_rules! bit_flags {
    ($name:ident => $(bit $bit:expr ; $zero:ident (0) | $one:ident (1) ),*) => {
        #[derive(Clone, Copy)]
        pub struct $name {
            data: u32,
            mask: u32,
        }

        impl $name {
            $(
            pub const $zero: Self = Self { data: 0, mask: (1 << $bit) };
            pub const $one: Self = Self { data: (1 << $bit), mask: (1 << $bit) };
            )*

            pub fn empty() -> Self {
                Self::from(0)
            }

            pub fn from(value: u32) -> Self {
                Self { data: value, mask: 0xFFFFFFFF }
            }

            pub fn set(&self, other: Self) -> Self {
                Self {
                    data: (self.data & self.mask & !other.mask) | (other.data),
                    mask: self.mask | other.mask
                }
            }

            pub fn contains(&self, other: Self) -> bool {
                (self.data & other.mask) == other.data
            }

            pub fn to_raw_value(&self) -> u32 {
                self.data
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let mut set: Vec<String> = vec![];
                let mut remaining = self.data;
                $(
                    if self.contains(Self::$zero) {
                        set.push(stringify!($zero).to_string());
                    } else {
                        set.push(stringify!($one).to_string());
                    }

                    remaining &= !(1 << $bit);
                )*

                if remaining != 0 {
                    set.push(remaining.to_string());
                }

                write!(f, "{}", set.join(" | "))
            }
        }
    };
}

enum_def!(ShortItemCategory u8 =>
    Main = 0,
    Global = 1,
    Local = 2,
    Reserved = 3
);

enum_def_with_unknown!(MainItemTag u8 =>
    Input = 0b1000,
    Output = 0b1001,
    Feature = 0b1011,
    Collection = 0b1010,
    EndCollection = 0b1100
);

bit_flags!(ValueFlags =>
    // If data, then the value can be modified or change over time.
    bit 0 ; DATA (0) | CONSTANT (1),
    bit 1 ; ARRAY (0) | VARIABLE (1),
    bit 2 ; ABSOLUTE (0) | RELATIVE (1),
    bit 3 ; NO_WRAP (0) | WRAP (1),
    bit 4 ; LINEAR (0) | NON_LINEAR (1),
    bit 5 ; PREFFERED_STATE (0) | NO_PREFFERED (1),
    bit 6 ; NO_NULL_POS (0) | NULL_POS (1),
    // NOTE: This should always be zero on 'Input' items.
    bit 7 ; NON_VOLATIVE (0) | VOLATILE (1),
    bit 8 ; BIT_FIELD (0) | BUFFERED_BYTES (1)
);

enum_def_with_unknown!(CollectionItemType u8 =>
    Physical = 0x00,
    Application = 0x01,
    Logical = 0x02,
    Report = 0x03,
    NamedArray = 0x04,
    UsageSwitch = 0x05,
    UsageModifier = 0x06
);

enum_def_with_unknown!(GlobalItemTag u8 =>
    UsagePage = 0,
    LogicalMin = 1,
    LogicalMax = 2,
    PhysicalMin = 3,
    PhysicalMax = 4,
    UnitExponent = 5,
    Unit = 6,
    // Size of a single value in the report's data in bits.
    ReportSize = 7,
    ReportId = 8,
    // Maximum number of values of size ReportSize that are present in the report's data.
    ReportCount = 9,
    Push = 10,
    Pop = 11
);

enum_def_with_unknown!(LocalItemTag u8 =>
    Usage = 0,
    UsageMin = 1,
    UsageMax = 2,
    DesignatorIndex = 3,
    DesignatorMin = 4,
    DesignatorMax = 5,
    StringIndex = 6,
    StringMin = 7,
    StringMax = 8,
    Delimiter = 9
);

#[derive(Debug)]
pub enum Item {
    Input(ValueFlags),
    Output(ValueFlags),
    Feature(ValueFlags),

    BeginCollection {
        typ: CollectionItemType,
    },
    EndCollection,

    // Collection {
    //     typ: CollectionItemType,
    //     items: Vec<Item>,
    // },
    Global {
        tag: GlobalItemTag,
        value: u32,
    },
    Local {
        tag: LocalItemTag,
        value: u32,
    },

    Short {
        category: ShortItemCategory,
        tag: u8,
        value: u32,
    },
    Long {
        tag: u8,
        data: Vec<u8>,
    },
}

impl Item {
    // TODO: Remove me.
    pub fn visit_all<'a, F: 'a + FnMut(&Item)>(&self, f: &mut F) {
        f(self);
        // if let Item::Collection { items, .. } = self {
        //     for item in items {
        //         item.visit_all(f);
        //     }
        // }
    }
}

// Section 7.2.1
// TODO: Validate this must not be zero.
enum_def_with_unknown!(ReportType u8 =>
    Input = 1,
    Output = 2,
    Feature = 3
);

pub fn parse_items(mut input: &[u8]) -> Result<Vec<Item>> {
    // TODO: Check for out of bounds issues.

    let mut items = vec![];

    while !input.is_empty() {
        let prefix = input[0];
        if prefix == 0b11111110 {
            let size = input[1] as usize;
            let long_item_tag = input[2];
            let data = &input[3..(3 + size)];
            input = &input[(3 + size)..];

            items.push(Item::Long {
                tag: long_item_tag,
                data: data.to_vec(),
            });
        } else {
            // Number of bytes of data in this item after the prefix byte.
            let mut size = (prefix & 0b11) as usize;
            if size == 0b11 {
                size = 4;
            }

            let category = ShortItemCategory::from_value((prefix >> 2) & 0b11).unwrap();
            let tag = prefix >> 4;

            // TODO: Check that this is using the correct endian.
            let value = {
                let data = &input[1..(1 + size)];
                input = &input[(1 + size)..];

                let mut buf = [0u8; 4];
                buf[0..data.len()].copy_from_slice(&data);
                u32::from_le_bytes(buf)
            };

            // TODO: "If the bSize field equals 3, bits 16-31 of the 4-byte data portion of
            // the item are interpreted as a Usage page"
            // ^ But the good news is that usage page 0 is undefined so we should never run
            // into this situation.

            match category {
                ShortItemCategory::Main => {
                    let main_tag = MainItemTag::from_value(tag);
                    match main_tag {
                        MainItemTag::Input => {
                            let flags = ValueFlags::from(value);
                            items.push(Item::Input(flags));
                        }
                        MainItemTag::Output => {
                            let flags = ValueFlags::from(value);
                            items.push(Item::Output(flags));
                        }
                        MainItemTag::Feature => {
                            let flags = ValueFlags::from(value);
                            items.push(Item::Feature(flags));
                        }
                        MainItemTag::Collection => {
                            // TODO: Assert that there's up to 1 byte of data
                            let typ = CollectionItemType::from_value(value as u8);
                            items.push(Item::BeginCollection { typ })
                        }
                        MainItemTag::EndCollection => {
                            if size != 0 {
                                return Err(err_msg(
                                    "Expected no data for the End Collection item",
                                ));
                            }

                            items.push(Item::EndCollection);
                        }
                        MainItemTag::Unknown(tag) => {
                            items.push(Item::Short {
                                category,
                                tag,
                                value,
                            });
                        }
                    }
                }
                ShortItemCategory::Global => {
                    let tag = GlobalItemTag::from_value(tag);
                    items.push(Item::Global { tag, value });
                }
                ShortItemCategory::Local => {
                    let tag = LocalItemTag::from_value(tag);
                    items.push(Item::Local { tag, value });
                }
                ShortItemCategory::Reserved => {
                    items.push(Item::Short {
                        category,
                        tag,
                        value,
                    });
                }
            }
        }
    }

    // if collection_stack.len() != 0 {
    //     return Err(err_msg("Unclosed collections in data"));
    // }

    Ok(items)
}

pub fn serialize_item(item: &Item, out: &mut Vec<u8>) {
    let (category, tag, value) = match item {
        Item::Input(v) => (
            ShortItemCategory::Main.to_value(),
            MainItemTag::Input.to_value(),
            v.to_raw_value(),
        ),
        Item::Output(v) => (
            ShortItemCategory::Main.to_value(),
            MainItemTag::Output.to_value(),
            v.to_raw_value(),
        ),
        Item::Feature(v) => (
            ShortItemCategory::Main.to_value(),
            MainItemTag::Feature.to_value(),
            v.to_raw_value(),
        ),
        Item::BeginCollection { typ } => (
            ShortItemCategory::Main.to_value(),
            MainItemTag::Collection.to_value(),
            typ.to_value() as u32,
        ),
        Item::EndCollection => (
            ShortItemCategory::Main.to_value(),
            MainItemTag::EndCollection.to_value(),
            0,
        ),
        Item::Global { tag, value } => {
            (ShortItemCategory::Global.to_value(), tag.to_value(), *value)
        }
        Item::Local { tag, value } => (ShortItemCategory::Local.to_value(), tag.to_value(), *value),
        Item::Short {
            category,
            tag,
            value,
        } => (category.to_value(), *tag, *value),
        Item::Long { tag, data } => todo!(),
    };

    let value_bytes = value.to_le_bytes();
    let mut value_size = value_bytes.len();
    while value_size > 0 && value_bytes[value_size - 1] == 0 {
        value_size -= 1;
    }
    // NOTE: A size value of 0b10 is used to signify a Long item and a size of 3 is
    // not allowed.
    if value_size >= 2 {
        value_size = 4;
    }

    let prefix = {
        (if value_size == 4 {
            0b11
        } else {
            value_size as u8
        }) | category << 2
            | tag << 4
    };

    out.push(prefix);
    out.extend_from_slice(&value_bytes[0..value_size]);
}

// TODO: Use hashmaps.
#[derive(Clone, Debug)]
pub struct ItemStateTable {
    pub locals: HashMap<LocalItemTag, Vec<u32>>,
    pub globals: HashMap<GlobalItemTag, u32>,
}

// TODO: Find a better name for this.
#[derive(Clone, Debug)]
pub struct Report {
    pub state: ItemStateTable,
    pub var: ReportVariant,
}

#[derive(Clone, Debug)]
pub enum ReportVariant {
    Collection {
        typ: CollectionItemType,
        children: Vec<Report>,
    },
    Input(ValueFlags),
    Output(ValueFlags),
    Feature(ValueFlags),
}

pub fn parse_reports(items: &[Item]) -> Result<Vec<Report>> {
    let mut item_state_table_stack = vec![];

    let mut item_state_table = ItemStateTable {
        locals: HashMap::new(),
        globals: HashMap::new(),
    };

    let mut collection_stack = vec![];

    let mut reports = vec![];

    for item in items {
        match item {
            Item::BeginCollection { typ } => {
                collection_stack.push((reports, *typ, item_state_table.clone()));
                item_state_table.locals.clear();
                reports = vec![];
            }
            // TODO: Should I clear the locals on an EndCollection (given that it is technically a
            // main t)
            Item::EndCollection => {
                let (parent_reports, collection_typ, state) = collection_stack
                    .pop()
                    .ok_or_else(|| err_msg("Unexpected End Collection"))?;

                let collection = Report {
                    state,
                    var: ReportVariant::Collection {
                        typ: collection_typ,
                        children: reports,
                    },
                };
                reports = parent_reports;
                reports.push(collection);
            }

            Item::Input(flags) => {
                reports.push(Report {
                    state: item_state_table.clone(),
                    var: ReportVariant::Input(*flags),
                });
                item_state_table.locals.clear();
            }
            Item::Output(flags) => {
                reports.push(Report {
                    state: item_state_table.clone(),
                    var: ReportVariant::Output(*flags),
                });
                item_state_table.locals.clear();
            }
            Item::Feature(flags) => {
                reports.push(Report {
                    state: item_state_table.clone(),
                    var: ReportVariant::Feature(*flags),
                });
                item_state_table.locals.clear();
            }

            Item::Global { tag, value } => {
                if *tag == GlobalItemTag::Push {
                    item_state_table_stack.push(item_state_table.clone());
                } else if *tag == GlobalItemTag::Pop {
                    item_state_table = item_state_table_stack.pop().ok_or_else(|| {
                        err_msg("Attemping Pop, but item state table stack is empty")
                    })?;
                } else {
                    item_state_table.globals.insert(*tag, *value);
                }
            }
            Item::Local { tag, value } => {
                item_state_table
                    .locals
                    .entry(*tag)
                    .or_insert(vec![])
                    .push(*value);
            }
            Item::Short { .. } => todo!(),
            Item::Long { .. } => todo!(),
        };
    }

    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    /*
     05 0C 09 01 A1 01 85 01 15 00 25 01 75 01 95 20
     09 B5 09 B6 09 B7 09 CD 09 E0 09 E2 09 E3 09 E4
     09 E5 09 E9 09 EA 0A 52 01 0A 53 01 0A 54 01 0A
     55 01 0A 8A 01 0A 21 02 0A 23 02 0A 24 02 0A 25
     02 0A 26 02 0A 27 02 0A 2A 02 0A 92 01 0A 94 01
     0A 83 01 0A 02 02 0A 03 02 0A 07 02 0A 18 02 0A
     1A 02 09 B8 81 02 C0 05 01 09 80 A1 01 85 02 05
     01 19 81 29 83 15 00 25 01 95 03 75 01 81 06 95
     01 75 05 81 01 C0 05 01 09 06 A1 01 85 06 05 07
     75 08 95 01 81 03 15 00 25 01 19 04 29 3B 75 01
     95 38 81 02 19 3C 29 65 75 01 95 2A 81 02 19 85
     29 92 95 0E 81 02 C0

    003:072:000:DESCRIPTOR         1659901106.353962
     05 01 09 06 A1 01 05 07 19 E0 29 E7 15 00 25 01
     75 01 95 08 81 02 05 08 75 08 95 01 81 01 19 01
     29 05 75 01 95 05 91 02 75 03 95 01 91 01 05 07
     15 00 26 A4 00 19 00 2A A4 00 75 08 95 06 81 00
     05 0C 09 00 15 80 25 7F 75 08 95 40 B1 02 C0

    */

    #[test]
    fn parsing_test() {
        let data = common::hex::decode("050C0901A1018501150025017501952009B509B609B709CD09E009E209E309E409E509E909EA0A52010A53010A54010A55010A8A010A21020A23020A24020A25020A26020A27020A2A020A92010A94010A83010A02020A03020A07020A18020A1A0209B88102C005010980A101850205011981298315002501950375018106950175058101C005010906A10185060507750895018103150025011904293B750195388102193C29657501952A810219852992950E8102C0").unwrap();

        let items = parse_items(&data).unwrap();

        println!("{:#?}", items);
    }
}
