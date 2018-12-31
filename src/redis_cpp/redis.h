#ifndef REDIS_H_
#define REDIS_H_

#include "redis_resp.h"
#include "poller.h"
#include "db.h"

#include <map>
#include <vector>
#include <string>


#define RESP_ERROR_NOT_ARRAY RESPErrorConst("Command is not an array")

#define BUFFER_SIZE 4096


class RedisConnection;

class RedisServer : PollerHandler {
public:
	/**
	 * Call before using the server to initialize 
	 */
	static void Init();

	RedisServer(Poller *ctx, rocksdb::DB *db);

	~RedisServer();

	int listen(int port);

	void handle(PollerState state, int num);

private:
	friend class RedisConnection;

	rocksdb::DB *db;

	// Which clients are on which rooms
	std::map<std::string, std::vector<RedisConnection *>> allRooms;

	Poller *ctx;
	int fd;
};


// NOTE: For a pubsub wire reference see: https://redis.io/topics/pubsub
// TODO: We must make sure to only have a single global trie 
// TODO: Most of this can be generlized as an IOServer
class RedisConnection : IOHandler {
public:

	RedisConnection(Poller *ctx, RedisServer *server, int fd) : IOHandler(ctx, fd) {
		this->server = server;
	}

	// TODO: On close we must unsubscribe from all rooms

	int handle_readable();

	int run_command(std::shared_ptr<RESPObject> pObj);

	void run_command_get(int argc, RESPBuffer *args[]);
	void run_command_set(int argc, RESPBuffer *args[]);
	void run_command_command(int argc, RESPBuffer *args[]);
	void run_command_subscribe(int argc, RESPBuffer *args[]);
	void run_command_unsubscribe(int argc, RESPBuffer *args[]);
	void run_command_publish(int argc, RESPBuffer *args[]);
	

private:
	RESPParser parser;

	RedisServer *server;

	// List of all rooms that this connection is subscribed to
	bool subsriberMode = false;
	std::vector<std::string> rooms;
};


// NOTE: For convenience, args and argc do NOT include the command itself
typedef void (RedisConnection::*RedisCommandHandler)(int argc, RESPBuffer *args[]);

struct RedisCommandEntry {
	const char *name;
	int minArgs;
	int maxArgs;
	RedisCommandHandler handler;
	bool allowInSubMode = false;
};



#endif