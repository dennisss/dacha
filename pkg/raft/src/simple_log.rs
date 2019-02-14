use super::errors::*;
use super::rpc::*;
use super::atomic::*;
use super::log::*;
use super::protos::*;
use std::sync::{Arc, Mutex};
use std::path::Path;


/// A simple log implementation backed be a single file that is rewritten every time a flush is needed and otherwise stores all entries in memory 
pub struct SimpleLog {
	mem: MemoryLogStorage,
	snapshot: Mutex<(u64, BlobFile)>
}

impl SimpleLog {

	pub fn create(path: &Path) -> Result<SimpleLog> {
		let b = BlobFile::builder(path)?;

		let log: Vec<LogEntry> = vec![];
		let file = b.create(&marshal(log)?)?;

		Ok(SimpleLog {
			mem: MemoryLogStorage::new(),
			snapshot: Mutex::new((0, file))
		})
	}

	pub fn open(path: &Path) -> Result<SimpleLog> {
		let b = BlobFile::builder(path)?;
		let (file, data) = b.open()?;

		let log: Vec<LogEntry> = unmarshal(&data)?;
		let mem = MemoryLogStorage::new();

		println!("RESTORE {:?}", log);

		let mut match_index = 0;

		for e in log {
			match_index = e.index;
			mem.append(e);
		}

		Ok(SimpleLog {
			mem,
			snapshot: Mutex::new((match_index, file))
		})
	}

	pub fn purge(path: &Path) -> Result<()> {
		let b = BlobFile::builder(path)?;
		b.purge()?;
		Ok(())
	}

}


impl LogStorage for SimpleLog {
	fn term(&self, index: u64) -> Option<u64> { self.mem.term(index) }
	fn first_index(&self) -> Option<u64> { self.mem.first_index() }
	fn last_index(&self) -> Option<u64> { self.mem.last_index() }
	fn entry(&self, index: u64) -> Option<Arc<LogEntry>> { self.mem.entry(index) }
	fn append(&self, entry: LogEntry) { self.mem.append(entry); }
	fn truncate_suffix(&self, start_index: u64) { self.mem.truncate_suffix(start_index); }
	
	// TODO: May be wrong on truncations right?
	fn match_index(&self) -> Option<u64> {
		Some(self.snapshot.lock().unwrap().0)
	}

	fn flush(&self) -> Result<()> {
		// TODO: Must also make sure to not do unnecessary updates if nothing has changed
		// TODO: This should ideally also not hold a snapshot lock for too long as that may 

		let mut s = self.snapshot.lock().unwrap();

		let idx = self.mem.last_index().unwrap_or(0);
		let mut log: Vec<LogEntry> = vec![];

		let mut last_idx = s.0;

		for i in 1..(idx + 1) {
			let e = self.mem.entry(i).expect("Failed to get entry from log");
			last_idx = e.index;
			log.push((*e).clone());
		}

		s.1.store(&marshal(log)?)?;
		s.0 = last_idx;

		Ok(())
	}
}


