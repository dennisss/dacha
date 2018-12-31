#include <iostream>
using namespace std;

#include "poller.h"
#include "redis.h"
#include "trie.h"

#include <cassert>

#include "rocksdb/cache.h"
#include "rocksdb/compaction_filter.h"
#include "rocksdb/options.h"
#include "rocksdb/slice.h"
#include "rocksdb/table.h"
#include "rocksdb/utilities/options_util.h"
#include "rocksdb/filter_policy.h"
#include "rocksdb/slice_transform.h"

rocksdb::DB *db;


class DataStoreNumberSet {
public:

	DataStoreNumberSet(rocksdb::DB *db) {
		this->db = db;
	}

private:
	rocksdb::DB *db;

};


bool running = true;
void signal_handler(int sig) {
	running = false;
}



/*

Benchmarking
------------

Plain REDIS:
redis-benchmark -t set,get -n 100000 -q

With snapshoting:
SET: 49480.46 requests per second
GET: 45495.91 requests per second
Completely in-memory:
SET: 52493.44 requests per second
GET: 50787.20 requests per second

Our implementation
redis-benchmark -t set,get -n 100000 -q -p 6381

SET: 22951.57 requests per second
GET: 25733.40 requests per second

Without the WAL
SET: 47892.72 requests per second
GET: 34977.27 requests per second

NOTE: It gets even faster with -O3 and TCP_NODELAY
*/



int main(int argc, const char *argv[]) {

	/*
	const char *test = "*1\r\n$7\r\nCOMMAND\r\n";
	const char *test2 = "*1\r\n$5\r\nHELLO\r\n";

	RESPParser parser;
	auto r = parser.parse(test, strlen(test));
	cout << r << endl;

	auto s = parser.status();
	cout << s << endl;

	parser.grab();

	// Making sure that we can parse twice

	r = parser.parse(test2, strlen(test2));
	cout << r << endl;

	s = parser.status();
	cout << s << endl;

	return 0;
	*/

	Poller ctx;
	if(ctx.init()) {
		cout << "failed to init poller" << endl;
		return -1;
	}


	cout << "Opening database" << endl;
	rocksdb::Options options;

	/*
	// In memory settings
	options.allow_mmap_reads = true;
	options.max_open_files = -1;

	
	rocksdb::BlockBasedTableOptions table_options;
	table_options.filter_policy.reset(rocksdb::NewBloomFilterPolicy(10, true));
	table_options.no_block_cache = true;
	table_options.block_restart_interval = 4;
	options.table_factory.reset(rocksdb::NewBlockBasedTableFactory(table_options));

	options.compression = rocksdb::CompressionType::kNoCompression;
	*/

	// Hash tables are much faster for doing in memory stuff
	
	options.prefix_extractor.reset(rocksdb::NewFixedPrefixTransform(1));
    options.memtable_factory.reset(rocksdb::NewHashSkipListRepFactory(1024));
    options.allow_concurrent_memtable_write = false;
	options.compression = rocksdb::CompressionType::kNoCompression;
	

	//options.memtable_factory.reset(rocksdb::NewHashSkipListRepFactory(1024));
	options.write_buffer_size = 1024*1024*512;
	options.max_write_buffer_number = 5;
	options.min_write_buffer_number_to_merge = 2;
	

	options.manual_wal_flush = true;

	options.create_if_missing = true;
	rocksdb::Status status = rocksdb::DB::Open(options, "/Volumes/Data/datalayer", &db);
	assert(status.ok());


	cout << "Starting server" << endl;
	RedisServer::Init();
	RedisServer server(&ctx, db);
	if(server.listen(6381)) {
		cout << "failed to start server" << endl;
		return 1;
	}

	running = true;
	signal(SIGINT, signal_handler);


	cout << "Polling for connections" << endl;
	int res;

	// Poll for messages
	while(running) {
		res = ctx.poll();
		if(res) {
			cout << "poller failed" << endl;
			break;
		}
		// TODO: Return value
	}


	delete db;

	return 0;

	
	/*
	int num = 0;

	for(int i = 0; i < 100; i++) {

		rocksdb::Iterator* it = db->NewIterator(rocksdb::ReadOptions());
		
		for(it->SeekToFirst(); it->Valid(); it->Next()) {
			num++;
			//cout << it->key().ToString() << ": " << it->value().ToString() << endl;
		}
		
		assert(it->status().ok()); // Check for any errors found during the scan
		delete it;


	}
	
	cout << "Iterated over " << num << " keys" << endl; 
	*/

}