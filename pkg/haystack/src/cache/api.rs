use super::super::common::*;
use super::super::store::api::*;


pub enum MachineIds {
	Data(Vec<MachineId>),

	/// The meaning of this will depend on the method used on the cache but will generally mean that the cache is free to choose which machines the request should be forwarded to
	Unspecified
}

impl MachineIds {
	pub fn to_string(&self) -> String {
		match self {
			// TODO: Must have at least one element for this to be valid
			MachineIds::Data(arr) =>
				arr.iter().map(|id| id.to_string()).collect::<Vec<String>>().join("-"),
			MachineIds::Unspecified =>
				"-".into()
		}
	}
}

impl std::str::FromStr for MachineIds {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s == "-" {
			return Ok(MachineIds::Unspecified);
		}

		let mut list = vec![];

		for part in s.split('-').into_iter() {
			match part.parse::<MachineId>() {
				Ok(v) => list.push(v),
				Err(_) => return Err("Contains invalid ids")
			};
		}

		Ok(MachineIds::Data(list))

	}
}


pub enum CachePath {
	// '/'
	Index,

	// '/<machine_ids>/<some_valid_store_path>'
	Proxy {
		machine_ids: MachineIds,
		store: StorePath
	}
}

impl CachePath {
	pub fn from(segs: &[String]) -> std::result::Result<CachePath, &'static str> {
		if segs.len() == 0 {
			return Ok(CachePath::Index);
		}

		let machine_ids = match segs[0].parse::<MachineIds>() {
			Ok(v) => v,
			Err(_) => return Err("Invalid machine ids")
		};

		let store = StorePath::from(&segs[1..])?;

		Ok(CachePath::Proxy {
			machine_ids,
			store
		})
	}

	pub fn to_string(&self) -> String {
		match self {
			CachePath::Index => "/".into(),
			CachePath::Proxy { machine_ids, store } => 
				format!("/{}{}", machine_ids.to_string(), store.to_string())
		}
	}
}


