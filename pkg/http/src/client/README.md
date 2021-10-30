


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