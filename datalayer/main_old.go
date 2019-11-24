package main

import (
	"fmt"

	"github.com/tecbot/gorocksdb" // https://godoc.org/github.com/tecbot/gorocksdb
)

func main() {
	fmt.Println("Hello")

	bbto := gorocksdb.NewDefaultBlockBasedTableOptions()
	bbto.SetBlockCache(gorocksdb.NewLRUCache(3 << 30))
	opts := gorocksdb.NewDefaultOptions()
	opts.SetBlockBasedTableFactory(bbto)
	opts.SetCreateIfMissing(true)
	db, err := gorocksdb.OpenDb(opts, "mydb")

	ro := gorocksdb.NewDefaultReadOptions()
	wo := gorocksdb.NewDefaultWriteOptions()

	// if ro and wo are not used again, be sure to Close them.
	err = db.Put(wo, []byte("foo"), []byte("bar"))

	value, err := db.Get(ro, []byte("foo"))

	if err == nil {
		fmt.Println(string(value.Data()))
		fmt.Println(err)
	}

	defer value.Free()

	err = db.Delete(wo, []byte("foo"))

}

/*

Cassandra really annoys me because of the ineffiency of the repair process
- Cockroachdb is very reasonable aside from the fact that backups are crazy inefficient

- We could make it more efficient by ourselves if we

Let's just do the following:
- If we decide on Cassandra, then we need to constantly do read repairs
	- Use raft to agree on multiple

- TODO: Etcd raft probably doesn't
	-
	- TODO: Another thing we don't implement is what to do when new nodes come online or re-online
	- We don'e have a good long-term


- Why CockroachDB backup is so slow
	- My interlaced tables are not a super good idea
	-


Raft does its own replication process, but it isn't very relevant to us because I don't want to use raft replies in order to get it consistent

But for event logs, we have unique ids, so we could just use cassandra
- We could be able to this for tf operations if we had guranteed unique ids for every single
- TODO Annoying part about cassandra is that it won't necessarily replicate in a timely manner if we
	- NOTE: The issue issue with cassandra is that the hash based partitioning doesn't work well with all of our range optimizations

- Cassandra Big-Ass ids solution
	- Records with primary key: [ doc_id, i, view_id|node_id, seq_num ]
		- NOTE: This will require one counter and one node id per cpu corerecoes
		- For servers, the  [ node_id, seq_num ] should be able to almost always stay below 128bits if we use 64bit serial node ids




Step 1: Single node rocksdb interface
Step 2: Replication: define protocol for sending operations to other servers
	- Consists of set and unset commands given string key and value
	- Must have a concept of
Step 3: Raft group for the db
	- For now having a static list of nodes
	- Elect a leader
	- Slaves should redirect requests to the master
	- TODO:


- Cassandra strong consistency
	- Write to majority of nodes
	- If we have 5 nodes, write to

- Cassandra with majority write will succeed in preventing duplicate ids (but it becomes problematic if one of the majority fails and there is no longer a majoriy)

- TODO: 128Mb

- Simplest way to thing about the new database would be to consider it to be like etcd but with support for
	- Raft must be used to have some idea of checkpointing data such that we know which

	- TODO: Building it based on raft won't be efficient unless we do multi-raft

	- TODO: We don't need to do as much for Raft because we only really care about picking the most up to date replica at the time of

	- TODO: Using for just leader election does not help us very much. Waiting for acks doesn't really help us because we would be doing that anyway

	- Only real benefit would be to have it be much faster than cockroachdb

	- Implementing pub-sub on top of cockroachdb



- What I am looking for from Cassandra is the hinted handoff feature
	- It will allow nodes to go down without missing data performed
	- TODO: Then the main question is: who should store the hint log?

- General idea is that when there is no serializable log per range, how do we ensure that everyone gets every change without using raft


- TODO: Nothing in this scheme would stop us from electing a node which is not part of the majority
	- Basically write a key, acknowledge from a majority (3/5), then  the master dies
	- Re-election would now be allowed to re-elect one of the 2 nodes that is behind


- TODO: If we are going to wait for an ACK, then we might as well use raft for sending out the replication requests


NOTE: Upon reader re-election, a node may need to rewind once it discoses that the
- TODO: If a master is re-elected, we must ensure that we don't mess up the timelining
	- If A received a change from time 1 to 3 from the old master
	- If master fails and elects C to be master,
	- C sends a mesage 1-3 and a message 3-4
		- If only the 3-4 message is seen by A, then we could get into trouble
		- TODO: We should attempt to not elect new masters that are very far behind
			- Basically no node that is still in the process of catching up with its log
	- To distinguish the time '3' timestamps, we need to prefix the timestamp by the
		- TODO: In raft the leader # is called a 'term'


brew install rocksdb
brew install zstd


CGO_CFLAGS="-I/usr/local/Cellar/rocksdb/5.12.4/include" \
CGO_LDFLAGS="-L/usr/local/Cellar/rocksdb/5.12.4/lib -lrocksdb -lstdc++ -lm -lz -lbz2 -lsnappy -llz4 -lzstd" \
go get github.com/tecbot/gorocksdb


*/
