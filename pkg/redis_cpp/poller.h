#ifndef POLLER_H_
#define POLLER_H_

#include <unistd.h>
#include <stddef.h>
#include <sys/event.h> // For kqueue
#include <sys/time.h>

#include <string.h>
#include <vector>
#include <algorithm>

#define EVENT_BUFFER_SIZE 16


enum PollerState {
	PollerStateReadable = 1,
	PollerStateReadEOF = 2, /**< When everything readable is currently buffered and there will be no more */
	PollerStateWritable = 3,
	PollerStateWriteEOF = 4, /**< When no more can be written (typically the receiving end has been closed) */
	PollerStateTimeout = 5 /**< Triggered when a timer expires. NOTE: The num returned is the id of the timer */
};

class PollerHandler {
public:
	/**
	 * Should respond to some events received about any file descriptors attached to the poller
	 * 
	 * @param num depends on the state
	 *            For Write, this is the number of bytes in the out buffer that can be written
	 *            For Read on listening sockets, this is the number of available connections
	 */
	virtual void handle(PollerState state, int num) = 0;

};

// https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
// Another good example: https://gist.github.com/josephg/6c078a241b0e9e538ac04ef28be6e787
class Poller {
public:
	Poller() { }

	int init() {
		/* Create kqueue. */
		kq = kqueue();
		if(kq == -1) {
			//err(EXIT_FAILURE, "kqueue() failed");
			return 1;
		}

		return 0;
	}

	// NOTE: Only read and write are supported
	// If data is null, we will 
	int toggle(int fd, PollerState state, PollerHandler *data) {
		struct kevent event;  /* Event we want to monitor */
		event.ident = fd;
		event.fflags = 0;
		event.data = 0;
		event.udata = data;

		/* Initialize kevent structure. */
		if(state == PollerStateReadable) {
			event.filter = EVFILT_READ;
		}
		else if(state == PollerStateWritable) {
			event.filter = EVFILT_WRITE;
		}
		else {
			return 1;
		}

		if(data != NULL) {
			event.flags = EV_ADD | EV_ENABLE;
		}
		else {
			event.flags = EV_DISABLE;
		}


		/* Attach event to the kqueue. */
		int ret = kevent(kq, &event, 1, NULL, 0, NULL);
		if(ret == -1) {
			//err(EXIT_FAILURE, "kevent register");
			return 1;
		}

		if(event.flags & EV_ERROR) {
			//errx(EXIT_FAILURE, "Event error: %s", strerror(event.data));
			return 1;
		}

		return 0;
	}

	// To top listening on a file descriptor
	int remove(int fd) {
		// TODO: We can clear by closing the fd
	}

	// See bsd timer example here: https://wiki.netbsd.org/tutorials/kqueue_tutorial/#index5h2
	// TODO: For timers it will be very useful to attach to specific functions
	// TODO: Ideally should 
	int setTimeout(int ms, PollerHandler *data, bool recurring = false) {
		int id = ++lastTimerId;

		// TODO: implement recurring
		// TODO: For a one time timeout, do we need to remove it from the queue after it expires

		struct kevent event;
		EV_SET(&event, 1, EVFILT_TIMER, EV_ADD | EV_ENABLE, NOTE_MSECONDS, 5000, data);

		int ret = kevent(kq, &event, 1, NULL, 0, NULL);
		if(ret == -1) {
			return 0;
		}

		if(event.flags & EV_ERROR) {
			return 0;
		}

		return id;
	}

	/**
	 * Stops a timeout given the id returned by setTimeout
	 */
	int clearTimeout(int id) {
		struct kevent event;
		EV_SET(&event, id, EVFILT_TIMER, EV_DELETE, 0, 0, NULL);

		int ret = kevent(kq, &event, 1, NULL, 0, NULL);
		if(ret == -1) {
			return 1;
		}

		if(event.flags & EV_ERROR) {
			return 1;
		}

		return 0;
	}


	int poll() {
		int i;
		struct kevent *te;
		struct kevent tevent[EVENT_BUFFER_SIZE]; /* Event triggered */

		struct timespec timelimit;
		timelimit.tv_sec = 1;
		timelimit.tv_nsec = 0;


		/* Sleep until something happens. */
		int ret = kevent(kq, NULL, 0, tevent, 16, &timelimit);
		if(ret == -1) {
			//err(EXIT_FAILURE, "kevent wait");
			return 1;
		}
		else if(ret == 0) {
			return 0;
		}

		// TODO: We may want to provide some protection for handlers than are already interally closing themeselves from receiving more events
		for(i = 0; i < ret; i++) {
			te = &tevent[i];

			PollerHandler *handler = (PollerHandler *) te->udata;

			int data = te->data;

			if(te->filter == EVFILT_READ) {
				handler->handle(PollerStateReadable, data);

				// TODO: Make sure that we remove the listener for this immediately afterwards
				if(te->flags & EV_EOF) {
					handler->handle(PollerStateReadEOF, data);
				}
			}
			else if(te->filter == EVFILT_WRITE) {
				handler->handle(PollerStateWritable, data);

				if(te->flags & EV_EOF) {
					handler->handle(PollerStateWriteEOF, data);
				}
			}
			else if(te->filter == EVFILT_TIMER) {
				handler->handle(PollerStateTimeout, te.ident);
			}
		}

		return 0;
	}

	


private:
	int kq;
	int lastTimerId = 0;

};


/**
 * Intended for wrapping a single file descriptor and will close itself once the write end has closed
 * ^ This closing behavior is somewhat hardcoded for the purpose of usage with a TCP connection which is closed by the client and not the server
 */
class IOHandler : PollerHandler {
public:
	IOHandler(Poller *ctx, int fd) {
		this->ctx = ctx;
		this->fd = fd;
		this->closing = false;
		ctx->toggle(fd, PollerStateWritable, this); // We request an initial event just so that we can see the size of the output buffer (in order to optimize the first write)
		ctx->toggle(fd, PollerStateReadable, this);

		// TODO: Next step would be to make a buffer linked list to reduce the complexity of resizing the buffer
		// I'd rather always 
		// TODO: Ideally make each block a 4096 byte memory page aligned buffer always keep at least one buffer and attempt to use the 
		write_buf.reserve(1024);
	}

	virtual ~IOHandler() {};

	virtual int handle_readable() = 0;
	
	void close() {
		this->closing = true;
		//ctx->toggle(fd, PollerStateReadable, NULL);
		// NOTE: All poller events will be cleared by the close()
		::close(fd);
		delete this;
	}

	void handle(PollerState state, int num) {
		if(closing) {
			return;
		}

		if(state == PollerStateReadable) {
			// TODO: It would be useful to test for eof incase we don't need to read anything
			handle_readable();
		}
		else if(state == PollerStateReadEOF) {
			ctx->toggle(fd, PollerStateReadable, NULL);
		} 
		else if(state == PollerStateWritable) {
			if(flush_writes(num) < 0) {
				// Error flushing writes
			}
		}
		else if(state == PollerStateWriteEOF) {
			//cout << "Client connection was closed" << endl;
			if(write_buf.size() == 0) {
				this->close();
			}

			// TODO: At this point, we should be waiting for writes to finish and then we will be able to close			
		}
	}


	int write(const char *buf, int len) {
		int nwritten = 0;

		// If we can write, then we will write immediately
		if(write_buf.size() == 0 && write_avail > 0) {

			int res = ::write(fd, buf, std::min(write_avail, len));
			if(res < 0) {
				return res;
			}

			nwritten += res;
			len -= res;
			buf += len;
		}

		if(len == 0) {
			return nwritten;
		}


		// Otherwise, we will buffer it for later
		int off = write_buf.size();
		write_buf.resize(off + len);
		memcpy(&write_buf[off], buf, len);
		nwritten += len;

		// If this is the first amount of buffered data, we will enable the write poller
		if(off == 0) {
			if(ctx->toggle(fd, PollerStateWritable, this)) {
				return -1;	
			}
		}

		return nwritten;
	}

	int fd;

private:

	int flush_writes(int n) {
		this->write_avail = n;
		if(write_buf.size() > 0 && n > 0) {

			int res = ::write(fd, &write_buf[0], std::min(n, (int) write_buf.size()));
			if(res < 0) {
				return res;
			}

			write_avail -= res;

			// The efficieny of this depends on the 
			if(res == write_buf.size()) {
				write_buf.clear();
				//write_buf.resize(0);
			}
			else {
				write_buf.erase(write_buf.begin(), write_buf.begin() + res);
			}

			return res;
		}

		// Once the buffer is flushed, we can stop listening
		if(write_buf.size() == 0) {
			if(ctx->toggle(fd, PollerStateWritable, NULL)) {
				return -1;
			}
		}
		
		return 0;
	}


	Poller *ctx;

	bool closing;

	int write_avail = 0;
	// TODO: If we limit the minimum buffer size, then we can make this an efficient ring buffer
	std::vector<char> write_buf;

};



#endif