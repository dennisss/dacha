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
            data: u32
        }

        impl $name {
            pub fn from(value: u32) -> Self {
                Self { data: value }
            }

            $(
                pub fn $zero(&self) -> bool {
                    ((self.data >> $bit) & 0b1) == 0
                }

                pub fn $one(&self) -> bool {
                    ((self.data >> $bit) & 0b1) != 0
                }
            )*
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut set: Vec<String> = vec![];
                let mut remaining = self.data;
                $(
                    if self.$zero() {
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
    bit 0 ; data (0) | constant (1),
    bit 1 ; array (0) | variable (1),
    bit 2 ; absolute (0) | relative (1),
    bit 3 ; no_wrap (0) | wrap (1),
    bit 4 ; linear (0) | non_linear (1),
    bit 5 ; preferred_state (0) | no_preferred (1),
    bit 6 ; no_null_pos (0) | null_pos (1),
    // NOTE: This should always be zero on 'Input' items.
    bit 7 ; non_volatile (0) | volatile (1),
    bit 8 ; bit_field (0) | buffered_bytes (1)
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
    ReportSize = 7,
    ReportId = 8,
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
