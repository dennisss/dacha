The Repository
==============

This is a monorepo for housing many exciting projects and research implementations that I'm building. Individual subprojects are located in the `pkg/[name]` folders.


Listing
-------

- `raft`: Implementation of Raft consensus and everything needed to make a replicated state machine from an existing non-replicated one

- `haystack`: (Re-)Implementation of the Facebook Haystack distributed photo (or blob) store written in Rust

- `μsvg`: Aggressive SVG minifier written in Node.js


Building
--------

Install Rust using `rustup install nightly-2019-01-23`

READ ME: https://github.com/leandromoreira/digital_video_introduction

- Rust based projects
	- Require nightly to be installed
	- Build most recently on `rustc 1.33.0-nightly (19f8958f8 2019-01-23)`
		- Mainly requires for futures-await


General long term implementation order:
- See also https://github.com/antirez/disque

- See also the space of read-only databases:
	- PalDB: https://engineering.linkedin.com/blog/2015/10/open-sourcing-paldb--a-lightweight-companion-for-storing-side-da

- Raft
- Chubby (based on Raft for now)
	- Mainly a future prerequisite to make it easier to implement everything else without 

- Dynamo (would be greatly simplified by having Chubby just for maintaining a device list)
	- Dynamo uses a Gossip protocol (probably depends on something like UDP Broadcast/Multicast)
	- But both of modes are hard to use in cloud environments
		- In Kubernetes this would be equivalent to fetching the list of pods in a deployment or service
		- Otherwise, trivially implementable given an ambient key-value store present based on a common key prefix, heartbeats, etc.
			- This would then be extendable back to what we do right now in an adhoc way for the haystack via SQL 
			

- S3/Cloud Storage style Object Storage
	- Create a pending file record allocated to some number of servers
	- Transfer over to a quorum of them
	- Basically upload over to GFS
	- GFS master deals with maintaining minimum consistency levels
		- This would be for a single-datacenter configuration
		- For multi-
	- Mark the file as uploaded with 

	- This is how Google Cloud Storage uses
		- https://www.infoq.com/presentations/Google-Cloud-Storage

https://cloud.google.com/files/storage_architecture_and_challenges.pdf
	- BlobStore supposedly built on top of BigTable (I assume mainly just for performing deduplication based on file hashes)

https://medium.com/@jerub/the-production-environment-at-google-8a1aaece3767
	- Also makes reference to a Blobstore system 
	- The general idea though is still that of atomic writes of a single file
		- Basically write a huge file into GFS
		- Ideally it should support good sequential read-back performance (so chunks largely appended together)


- CosmosDB overview
	- https://www.systutorials.com/3222/microsofts-cosmos-service/

- GFS (depends on Chubby)
- BigTable (depends on GFS + Chubby)
- MegaStore (depends on GFS + Chubby + a 2 phase commit protocol functional)
- Spanner
	- Just up to the constraints of mapking a distributed key-value store
	- BigTable + Linearizability



See also MITs Chord DHT
- https://github.com/sit/dht




How would one build Colossus
- Issues with GFS
	- Single master
	- Large files can get unwieldy for operations
	- Some more info on it: http://www.pdsw.org/pdsw-discs17/slides/PDSW-DISCS-Google-Keynote.pdf
- v2/Colossus
	- Balances load across disks of different sizes
		- Ideally keep equal parts hot data per disk to maximize usage of IOPS
	- The key difference is that metadata is not sharded
		- Basically run Colossus on BigTable on Colossus (or originally GFS)
		- Root level metadata about tablet placement should fit in memory an can likely have much lower availability

	- Then we have a more sophisticated notion of replication groups
		- How to best do multi-region replication groups
			- Using something like spanner, we can still maintain strong global consistency
			- Obviously if we can't contact a quorum of other datacenters then we can't ever satisfy global replication
		- A lesser task we can agree on
			- Find some non-quorum of datacenters that we can agree on
			- As long as the number is above some threshold, we can agree that something is written
			- Main goal becomes ensuring consistency over the long term
			- Obviously no longer strongly consistent without a quorum

