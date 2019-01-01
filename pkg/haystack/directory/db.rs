use diesel::prelude::*;
use diesel::pg::PgConnection;
use super::super::errors::*;
use super::models::*;
use super::schema;
use bitwise::Word;
use std::env;
use dotenv::dotenv;

/// Simple wrapper for performing 
pub struct DB {
	conn: PgConnection
}


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
			.first::<Param>(&self.conn);

		match res {
			Ok(p) => Ok(Some(p.value)),
			Err(diesel::result::Error::NotFound) => Ok(None),
			Err(e) => Err(Error::from(e))
		}
	}

	/// Creates a new parameter. Errors out if it already exists
	pub fn create_param(&self, key: i32, value: Vec<u8>) -> Result<()> {

		let mut new_param = Param {
			id: key,
			value
		};

		let nrows = match diesel::insert_into(schema::params::table).values(&new_param).execute(&self.conn) {
			Ok(n) => n,
			Err(e) => return Err(Error::from(e))
		};

		if nrows != 1 {
			return Err("Failed to insert new param".into());
		}

		Ok(())
	}

	// Last remaining point is to super better transmutting 

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

}

fn establish_connection() -> PgConnection {
	dotenv().ok();

	let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
	PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}