# Cluster Frontend Service

This directory contains the code for the frontend binary which handles serving of traffic received over public domains.

The goals of the frontend are mainly the following:

1. Exposing cluster services over public DNS (serving a public TLS certificate that proxies to internal TLS based servers).
2. Supporting clients without mTLS (by supporting username/password login converted to web cookies).

Note that the frontend job assumes that a single HTTP/TLS connection is coming from a single end user device. This is used to cache login credentials.

## DOS Protections

Since this service on the directly exposed to the internet, we aim to high strong rejection of overload from abusive clients. We can't realistically protect against a massive bot net attack, but attack from just a few IPs should be mitigatable. This is mainly implemented as follows:

- Local token bucket based usage limiting.
    - Roughly each IP is given 1000 tokens ever ten seconds that it can use to perform actions.
    - Before performing an action, we deduct some number of tokens from the IP's budget and return an error if there are insufficient tokens.
        - (must be done before the ation since we cancel operations if the client terminates the connection)
    - Actions (and their costs) are the following:
        - Start a new TCP stream : 50
        - One of these actions for each request:
            - Authenticate a new `Auth-Key` : 25
            - Start an HTTP request (unauthenticated) : 10
            - Start an HTTP request (authenticated) : 1
- Concurrency limits
    - Up to 1 `Auth-Key` token can be validated per connection at a time.
- Request timeouts
    - TODO: Implement this.
    - We will limit the maximum time spent on requests as measured from start to last byte written in the body/trailer.
    - Limits
        - Unauthenticated Requests: Max 5 seconds.
        - Authenticated Requests: Max 60 seconds
- Connection limits
    - Up to 10,000 connections can be active on a server at a time. After that we blindly reject new ones.
    - Each connection must complete a TLS handshake within 2 seconds or get terminated.
    - Only HTTP2 is allowed.
    - Maximum number of active connections per peer IP is limited to ~16.
    - Maximum number of active requests (HTTP2 streams) is limited to 16 (via the MAX_CONCURRENT_STREAMS HTTP2 setting).
    - TODO: Timeout idle connections (those with no active requests) in the HTTP server to 5 minutes.