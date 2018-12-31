#include "redis.h"

#include <fcntl.h>
#include <sys/types.h>
#include <sys/socket.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
#include <poll.h>
#include <stdint.h>

#include <iostream>
using namespace std;


#define TCP_CONNECTION_BACKLOG_SIZE 64


RedisServer::RedisServer(Poller *ctx, rocksdb::DB *db) {
	this->ctx = ctx;
	this->db = db;
	fd = -1;
}

RedisServer::~RedisServer() {
	if(fd >= 0) {
		close(fd);
		fd = -1;
	}
}

int RedisServer::listen(int port) {
	if((fd = socket(AF_INET, SOCK_STREAM, 0)) < 0) {
		perror("cannot create socket");
		return -1;
	}

	#ifdef SO_REUSEPORT
	int reuse = 1;
	if(setsockopt(fd, SOL_SOCKET, SO_REUSEPORT, &reuse, sizeof(int)) == -1){
		printf("Failed to make socket reusable\n");
		exit(1);
		return -1;
	}
	#endif


	struct sockaddr_in addr;
	memset((char *)&addr, 0, sizeof(addr));
	addr.sin_family = AF_INET;
	addr.sin_addr.s_addr = htonl(INADDR_ANY);
	addr.sin_port = htons(port);

	if(::bind(fd, (struct sockaddr *) &addr, (socklen_t) sizeof(addr)) < 0) {
		perror("bind failed");
		close(fd);
		return -1;
	}

	if(::listen(fd, TCP_CONNECTION_BACKLOG_SIZE) != 0) {
		perror("listen failed");
		close(fd);
		return -1;
	}

	int flags = fcntl(fd, F_GETFL, 0);
	if(fcntl(fd, F_SETFL, flags | O_NONBLOCK)) {
		perror("failed to make server socket non-blocking");
		close(fd);
		return -1;
	}

	
	if(ctx->toggle(fd, PollerStateReadable, this)) {
		cout << "failed to add server to poller" << endl;
		close(fd);
		return -1;
	}

	return 0;
}

void RedisServer::handle(PollerState state, int num) {
	if(state == PollerStateReadEOF) {
		cout << "Server closed" << endl;
	}
	else if(state == PollerStateReadable) {

		struct sockaddr_in addr;
		socklen_t addrlen = sizeof(struct sockaddr_in);

		for(int i = 0; i < num; i++) {
			int conn_fd = accept(fd, (struct sockaddr *) &addr, &addrlen); //, SOCK_NONBLOCK);

			if(conn_fd < 0) {
				// This typically means that for some reason, the number we were given was incorrect
				perror("failed to accept");
				return;
			}

			int flags = fcntl(conn_fd, F_GETFL, 0);
			if(fcntl(conn_fd, F_SETFL, flags | O_NONBLOCK)) {
				perror("failed to make client socket non-blocking");
				close(conn_fd);
				continue;
			}

			int nodelay = 1;
			if(setsockopt(conn_fd, IPPROTO_TCP, TCP_NODELAY, &nodelay, sizeof(int)) == -1){
				printf("Failed to set no delay\n");
				close(conn_fd);
				continue;
			}


			// TODO: Also configure TCP_KEEPALIVE

			new RedisConnection(ctx, this, conn_fd);
		}
	}
}
