# HTTP Client and Server

## References

- [HTTP 1.1](https://tools.ietf.org/html/rfc7230)
- [URI](https://tools.ietf.org/html/rfc3986)
- [URL](https://tools.ietf.org/html/rfc1738)
- [HPACK](https://tools.ietf.org/html/rfc7541)

Dealing with obs-fold:
- We will replace any '\r' or '\n' characters in the the value sent be a client or received by a server with spaces.



Old
---

- HTTP 1.1
	- Supports persistent connections
		- On significant errors, the connection will be closed.
	- Pipelining is not explicitly supported.


TODO: FTP: ftp://prep.ai.mit.edu/pub/gnu/

TODO: Deflate: https://tools.ietf.org/html/rfc1951

TODO: Cookie jar

- Usually I don't need fancy traits implementing futures
	- So usually relatively simple
	- If I did need it, then I could just implement 

For body, we will implement a Box<Future> from read so that I don't need to implement this stuff

- Other stuff:
	- How to incrementally read
	- 


Hop-to-hop headers are generally disallowed in user provided requests/responses
- These will be derived based on the given body internally.

TODO: Once any gzip data has been read, we must ensure that we are then at the end of the stream if appropriate.