
/*
	Basically an implementation of the wire protocol described here:
	https://docs.mongodb.com/manual/reference/mongodb-wire-protocol/
*/

extern crate bson;

/*
	- Basically one single global range of all data ever
		- Different subunits for different aspects of state
		- 

	For decoding, we will essentially 

	One big question is how to make Raft have less consistent writes:
	- Assuming that we still have a leader
		- Stream and perform commits immediately

	- The main issue is how to deal with performing rollbacks

	- The most trivial setup is to perform a total rerun of the oplog
	- Naturally assume that rollbacks are never needed for ultra-high performance stuff

	- MongoDB style oplog with occassional commits
		- Followers don't need to deal with rollback, but they do need to 
	- Because packets are still sequential multiple consistent writes will block each other
		- No real lose in throughput

	- From what I can tell, CosmosDB consistency levels only effect the read operations?

	- In order to get better performance, we will support defining the state machine implementation per key range
		- This will allow proper log segmentation in case different models require different 

	- Going 


	Going full Dynamo/Cassandra yolo mode
		- Theoretically anyone can make a write
		- No longer any need for a sense of raft
		- Such a system is best for CRDT style semantics and mesh streaming networks
		- Implement Riak style CRDT semantics as well
		- Although Last-Write-Wins is still trivial to implement as a CRDT as well
		- 

*/

/*
	We will make a bson document serializable by encoding it as a byte buffer
	-> We assume that byte arrays have a well defined size

	-> Good news being that there are no end of stream terminated blocks

*/


/*
#[derive(Serialize, Deserialize)]
struct apples {
	pub time: std::time::Instant
}
*/

/*
	bincode is almost perfect but it does not support me just giving it a 

	Simple types:
	- LengthPrefixed([u8])


*/


struct MessageHeader {

	/// Total message size (inclusive of this header)
	message_length: u32,

	/// Unique identifier for this message
	request_id: u32,
	
	/// The request_id of the original request if this is a response packet
	response_to: u32,

	/// 
	op_code: u32
}


// TODO: Rewrite all of these descriptions
enum OpCode {
	Reply = 1, // Reply to a client request. responseTo is set.
	Update = 2001, // Update document.
	Insert = 2002, // Insert new document.
	RESERVED = 2003, // Formerly used for OP_GET_BY_OID.
	Query = 2004, // Query a collection.
	GetMore = 2005, // Get more data from a query. See Cursors.
	Delete = 2006, // Delete documents.
	KillCursors = 2007, // Notify database that the client has finished with the cursor.
	Command = 2010, // Cluster internal protocol representing a command request.
	CommandReply = 2011, // Cluster internal protocol representing a reply to an OP_COMMAND.
	Message = 2013 // Send a message using the format introduced in MongoDB 3.6.
}

/*
	General decoding strategy:
	- If possible, do not do any non-opaque stuff
	- 

*/

