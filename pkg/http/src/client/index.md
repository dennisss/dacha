

TODO: We need a better way to health check in the DirectClient (when active TCP connections are closed as idle). Implementations like Google use a lighter weight UDP side chnanel. 

Key edge cases:
- Receiving a 'Connection: close'
- Idle timeout
- TCP FIN received.
- HTTP2 graceful shutdown

/*
Tracking error state:
- V2:
    - We will basically always see a shutting_down event.
        - Mark which specific connection had the failure
    - When the runner returns, if it is still the only connection, it should be marked as a failure.
    - Until the backoff time elapses we won't attempt to start a new connection
        - In Failure state during backoff
    - Will use Connecting state once backoff has elapsed.
    - In all cases, the current state is well defined and can be stored as one mutex (augmented by in-flight limits)
    - Can just make the overall_state a Condvar.

    - If a connection failed, this to increment backoff but don't double count failures for a single connection.
        - As soon as shutdown starts, we will allow trying to start a new connection.

    - TODO: If the connection knows it is shutting down, print the error message at that time rather than waiting for the cleanup

- V1
    - TODO: Add a shutdown event to the v1 listener as well so that we can detect shutdown before connections are actually shutdown
        - Alternative is to detect that a connection is just not accepting connections so we can't use it.
        - Generally need to be able to signal
    - As soon as a connection fails, indicate that a failure occured.
        - At this point we may have dozens of other legit connections.
        - Wait for the connect_backoff
            - Until elapsed, stay in failed state
            -

So the connection runner:
- Check if we saw a failure.
    - If so, then schedule a backoff timeout
    - After the backoff timeout is done, we can go back to another state.
- Issue is that we could immediately start seeing more errors
    - e.g. the same connection could be completely shut down
    - Can just deduplicate errors for one connection
        - In the case of HTTP2, this is good enough
        - For HTTP1, other connections may see deferred errors as we don't do periodic pings.
            - Could
            - We can still send connections in
*/

/*
Managing the outstanding request count:

- We could intercept the body ourselves to add a tracker:
        - Doesn't work for the case ofa

For HTTP1 also perform exponential increase in the number of connections we are allowed to perform concurrently
- First 1
- Then 2
- Then 4
...

Easy to maintain state, but hard to maintain request counters:
- Decreasing the request count is not essential.
    - Can just sent an event to the other thread.
*/

/*
When we have a slot for a connection, we need to notify it.

Some other issues:
- Any already enqueued requests should have priority when it comes time

*/


TODOs:
- Settings to expose
    - DirectClient: Limit maximum amount of time to continue trying to maintain a connection. After that amount of time elapses without any requests received, stop trying to maintain a 
    - DirectClient: Support pooling of multiple requests
        - Also when an HTTP2 server wants to gracefully stop, start up a second connection while the first is closing.
            - Or should we just instead consider this a DirectClient failure and reconnect one layer up.
    - Timeout for max lifetime of a single server or client connection with/without activity.
    - In HTTP1 mode, limit the number of requests we make using a single connection.

// Important things:
// - If a connection rejects a request because the connection failed, the
//   connection should immediately start reporting that.

/*
General summary of the types of errors we could have:
- Connecting errors:
    - While creating the connection (HTTP upgrade, TLS handshake, HTTPv2 settings exchange) there was an error
    - These errors can either be:
    - Generally all these should be retryable (with backoff before trying the same backend again).

- While connection is running errors:
    - Connection just dies (not sure if things are retryable)
    - Connection gracefully dies ()

- Errors that just effect a single request: (these errors mean that the connection is probably still ok to use).
    - Tryivially retryable (because the server didn't do any processing)
    - Everything else


- Backend State
    - A candidate IP/host name based address to which we can send stuff is in one of the following states:
        - NO_CONECTION
        - CONNECTING
        - CONNECTED (IDLE)
        - ACTIVE : Requests are running on this connection
        - FAILED : A fatal error recently occured on this backend. Backoff before re-connecting.

- Will have a per-backend backend connection backoff
- Separately backoff for retrying requests (with a max number of retries).

TODO: When we see some types of errors like invalid TCP certificates, we could probably error out immediately without continuing to retry as these are usually rather permanent failrues.
    => If we are only dealing with a single target address, then it makes sense to error out immediately on these types of INVALID_ARGUMENT style errors.
    - For more than one address, it's important that we can recover in the long term (so at least we should continue to retry connecting if a new request comes along).

    ^ This is essentially why we want to have some form of 'fail_fast' even for regular HTTP clients.

Strategy:
=> Everything in new_connection() should trigger a REFUSED_STREAM?
*/

/*
General goal:
- Created rpc channels / http clients should not produce errors.
- But, when requesting something, we may propagate some types of errors.
    - Errors should generally only ever see

Errors we need to handle:
- Http level semantics with ProtocolErrorV2
- TLS errors?
    - e.g. TLS handshake refusal is retryable
- Important to know if the error occured before any processing occured.
*/

// TODO: The interesting thing about this function is that basically everything
// in it is retryable.
// - The main question is whether we expect to see permanent failures at the
//   connection layer?

// TODO: Extract all this single connection logic out of the http::Client

/*
Channel design:
- We will need to have a resolver trait that supports generating a list

- Each HTTP backend will be either:
    - HTTPv1: Rely on TCP keep alive.
        - Will start a new connection each time

- Tricky part is that unless we know want to delay connections to understand if

Strategies for

*/