use diesel::*;
use super::schema::*;
use chrono::{DateTime, Utc};

pub enum ParamKey {
	ClusterId = 1
}


#[derive(Queryable, Insertable)]
#[table_name = "params"]
pub struct Param {
	pub id: i32,
	pub value: Vec<u8>
}


#[derive(Queryable, Identifiable)]
#[table_name = "store_machines"]
pub struct StoreMachine {
	pub id: i32,
	pub addr_ip: String,
	pub addr_port: i16,
	pub last_heartbeat: DateTime<Utc>,

	/// Sum of the space allocated allocated towards every single volume assigned to it
	/// (managed by the directory)
	pub allocated_space: i64,

	/// Total space on the machine's disks
	/// Decided by the machine itself and is usually a small amount lower than the full physical capacity to account fo metadata 
	/// (managed by the store: set during heartbeats)
	pub total_space: i64,

	/// For all locked volumes, this is the amount of space which has gone unused
	/// (managed by the store: set during heartbeats)
	pub reclaimed_space: i64,

	/// Set to true if the machine is accepting new writes
	pub write_enabled: bool,

	pub dirty: bool
}

#[derive(Insertable)]
#[table_name = "store_machines"]
pub struct NewStoreMachine<'a> {
	pub addr_ip: &'a str,
	pub addr_port: i16,
}

#[derive(Queryable, Identifiable)]
#[table_name = "cache_machines"]
pub struct CacheMachine {
	pub id: i32,
	pub addr_ip: String,
	pub addr_port: i16,
	pub last_heartbeat: DateTime<Utc>,
	pub ready: bool,
	pub hostname: String
}

#[derive(Insertable)]
#[table_name = "cache_machines"]
pub struct NewCacheMachine<'a> {
	pub addr_ip: &'a str,
	pub addr_port: i16,
	pub hostname: &'a str
}





#[derive(Queryable, Identifiable)]
#[table_name = "logical_volumes"]
pub struct LogicalVolume {
	pub id: i32,
	pub num_needles: i64,
	pub used_space: i64,
	pub allocated_space: i64,
	pub write_enabled: bool,
	pub hash_key: i64,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>
}

#[derive(Insertable)]
#[table_name = "logical_volumes"]
pub struct NewLogicalVolume {
	pub hash_key: i64,
}


#[derive(Queryable, Insertable)]
#[table_name = "physical_volumes"]
pub struct PhysicalVolume {
	pub logical_id: i32,
	pub machine_id: i32
}


pub struct Photo {
	pub id: i64,
	pub volume_id: i32,
	pub cookie: Vec<u8>
}

#[derive(Queryable, Identifiable)]
#[table_name = "photos"]
struct PhotoData {
	pub id: i64,
	pub volume_id: i32,
	pub cookie: Vec<u8>
}

#[derive(Insertable)]
#[table_name = "photos"]
pub struct NewPhoto<'a> {
	pub volume_id: i32,
	pub cookie: &'a [u8]
}



