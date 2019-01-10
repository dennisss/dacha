use diesel::*;
use super::schema::*;
use super::super::common::{STORE_MACHINE_HEARTBEAT_TIMEOUT, ALLOCATION_SIZE};
use chrono::{DateTime, Utc, Duration};

pub enum ParamKey {
	ClusterId = 1
}


#[derive(Queryable, Insertable)]
#[table_name = "params"]
pub struct Param {
	pub id: i32,
	pub value: Vec<u8>
}


#[derive(Queryable, Identifiable, AsChangeset)]
#[table_name = "store_machines"]
pub struct StoreMachine {
	pub id: i32,
	pub addr_ip: String,
	pub addr_port: i16,
	pub last_heartbeat: DateTime<Utc>,

	pub ready: bool,
	pub alive: bool,
	pub healthy: bool,

	/// Sum of the space allocated towards every volume on this machine
	/// Updated periodically by the store
	pub allocated_space: i64,

	/// Total space on the machine's disks
	/// Decided by the machine itself and is usually a small amount lower than the full physical capacity to account fo metadata 
	pub total_space: i64,


	/// Set to true if the machine is accepting new writes (for existing volumes)
	/// NOTE: This says nothing about new-allocations right now
	pub write_enabled: bool
}

impl StoreMachine {

	/// Check whether or not we are allowed to read from this machine
	pub fn can_read(&self) -> bool {
		// TODO: Eventually also account for the external health checks by pitch-fork

		if !self.ready {
			return false;
		}

		let now = Utc::now();
		if (
			now.ge(&self.last_heartbeat) &&
			(now - (self.last_heartbeat)).ge(&Duration::milliseconds(STORE_MACHINE_HEARTBEAT_TIMEOUT as i64))
		) {
			return false;
		}

		true
	}

	/// Check whether or not we are allocated to write new needles to any writeable volume on this machine
	pub fn can_write(&self) -> bool {
		self.write_enabled && self.can_read()
	}

	/// Check whether we are allowed to create a new volume on this machine
	pub fn can_allocate(&self) -> bool {
		self.can_read() && (self.allocated_space + (ALLOCATION_SIZE as i64) < self.total_space)
	}

	pub fn addr(&self) -> String {
		self.addr_ip.clone() + ":" + &self.addr_port.to_string()
	}
}



#[derive(Insertable)]
#[table_name = "store_machines"]
pub struct NewStoreMachine<'a> {
	pub addr_ip: &'a str,
	pub addr_port: i16,
}

/// NOTE: These will be ephemeral and will only exist while they need to 
#[derive(Queryable, Identifiable)]
#[table_name = "cache_machines"]
pub struct CacheMachine {
	pub id: i32,
	pub addr_ip: String,
	pub addr_port: i16,
	pub last_heartbeat: DateTime<Utc>,
	pub ready: bool,
	pub alive: bool,
	pub healthy: bool,
	pub hostname: String // TODO: Do we still want to use this?
}

impl CacheMachine {

	// Basically the same as the StoreMachine one
	pub fn can_read(&self) -> bool {
		if !self.ready {
			return false;
		}

		let now = Utc::now();
		if (
			now.ge(&self.last_heartbeat) &&
			(now - (self.last_heartbeat)).ge(&Duration::milliseconds(STORE_MACHINE_HEARTBEAT_TIMEOUT as i64))
		) {
			return false;
		}

		true
	} 
	
}


#[derive(Insertable)]
#[table_name = "cache_machines"]
pub struct NewCacheMachine<'a> {
	pub addr_ip: &'a str,
	pub addr_port: i16,
	pub hostname: &'a str
}

// Logical volumes are locked once a physical volume is near its limit
// Otherwise, we don't really

// 


#[derive(Queryable, Identifiable, AsChangeset)]
#[table_name = "logical_volumes"]
pub struct LogicalVolume {
	pub id: i32,
	pub write_enabled: bool,
	pub hash_key: i64
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


#[derive(Queryable, Identifiable, AsChangeset)]
#[table_name = "photos"]
pub struct Photo {
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



