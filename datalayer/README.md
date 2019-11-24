DataLayer
=========

TODO: If running in memory, we'd want to use this: https://github.com/facebook/rocksdb/wiki/PlainTable-Format

Designing as a durable pubsub store
- Basically one single very long k-v table with just appends at the beginning or end
- Individual separate rows to mark consumer offsets in this log
- Partitioning the feed:
	- 


Hopefully to become a database inspired by BigTable, CockroachDB, and Redis written in Go
- The objective is to meet somewhere in the middle of these three
- We will organize data similarly to BigTable as tablets backed by RocksDB
- Replication will be Master-Slave with a single master per tablet
- Like CockroachDB, all nodes run the same code
	- Masters are managed using Raft consensus
	- But, we will NOT use Raft during writes
	- Instead writes will be eventually consistent but are completely ordered for
	- Writes should be able to optionally wait for some number of nodes to acknowledge the operation before continuin
		- If acknowledgement fails, then the master will attempt to apply a 'revert' to the oplog, but there are no gurantees for this 
		- Clients should be start enough to look for the revert before applying the previous operations (if they haven't applied them yet)
		- Revert is the action of deleting 

- API: gRPC protocol supporting the following operations
	- Get Key
	- Set/Insert Key(s)
	- Get Key range


Phase 2:
- After we build the BigTable style durable database layer, we will build a Redis style in-memory cache by swapping out the RocksDB store for an in-memory store

Phase 3:
- Use rows of the cache layer to store a list of subscriptions (along with timestamps of when the subscriptions started)
	- Use a node went through a restart after the subscription time, then we don't need to give 


We'd like to keep the complexity of this ambitious system down by forgoing high availability 

Phase 4:
- Some form of native support for CRDT support to bring back high availability
	- This will basically be in the form of certain ranges whose data doesn't need serializability or strong-consistency 
	- Moving towards more of a Cassandra model for these ranges


Phase 5:
- Time series support
	- The main new feature would be to support transparent range splitting based on load
	- If a timeseries is getting a lot of load, split ranges based on a hash of the timestamp starting at some time chunked into blocks of some time length
	- Probably best to go per week



---- More notes below ----

- Master-slave replication with single range database would be step 1
- Then we'd need to have many master-slave operations such as this to do stuff



1. Build abstraction around RocksDB for Go
2. Cluster node starts up. If it doesn' have a root tablet, create an empty root tablet and elect a leader or it
3. Once we have a leader, it should create a single child data table and store it's unique id and data range in the metadata table

4. Once everyone has the first table, we must elect a leader for it
	- Assuming we partition data based on which cluster it come into, we can 

2. First task is to perform master election for the root tablet 

- General idea:
	- Send to all of them

- Relatively stable table schema
- HBase on the 


Basically I need a BigTable clone
- Needs a chubby locking system-esc thing to manage masters for each tablet
	- We can use etcd raft groups to 
- Master should think about the state of each slave


Needed networked operations: 


How can we leverage cockroachdb for simplicity:
- We can sync tablet metatables in cockcoachdb?
	-


- How to handle proper splits:
	- With a static list of ranges, everything is super easy as we just need to many independent consistency groups

	- Splits can occur independently at each node
- Splits may only be initiated by current master

- Writes two atomic records to the metadata range via the master. Once a 

- Each node will store copies of the metadata
	- When a node receives a change to the metadata table that they are overlooking, they will receive adjust their current configuration to that new configuration
		- This may require either splitting or reverting splits

- Efficient splits:
	- Given a well decided split point
	- Create two new tables
	- Redirect all reads and writes to these two new tables
		- Reads for ranges or individual keys that miss will also query the initial table
	- Transfer all data to the new two tables
	- NOTE: Because new writes could be overwritten during transfer, we will timestamp each row so that we know to 
	- NOTE: To ensure durability, we will record the current progress of operations on tables into a separate RocksDB table
	- Upon completion of data transfer, we can delete the old query
	- Garbage collect stuff now

- Likewise for range joins,
	- Create new table
	- Redirect reads
	- Transfer data
	- Upon completion, delete old tables

- How to: making single row operations transactional
	- Perform operations regularly 
	- Use raft log to propose new timestamp for the whole
	- NOTE: If the node ever restarts, we need to garbage collect any rows that are ahead of the locally known timestamp because we can't gurantee that there are part of the actual 
		- A slave is allowed to vote for a change if it has received the data
		- 

- Bulk backup 
	- Pick a timestamp
	- Backup whole range from some slave
	- Store as a partitioned output
	- Restore should work efficiently as we don't need to do any 


- Have a built in Cache as well
	- Basically would end up just being like the usual one but with in-memory ranges 

- Would be really great if this could relay events as well
	- Room subscription model
	- Basically we need to support very many rooms with reletated few messages flowing through them
	- There is no need for consistency with this really
	- just need to efficiently know which nodes to send data to
		- nodes will have remote api clients connected to them that will create subscriptions
		- but it is the nodes that will be receiving them
			- Convert any database key into a pubsub-able entity
				- These will be special keys which store a list of which nodes are subscribed to it (encoded as set bits)
				- When a message comes in at a node, it reads back it's subscriber list for said key,  

- How about a full text node search node as well
