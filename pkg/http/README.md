# HTTP Client and Server

## References

- [HTTP 1.1](https://tools.ietf.org/html/rfc7230)
- [URI](https://tools.ietf.org/html/rfc3986)
- [URL](https://tools.ietf.org/html/rfc1738)
- [HPACK](https://tools.ietf.org/html/rfc7541)

Dealing with obs-fold:
- We will replace any '\r' or '\n' characters in the the value sent be a client or received by a server with spaces.

## Client/Server Semantics

The Client and Server implementations are built with robustness in mind rather than being
completely 'raw' interfaces. To accomplish this, the logic for a number of transport related
or compatibility related (HTTP v1/v2) features are implemented centrally in the Client/Server
internal code to avoid client error and hopefully simplify the development process for the common
case of conforming to all the RFC specifications. These semantics are described in the following
sections. The TLDR for the user's perspective is that the following headers should never be
included directly in request/response:

- `Transfer-Encoding`
- `TE` (TODO)
- `Content-Length`
- `Host`
- `Date`


### Content-Length

Both the provided Client and Server implementations will error out if a `Content-Length` header is
given in an local user created request/response. Instead, when sending an outgoing body, users
must make sure that the `http::Body::len()` method returns the correct value. The `Content-Length`
header will be added automatically based on this value.

### HEAD method

When a Server request handler receives a request with the HEAD method, it should operate
equivalently to if it received a GET request. As mentioned in the previous section, the
Server implementation will derive the `Content-Length` appropriately, but won't send any data
back to the client regardless of the data in the response `http::Body`.

### Host

For a client, a host/authority will first be passed when calling `Client::create(_)`. This host
name will be what will be used for connection initialization (DNS lookup, TLS handshake if applicable,
etc.). Later when constructing a request to send using a `RequestBuilder`, the user is NOT allowed
to specify a `Host` header via the `.header()` function of the builder. It is recommended that the
user use the `.path()` method to set a relative path (in which case the host will be derived from
the name given to `Client::create()`). Alternatively `.uri()` or `.host()` can be used to set a
host. In all cases the `Host` header will be generated automatically (or a `:authority` header in
the case of HTTP 2).

For a server, the raw `Host` header will be visible to the request handler, but the Server should
prefer to read the `request.uri.authority` field which will contain the normalized value and
account for the lack of a `Host` header in HTTP 2.

TODO: Refactor `.path` to only allow for relative paths (path-absolute and query: https://datatracker.ietf.org/doc/html/rfc7540#section-8.1.2.3)

### Client vs Connection

The internal `*Connection` structs represent single TCP/UDP connections to a remote server. They
can be used to perform multiple requests, but it comes for some limitations:

- For HTTP 1.x requests, requests/responses can't be multiplexed (will block on the head of line
  request).
- When the connection fails, it can no longer be used to perform further requests and won't perform
  any type of retrying.


The `Client` interface is meant to solve the above problems be acting as a pool or at least one
active connection at a time and handling single connection failures as gracefully as is reasonable.
A user should expect it to always be valid to send a request using a `Client` (it is never in a failed
state), but the `Client` interface is limited to exactly one target address.

### Header Folding

Before HTTP 1.1 RFC, HTTP header values could contain `\r` or `\n` characters (via the `obs-fold`
production rule). Per the 1.1 RFC recommendation, both the Client and Server interfaces will
replace any occurences of these characters with regular space characters before transmitting or
giving back to the interface user.

TODO: Make sure that this happens in HTTP 2

### TODOs

- Need to perform URI normalization on the client and server side (e.g. '/../../' to '/')
- Need to ensure that the path in an HTTP(S) request is never empty (instead should be '/').
- Proxy functionality
- Retrying of REFUSED_STREAM in HTTP2 or idempotent methods
 

## Old


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