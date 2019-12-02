

https://docs.minio.io/docs/minio-erasure-code-quickstart-guide

https://blogs.msdn.microsoft.com/windowsazurestorage/2010/12/30/windows-azure-storage-architecture-overview/

TODOs
-----

- Google BigTable
- Amazon Dynamo
- Google GFS (v1)
- Google Colossus (GFS v2)
- Google Piper
- Google Megastore
- Google Spanner
- Google Maglev
- Google F1

TODO: What would be really nice:
- Use the WAL for storing OT operations only so that we don't have to store both the document update and the OT op in the WAL (as crashes aren't super high frequency, it's better to optimize the size of the WAL in this way (fewest bytes to disk needed to commit))

TODO: Using the neon library for interoping with node.js + electron ui
- https://www.neon-bindings.com/



The original objective of this was to allow message publishes to only require sending messages to the minimum subset of all nodes with clients subscribed to the room

Development strategy
1. Link in RocksDB again
2. Implement the Redis interface
3. Implement Raft (or single Paxos)
4. Simple strong consistency 


- Likely a lot easier to start with RocksDB again and build up Redis first
	- Then build out BigTable
	- Mixin some Cassandra inspiration in there somewhere

- Brilliant version of SkipLists
	- https://brilliant.org/wiki/skip-lists/

- Notes on writing linked list in Rust
	- http://cglab.ca/~abeinges/blah/too-many-lists/book/README.html

- Also read this on skip lists
	- http://ticki.github.io/blog/skip-lists-done-right/


- Exposed API
	- Get, Set, GetRange, DeleteRange

	- Eventual api: GetMany (ideally with some optimisations for seeking over the)

- First hits the in-memory SkipList
	- Maintain size in bytes of skiplist
	- Once too big, make the skiplist immutable and skip with a new one
		- At this point, also start a new log file (so that all the actions are well-separated )
	- In the background, flush the immutable skiplist to an sst file (with all the usual bloom, etc. features)
	- When >= 2 sst's are present, start merging them
		- basically creating a new file and then 
		- 4-way merge operation between the 2 sets of key pairs and the 2 pairs of delete range operations
			- Although delete range operations 
		- Possibly merge the DeleteRange operations first and then zip with the keystream

- Garbage collecting a DeleteRange
	- Can only be done by merging the bottom most layers
		- Aka most likely the least likely two layers to be hit by new operations


/*
	Data block format:
	
	struct BlobBlock {


	}

	dL01
	^ Magic

	struct BlobBlock {
		checksum: u32; // Checksum of everything after the 'size'
		size: u16;	
		flags: u8; // Whether is not it is actually compressed
		decompressed_size: u32 // Present only if compressed
		// ... (compressed) data ...
	}

	struct FileMetadata {
		// Name/path implied by path
		permissions: u16;

		volume_idx: u8;
		blob_offset: u64; // offset from the start of the volume to the start of the compressed blob
		start_offset: u16; // offset from the start of the uncompressed blob data to the start of this file

	}
*/


/*
	Creating a sorted string table mostly compatible with:
	NOTE: We assume a nice 

	https://github.com/facebook/rocksdb/wiki/Rocksdb-BlockBasedTable-Format
	
	https://github.com/facebook/rocksdb/blob/master/table/block_builder.cc#L21

	https://github.com/google/leveldb/blob/master/doc/impl.md


	https://github.com/facebook/rocksdb/blob/cd9404bb774695732b715b7cccc1d8f7e4bd94a1/table/block_based_table_builder.cc#L500
*/
