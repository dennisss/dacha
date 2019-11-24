Protobuf requirements:
- Parse .proto file
- Generate code for a protobuf message
- For any file containing a protobuf, print out the contents using
  protobuf reflection.
	- Need DebugString parse/generation support.
- Generate code for a protobuf service
- Support HTTP(2) using futures/streams to enable RPCs (for now just request/response)
- Then gRPC
- 


Roadmap:
- Need plotting library
	- Primitives:
		- Line
		- Circle
		- Text
	- Control
		- Scroll
		- Drag


Goal for integration:
- Node.js starts a rust binary
- Waits for JSON/Protobuf messages as output which can



FoundationDB
- https://www.youtube.com/watch?v=WXk__WkS19o

SWIM Protocol Discussion
- https://www.brianstorti.com/swim/


- https://stackoverflow.com/questions/43932359/what-are-the-differences-between-cosmodb-and-documentdb

/*
	YouTube architecture
	- Upload 

	- Index videos
		- Photo { id, user_id, name }
	- For videos from a single user,
		- Just create a single index for [ user_id, id ]
		
	- For name based index
		- Sharded inverted index

	- Simple scenaio
		- Create inverted index indexing [ word, id ]

	- Upgrade a shard key which stored a hash of the id field
		- Searching is a fanout to many different machines storing each segment
		- Write availility would likely be based on replication factor and read factor

	- Significance of splitting up single documents across machines
		- Probably not needed
	
	- Given top-n results from each shard,
		- Assume each already has a rank associated with it
		- Perform merging of results
		- Basically k-way merging at this point
	
	- Further optimizations
		- Possible sharding based on specific words
		- 

	Read concurrency limited by how many replicas you have per shard
		- Maintaining a few replicas per availability 

	- Always readable probably
		- So we can 

*/

/*
	CA assumes that if there are no partitions ever, then 

	Spanner assumes

	Consistency and Availability
		- 

	CP
		- In the presense of a partition, we given up availability

	Consistency and Availability are things you choose based on waiting
		- You can't choose NOT to 

	CP partition tolerance
		- Quorum method
		- Can survive partitions as long as there is a quorum reachable from every single datacenter
		- Or if a datacenter goes down, some other datacenter can be reached

	Trivial CA system
		- In thep presence of a partition, block forever until the partition is resolved
		- Does this word?


	Distributed priority queue
	- Same idea as essentially maintaining a sorted list of stuff

	- Given some other key for the task, shard it to the appropriate sub-heap on a single set of replicas
		- Inserting into the system is relatively cheap (log_n of the cheapest system)
			- Depending on how implement each sorted list/heap and how many shards we have

	- Reading out will be log_k * log_n
		- The main question becomes do we need to know the total ordering
	
		- For something like sending out emails, we mainly need to have separate workers delegated to each individual shard (then it will be much cheaper)



	Representing MapReduce as an incremental computation job
	- Assume that all data is an incoming list of rows
	- Whenever a row comes in, schedule a task to run
	- 

	For any random read, can we gurantee that:
	1. The read is consistent
	2. The read will succeed 
	3. The system is not in a state of partition

	But then normal case for all systems is one of 
	- C and A occuring

	In the eventually consistent world,
	- 3 is typically a temporary state because of network latencies


	Cassandra strives for consistency by maintaining a ring with a reasonably stable 'leader'
		- Under normal cases of no membership changes, leaders will be very consistent
		- Therefore reads and writes become consistent

	Cassandra Read-Repair is sort of like the paxos strategy of applying values that were committed early before committed our latest value

*/