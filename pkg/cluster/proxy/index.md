# Proxy Service

This is an RPC service that implements a Layer 4 proxy.

Example:

```
# Start a target server on plain HTTP port 9000
cargo run --bin http_server -- --port=9000

# Start a proxy server listening on RPC port 8000
cargo run --bin cluster_proxy_server -- --port=8000

# Start a TCP server on port 8001 which forwards traffic to TCP port 9000 via the proxy server.
cargo run --bin cluster_proxy_forwarder -- --local_port=8001 --server_addr=localhost:8000 --target_addr=127.0.0.1:9000

# Testing
curl -v http://127.0.0.1:8001
curl -v http://127.0.0.1:8001 -H "Connection: close"

```

There is no authentication from a client to the forwarded aside from non-local IP address peers being blocked. From the forwarder to the server, standard cluster owner credentials are required and validated with mTLS.

## FAQ

- Why not make a Layer 3 proxy?
    - Requires using Linux raw sockets which requires root access.
    - More implementation work since this will require NAT translation of addresses and port numbers.
- why not use the Wireguard protocol?
    - Requires having a single crypto and client authentication implementation.
    - Wireguard UDP is lower overhead but UDP has a harder time with NAT/firewall traversal. Meanwhile RPC over HTTPS should pretty much always work.
