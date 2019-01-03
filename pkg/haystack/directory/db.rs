use diesel::prelude::*;
use diesel::pg::PgConnection;
use super::super::errors::*;
use super::super::common::*;
use super::models::*;
use super::schema;
use bitwise::Word;
use std::env;
use dotenv::dotenv;
use chrono::{Utc};


/// Simple wrapper for performing 
pub struct DB {
	conn: PgConnection
}

use diesel::expression::sql_literal::sql;
use diesel::sql_types::{Integer};

impl DB {

	pub fn connect() -> DB {
		DB {
			conn: establish_connection()
		}
	}


	pub fn get_param(&self, key: i32) -> Result<Option<Vec<u8>>> {
		use super::schema::params::dsl::*;

		let res = params
			.filter(id.eq(key))
			.first::<Param>(&self.conn).optional()?;

		match res {
			Some(v) => Ok(Some(v.value)),
			None => Ok(None)
		}
	}

	/// Creates a new parameter. Errors out if it already exists
	pub fn create_param(&self, key: i32, value: Vec<u8>) -> Result<()> {
		// Generally I would like a wrapper that makes a


		let mut new_param = Param {
			id: key,
			value
		};

		// TODO: Do 
		let nrows = match diesel::insert_into(schema::params::table).values(&new_param).execute(&self.conn) {
			Ok(n) => n,
			Err(e) => return Err(Error::from(e))
		};

		if nrows != 1 {
			return Err("Failed to insert new param".into());
		}

		Ok(())
	}

	pub fn create_logical_volume(&self, vol: &NewLogicalVolume) -> Result<LogicalVolume> {
		let v = diesel::insert_into(schema::logical_volumes::table)
			.values(vol)
			.get_result::<LogicalVolume>(&self.conn)?;

		Ok(v)
	}

	pub fn index_logical_volumes(&self) -> Result<Vec<LogicalVolume>> {
		use super::schema::logical_volumes::dsl::*;
		Ok(logical_volumes.get_results::<LogicalVolume>(&self.conn)?)
	}

	pub fn read_logical_volume(&self, id_value: VolumeId) -> Result<Option<LogicalVolume>> {
		use super::schema::logical_volumes::dsl::*;
		Ok(logical_volumes.filter(id.eq(id_value.to_signed())).first::<LogicalVolume>(&self.conn).optional()?)
	}

	/// Find all logical volumes associated with a single machine
	pub fn read_logical_volumes_for_store_machine(&self, id_value: MachineId) -> Result<Vec<LogicalVolume>> {

		use super::schema::logical_volumes::dsl::*;
		use super::schema::physical_volumes::dsl::*;

		Ok(
			logical_volumes
			.inner_join(physical_volumes)
			.filter(schema::physical_volumes::machine_id.eq(id_value.to_signed()))
			.get_results::<(LogicalVolume,PhysicalVolume)>(&self.conn)?
			.into_iter().map(|(v, _)| { v }).collect()
		)
	}

	// TODO: We do want to be able to update many logical volumes all at once for 
	pub fn update_logical_volume_writeable(&self, id_value: VolumeId, is: bool) -> Result<()> {
		use super::schema::logical_volumes::dsl::*;

		expect_changed(
			diesel::update(
				logical_volumes
				.filter(id.eq(id_value.to_signed()))
			)
			.set(write_enabled.eq(is))
			.execute(&self.conn)?
		)
	}

	pub fn create_photo(&self, new_photo: &NewPhoto) -> Result<Photo> {
		Ok(diesel::insert_into(schema::photos::table)
			.values(new_photo)
			.get_result::<Photo>(&self.conn)?)
	}

	pub fn read_photo(&self, id_value: NeedleKey) -> Result<Option<Photo>> {
		use super::schema::photos::dsl::*;
		Ok(photos.filter(id.eq(id_value.to_signed())).first::<Photo>(&self.conn).optional()?)
	}

	/// Performs a test-and-set on the volume_id of a photo
	/// Will succeed only if operation ended up changing the volume_id
	pub fn update_photo_volume_id(&self, photo: &Photo, new_volume_id: VolumeId) -> Result<()> {
		use super::schema::photos::dsl::*;

		expect_changed(
			diesel::update(
				photos
				.filter(id.eq(photo.id))
				.filter(volume_id.eq(photo.volume_id))
			)
			.set(volume_id.eq(new_volume_id as i32))
			.execute(&self.conn)?
		)
	}

	/// Deletes a photo
	/// Will succeed if and only if the logical volume hasn't changed since last time we checked
	pub fn delete_photo(&self, photo: &Photo) -> Result<()> {
		use super::schema::photos::dsl::*;

		expect_changed(
			diesel::delete(
				photos
				.filter(id.eq(photo.id))
				.filter(volume_id.eq(photo.volume_id))
			)
			.execute(&self.conn)?
		)
	} 

	// NOTE: We would like to atomically increment the size of allocated space as well as adding the volume
	// TODO: Should we also simultaenously change the allocation amounts
	pub fn create_physical_volume(&self, logical_id: VolumeId, machine_id: MachineId) -> Result<()> {
		expect_changed(
			diesel::insert_into(schema::physical_volumes::table)
				.values(&PhysicalVolume {
					logical_id: logical_id.to_signed(),
					machine_id: machine_id.to_signed()
				})
				.execute(&self.conn)?
		)
	}

	// TODO: Eventually may need to be able to delete physical_volume mappings if we decide that a machine is completely dead and not recoverable

	pub fn create_store_machine(&self, addr_ip: &str, addr_port: u16) -> Result<StoreMachine> {
		
		let new_machine = NewStoreMachine {
			addr_ip,
			addr_port: addr_port.to_signed()
		};

		let m = diesel::insert_into(schema::store_machines::table)
			.values(&new_machine)
			.get_result::<StoreMachine>(&self.conn)?;

		Ok(m)
	}

	pub fn index_store_machines(&self) -> Result<Vec<StoreMachine>> {
		use super::schema::store_machines::dsl::*;
		Ok(store_machines.get_results::<StoreMachine>(&self.conn)?)
	}

	pub fn read_store_machine(&self, id_value: MachineId) -> Result<Option<StoreMachine>> {
		use super::schema::store_machines::dsl::*;

		Ok(store_machines
			.filter(id.eq(id_value as i32))
			.first::<StoreMachine>(&self.conn).optional()?)
	}

	pub fn read_store_machines_for_volume(&self, vol: VolumeId) -> Result<Vec<StoreMachine>> {
		use super::schema::store_machines::dsl::*;
		use super::schema::physical_volumes::dsl::*;

		Ok(
			store_machines
			.inner_join(physical_volumes)
			.filter(schema::physical_volumes::logical_id.eq(vol.to_signed()))
			.get_results::<(StoreMachine,PhysicalVolume)>(&self.conn)?
			.into_iter().map(|(s, _)| { s }).collect()
		)
	}

	pub fn update_store_machine_heartbeat(&self,
		id_value: MachineId,
		ready_value: bool,
		addr_ip_value: &str, addr_port_value: u16,
		allocated_space_value: u64, total_space_value: u64, write_enabled_value: bool
	) -> Result<()> {
		use super::schema::store_machines::dsl::*;

		expect_changed(
			diesel::update(
				store_machines.filter(id.eq(id_value.to_signed()))
			)
			.set((
				ready.eq(ready_value),
				addr_ip.eq(addr_ip_value),
				addr_port.eq(addr_port_value.to_signed()),
				last_heartbeat.eq( Utc::now() ),
				allocated_space.eq(allocated_space_value.to_signed()),
				total_space.eq(total_space_value.to_signed()),
				write_enabled.eq(write_enabled_value)
			))
			.execute(&self.conn)?
		)
	}


	pub fn update_store_machine_health(&self, id_value: MachineId, alive_value: bool, healthy_value: bool) -> Result<()> {
		use super::schema::store_machines::dsl::*;

		expect_changed(
			diesel::update(
				store_machines.filter(id.eq(id_value.to_signed()))
			)
			.set((
				alive.eq(alive_value),
				healthy.eq(healthy_value)
			))
			.execute(&self.conn)?
		)
	}

	
}

fn expect_changed(n: usize) -> Result<()> {
	if n != 1 {
		Err("Nothing modified".into())
	}
	else {
		Ok(())
	}
}

fn establish_connection() -> PgConnection {
	dotenv().ok();

	let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
	PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}