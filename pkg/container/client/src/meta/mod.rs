pub mod client;
pub mod constants;
pub mod hostname;
pub mod table;
mod user;

pub use self::table::*;
pub use self::user::*;

use db_table::table_id;

pub const INVENTORY_PART_TABLE_ID: u32 = table_id!(60);
pub const INVENTORY_PACK_TABLE_ID: u32 = table_id!(61);