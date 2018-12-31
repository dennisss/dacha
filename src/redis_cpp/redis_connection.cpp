
#include "redis.h"
#include "trie.h"

#include <sstream>

using namespace std;


static Trie<RedisCommandEntry> RedisCommandStore;

static const RedisCommandEntry RedisCommandsList[] = {
	{ .name = "GET", .minArgs = 1, .maxArgs = 1, .handler = &RedisConnection::run_command_get },
	{ .name = "SET", .minArgs = 2, .maxArgs = 4, .handler = &RedisConnection::run_command_set },
	{ .name = "COMMAND", .minArgs = 0, .maxArgs = 0, .handler = &RedisConnection::run_command_command },
	{ .name = "SUBSCRIBE", .minArgs = 1, .maxArgs = 4096, .handler = &RedisConnection::run_command_subscribe, .allowInSubMode = true },
	{ .name = "UNSUBSCRIBE", .minArgs = 0, .maxArgs = 4096, .handler = &RedisConnection::run_command_unsubscribe, .allowInSubMode = true },
	{ .name = "PUBLISH", .minArgs = 2, .maxArgs = 2, .handler = &RedisConnection::run_command_publish }
};


void RedisServer::Init() {
	int n = sizeof(RedisCommandsList) / sizeof(RedisCommandEntry);
	
	for(int i = 0; i < n; i++) {
		RedisCommandStore.add(RedisCommandsList[i].name, &RedisCommandsList[i]);
	}
}




int RedisConnection::handle_readable() {

	int res;
	char buf[512];
	
	while(true) {
		res = read(fd, buf, sizeof(buf));
		if(res == 0) {
			// Eof (we will wait for all memory to write before proceeding)
			return 0;
		}
		else if(res < 0) {
			// probably a nonblocking error
			return 0;
		}

		// Call to parser
		int n = 0;
		while(n < res) {
			int ni = parser.parse(buf + n, res - n);
			if(ni == 0) {
				// Nothing was parsed: This should never happen
				close();
				return 0;
			}

			auto stat = parser.status();
			if(stat == RESPParserStatusInvalid) {
				close();
				return 0;
			}
			else if(stat == RESPParserStatusDone) {
				if(this->run_command(parser.grab())) {
					return 0;
				}
			}

			n += ni;
		}			
	}

	return 0;
}

int RedisConnection::run_command(std::shared_ptr<RESPObject> pObj) {
	// TODO: Block if a command is already running

	#define WRITE_ERROR(s) write(RESPErrorConst(s), RESPConstLen(RESPErrorConst(s)))

	if(pObj->type != RESPTypeArray) {
		write(RESP_ERROR_NOT_ARRAY, RESPConstLen(RESP_ERROR_NOT_ARRAY));
		return 0;
	}

	RESPArray *aObj = (RESPArray *) pObj.get();
	if(aObj->items.size() == 0) {
		write(RESPOKConst, RESPConstLen(RESPOKConst));
		return 0;
	}

	for(int i = 0 ; i < aObj->items.size(); i++) {
		if(aObj->items[i]->type != RESPTypeBulkString) {
			WRITE_ERROR("Expected request to be an array of bulk strings");
			return 0;
		}
	}

	
	RESPBuffer *cmdObj = (RESPBuffer *) aObj->items[0];
	char *cmd = &cmdObj->str[0];
	int cmdlen = cmdObj->str.size();

	// Commands are case insensitive
	for(int i = 0; i < cmdlen; i++) {
		cmd[i] = toupper(cmd[i]);			
	}

	const RedisCommandEntry *entry = RedisCommandStore.get(cmd, cmdlen);
	if(entry == NULL) {
		stringstream ss;
		ss << "-";
		ss << "unknown command '";
		ss.write(cmd, cmdlen);
		ss << "'";
		ss << "\r\n";

		string str = ss.str();
		write(str.c_str(), str.size());
		return 0;
	}

	int argc = aObj->items.size() - 1; // <- including the command
	auto args = argc > 0? &aObj->items[1] : NULL;

	if(argc < entry->minArgs || argc > entry->maxArgs) {
		stringstream ss;
		ss << "-";
		ss << "wrong number of arguments for '";
		ss.write(cmd, cmdlen);
		ss << "' command";
		ss << "\r\n";

		string str = ss.str();
		write(str.c_str(), str.size());
		return 0;
	}

	if(subsriberMode && !entry->allowInSubMode) {
		WRITE_ERROR("not allowed in subscription mode"); // TODO: Check what redis would do
		return 0;
	}


	(this->*(entry->handler))(argc, (RESPBuffer **) args);
}

/*
	To supoprt lists, we must support encoding the type in the buffer
	One key must be reserved just for 

	PubSub list encoding in KV Store:
	- [BaseKey][UniqueNodeId]
	- Publish becomes a traversal of this list followed by a network request to each node
		- If multiple nodes are in a separate region, randomly pick one know to probably be alive
		- ^ Then send it a request along with a plea to forward it to all nodes in that region

	- Before any of this, we need to have at least single raft consensus going for us

*/

void RedisConnection::run_command_get(int argc, RESPBuffer *args[]) {
	auto keyObj = args[0];
	char *key = &keyObj->str[0];
	int keylen = keyObj->str.size();


	// NOTE: slices have a .data() and a .size()

	//auto iter = mem_db.find()


	rocksdb::PinnableSlice val; // TODO: Do I need to clean this up in any way
	rocksdb::Status s = server->db->Get(rocksdb::ReadOptions(), server->db->DefaultColumnFamily(), rocksdb::Slice(key, keylen), &val);
	if(!s.ok()) {
		if(s.IsNotFound()) {
			write(RESPNilConst, RESPConstLen(RESPNilConst));
			return;
		}

		WRITE_ERROR("Failed to get key");
		return;
	}

	// This will force all of the below data to batch together
	//write_avail = 0;

	// TODO: Ideal case we would write this directly into our output buffer
	stringstream ss;
	ss << "$"; // RESPTypeBulkString;
	ss << val.size();
	ss << "\r\n";
	ss.write(val.data(), val.size()); // TODO: This is a possibly expensive copy (no mainly do it right now because of TCP_NODELAY)
	ss << "\r\n";

	string header = ss.str();
	write(header.c_str(), header.size());

	//write(val.data(), val.size());
	//write("\r\n", 2);
}

void RedisConnection::run_command_set(int argc, RESPBuffer *args[]) {
	auto keyObj = args[0];
	char *key = &keyObj->str[0];
	int keylen = keyObj->str.size();

	auto valObj = args[1];
	char *val = &valObj->str[0];
	int vallen = valObj->str.size();

	
	auto opts = rocksdb::WriteOptions();
	opts.disableWAL = true;

	rocksdb::Status s = server->db->Put(opts, rocksdb::Slice(key, keylen), rocksdb::Slice(val, vallen));

	if(!s.ok()) {
		WRITE_ERROR("Failed to set key");
		return;
	}

	write(RESPOKConst, RESPConstLen(RESPOKConst));

}

void RedisConnection::run_command_command(int argc, RESPBuffer *args[]) {
	write(RESPOKConst, RESPConstLen(RESPOKConst));
}

void RedisConnection::run_command_subscribe(int argc, RESPBuffer *args[]) {
	const char subscribe_str[] = "subscribe";

	subsriberMode = true;

	for(int i = 0; i < argc; i++) {
		// Response after each individual subscription is:
		// Array( BulkString("subscribe"), BulkString(roomName), Int(totalSubs) )

		auto roomObj = args[i];
		char *room = &roomObj->str[0];
		int roomlen = roomObj->str.size();

		string room_str(room, roomlen);

		bool found = false;
		for(int j = 0; j < rooms.size(); j++) {
			if(rooms[j] == room_str) {
				found = true;
				break;
			}
		}

		if(!found) {
			rooms.push_back(room_str);

			// Add to global list of rooms
			auto connvec_it = server->allRooms.find(room_str);
			if(connvec_it == server->allRooms.end()) {
				server->allRooms[room_str] = vector<RedisConnection *>();
				connvec_it = server->allRooms.find(room_str);
			}

			connvec_it->second.push_back(this);
		}


		// TODO: Heap just for the 
		// TODO: Consider using a heap for the rooms list

		stringstream ss;
		ss << "*3\r\n";
		ss << "$" << sizeof(subscribe_str) - 1 << "\r\n" << subscribe_str << "\r\n";
		ss << "$" << roomlen << "\r\n" << room_str << "\r\n";
		ss << ":" << rooms.size() << "\r\n";

		string str = ss.str();
		write(str.c_str(), str.size());
	}
}

void RedisConnection::run_command_unsubscribe(int argc, RESPBuffer *args[]) {
	const char unsubscribe_str[] = "unsubscribe";

	bool unsubscribeAll = false;
	if(argc == 0) {
		unsubscribeAll = true;
		argc = rooms.size();
	}

	// TODO: 
}

void RedisConnection::run_command_publish(int argc, RESPBuffer *args[]) {
	const char message_str[] = "message";

	auto roomObj = args[0];
	char *room = &roomObj->str[0];
	int roomlen = roomObj->str.size();
	string room_str(room, roomlen);

	auto valueObj = args[1];
	char *value = &valueObj->str[0];
	int valuelen = valueObj->str.size();

	int nsent;
	auto connvec_it = server->allRooms.find(room_str);
	if(connvec_it == server->allRooms.end()) {
		// No one to send it to
	}
	else {
		stringstream msg_ss;
		msg_ss << "*3\r\n";
		msg_ss << "$" << sizeof(message_str) - 1 << "\r\n" << message_str << "\r\n";
		msg_ss << "$" << roomlen << "\r\n" << room_str << "\r\n";
		msg_ss << "$" << valuelen << "\r\n";
		msg_ss.write(value, valuelen);
		msg_ss << "\r\n";

		string msg_str = msg_ss.str();

		const vector<RedisConnection *> &connvec = connvec_it->second;
		for(int i = 0; i < connvec.size(); i++) {
			RedisConnection *conn = connvec[i];
			conn->write(msg_str.c_str(), msg_str.size());
		}

		nsent = connvec.size();
	}

	stringstream ss;
	ss << ":" << nsent << "\r\n";

	string str = ss.str();
	write(str.c_str(), str.size());
}