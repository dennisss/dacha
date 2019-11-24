use super::errors::*;
use super::rpc::*;
use super::atomic::*;
use super::log::*;
use super::protos::*;
use std::sync::{Arc, Mutex};
use std::path::Path;


/// A simple log implementation backed be a single file that is rewritten completely every time a flush is needed and otherwise stores all entries in memory 
pub struct SimpleLog {
	mem: MemoryLog,

	/// The position of the last entry stored in the snapshot
	last_flushed: Mutex<LogSeq>,

	/// The single file backing the log
	snapshot: Mutex<BlobFile>
}

impl SimpleLog {

	pub fn create(path: &Path) -> Result<SimpleLog> {
		let b = BlobFile::builder(path)?;

		let log: Vec<LogEntry> = vec![];
		let file = b.create(&marshal(log)?)?;

		Ok(SimpleLog {
			mem: MemoryLog::new(),
			last_flushed: Mutex::new(LogSeq(0)),
			snapshot: Mutex::new(file)
		})
	}

	pub fn open(path: &Path) -> Result<SimpleLog> {
		let b = BlobFile::builder(path)?;
		let (file, data) = b.open()?;

		let log: Vec<LogEntry> = unmarshal(&data)?;
		let mem = MemoryLog::new();

		println!("RESTORE {:?}", log);

		let mut seq = LogSeq(0); // log.last().map(|e| e.pos.clone()).unwrap_or(LogPosition::zero());

		for e in log {
			seq = mem.append(e);
		}

		Ok(SimpleLog {
			mem,
			last_flushed: Mutex::new(seq),
			snapshot: Mutex::new(file)
		})
	}

	pub fn purge(path: &Path) -> Result<()> {
		let b = BlobFile::builder(path)?;
		b.purge()?;
		Ok(())
	}

}


impl Log for SimpleLog {
	// TODO: Because this almost always needs to be shared, we might as well force usage with a separate Log type that just implements initial creation, checkpointing, truncation, and flushing related functions
	fn term(&self, index: LogIndex) -> Option<Term> { self.mem.term(index) }
	fn first_index(&self) -> LogIndex { self.mem.first_index() }
	fn last_index(&self) -> LogIndex { self.mem.last_index() }
	fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSeq)> { self.mem.entry(index) }
	fn append(&self, entry: LogEntry) -> LogSeq { self.mem.append(entry) }
	fn truncate(&self, start_index: LogIndex) -> Option<LogSeq> { self.mem.truncate(start_index) }
	fn checkpoint(&self) -> LogPosition { self.mem.checkpoint() }
	fn discard(&self, pos: LogPosition) { self.mem.discard(pos) }

	// TODO: Is there any point in ever 
	fn last_flushed(&self) -> Option<LogSeq> {
		Some(
			self.last_flushed.lock().unwrap().clone()
		)
	}

	fn flush(&self) -> Result<()> {
		// TODO: Must also make sure to not do unnecessary updates if nothing has changed
		// TODO: This should ideally also not hold a snapshot lock for too long as that may 

		let mut s = self.snapshot.lock().unwrap();

		let idx = self.mem.last_index();
		let mut log: Vec<LogEntry> = vec![];

		let mut last_seq = LogSeq(0);

		for i in 1..(idx + 1) {
			let (e, seq) = self.mem.entry(i).expect("Failed to get entry from log");
			last_seq = seq;
			log.push((*e).clone());
		}

		s.store(&marshal(log)?)?;

		*self.last_flushed.lock().unwrap() = last_seq;

		Ok(())
	}
}


