# Cluster Bridging Utilities

These are utilities for using non-cluster aware networked programs with the cluster. During cluster creation/login, it is installed as a local systemd service to continously run in the background.

The main driving usecase is for allowing a user to go to a `*.cluster.internal` domain in their web browser and have that just work.

Note that the bridging will add some overhead so most applications should prefer using the native cluster networking utilities rather than going using standard DNS/TCP over the bridge.

## Security

The bridge currently runs as a single user and is accessible by any clients with ip 127.0.0.x. The only information that is exposed is cluster name resolution data so we assume that is not very sensitive. No privileges are granted to traffic that is passed through the bridge.

## Life of a request

When Google Chrome (on a Linux desktop) visits a url like `my-worker.worker.zone.cluster.internal:port`:

1. It will use the standard system utilities for performing DNS (`getaddrinfo` / `gethostbyname`)
1. These read the `hosts` section of `/etc/nsswitch.conf` which specifies a sorted list of options to try
    - By default, this first tries `/etc/hosts`, then mDNS, then normal DNS (`dns`)
1. For cluster addresses, we will hit `dns` which looks in `/etc/resolv.conf` to find the name server
    - This by default points to `127.0.0.53` which is the locally running `systemd-resolved` server.
1. We will have a custom `resolved` config in `/etc/systemd/resolved.conf.d/dacha-cluster.conf` which tells the service to first try the nameserver `127.0.0.80` before trying network interface specific DNS.
1. The cluster bridge service will be listening to UDP DNS packets on `127.0.0.80:53` and will get the request
    - It will return a NXDOMAIN ('not found') error for any non `.cluster.internal` names.
    - For cluster addresses, it will return an `A 127.0.0.80` record
1. The cluster bridge service will be listening for TCP traffic on `127.0.0.80:80` and `127.0.0.80:443`
    - Port 80 HTTP requests will be redirected to port 443.
    - For port 443 requests, we will sniff the host name from the TLS ClientHello packet and transparently redirect the all packets on the TCP stream to an appropriate worker in the cluster.
