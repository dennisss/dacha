# Cluster Ingress

This guide describes how to expose cluster services on the public web.

For this guide, we assume that you already own a public domain (e.g. `example.com`).

## Setup

- Create/reuse a [Google Cloud Project](https://console.cloud.google.com/).
    - You will need 
- Go to the `Cloud DNS` page in the console and Enable the API if not already enabled.
- Create a DNS Zone
    - Type: `Public`
    - Zone name can be anything
    - DNS name: Set to the domain name (e.g. `example.com`)
    - DNSSEC: `On`
- Hit the `Registrar setup` button at the top and copy the shown records into your domain provider.
- Go to the `Service Accounts` page and create/edit a service account which has the the `DNS Administrator` role under permissions.
- Create a new JSON format key for the service account and download it.
- Store the key as a key in your cluster's metastore db:
    - `cargo run --bin cluster_cli -- set_object --path=google_service_account path/to/key.json`
- Start a job which will generate TLS credentials for your domain:
    - Update the `--dns_name` flag in the `pkg/cluster/acme/config/letsencrypt_prod_refresher.job` file to reference your domain.
        - TODO: Make this file templated.
    - Then start the job with `cargo run --bin cluster_cli -- start_job pkg/cluster/acme/config/letsencrypt_prod_refresher.job`
- Start the auth job:
    - `cargo run --bin cluster_cli -- start_job pkg/cluster/config/auth.job`
- Start the frontend job:
    - `cargo run --bin cluster_cli -- start_job pkg/cluster/config/frontend.job`
- Setup port forwarding on your home router
    - You need to forward port 443 on your router to the ip:port for the frontend job.
    - You can find the ip:port by running `cargo run --bin cluster_cli -- list workers`
    - TODO: Automate this step.
- Set up DNS records to point your domain to your network
    - Update the `--dns_name` flag in `pkg/cluster/config/public_dns_refresher.job`.
    - Then run `cargo run --bin cluster_cli -- start_job pkg/cluster/config/public_dns_refresher.job`
    - TODO: Need to setup local DNS that overrules this and directly goes to the frontend machine.
- You should now be able to go to your domain (or `https://auth.domain.com`)
    - All the supported subdomains are defined in the `pkg/cluster/config/frontend_config.txtpb` file.

## Life of a request

When accessing a service through a web browser (going to `example.com` or a subdomain):

- Public DNS will resolve the domain name to your local network and your router will forward the connection to the `ingress.frontend` job.
- The frontend job receives the request via a TLS HTTP server hosting a public certificate for `(*.)example.com`
    - (obtained from Let's Encrypt)
- Based on per-worker quota limits, the frontend performs throttling on the connection to mitigate single peer abuse.
- If the request doesn't have a `Client-Id` cookie, the frontend sets it to a random value
    - This is mainly meant for affinity routing requests.
- If the request has an `Auth-Key` cookie, the frontend uses it to authenticate the request against a logged in user session.
- Then the frontend checks which backend should serve the request (as defined in the `frontend_config.txtpb` file)
- Each backend has a ACL defined and if the user is authorized to visit the backend, they are redirected to `auth.example.com`.
    - The `auth` backend never requires any permissions to access.
    - This special backend serves the purpose of logging in a user (via user name and password) and setting the `Auth-Key` cookie.
- Assuming the ACL checks have passed, the frontend forwards the HTTP request to the backend.
    - Extra headers are added to specify what session/user is logged in.
    - Connection from the frontend to the backends use standard internal mTLS and the backends are all setup to trust the frontend as a proxy service.
- Backends process the HTTP request and return a response.
    - For ACL checking purposes, backends check against the original user's name rather than the peer identity (which is the frontend job).

Important notes about cookies:

- All cookies are set on the `example.com` level so that a user only needs to login once to visit all subdomains.
- Cookies are 'HttpOnly' so can't be seen by JavaScript code.
- Cookies are stripped before forwarding to backends.
    - These last two behaviors prevent backends from impersonating a user when accessing other services.
