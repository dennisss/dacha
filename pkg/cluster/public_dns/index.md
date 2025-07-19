This is a simple application which just has the role of setting the DNS records 

Testing

```
cargo run --bin cluster_public_dns -- \
    --port=8000 \
    --dns_name=dacha.dev \
    --google_service_account_object=google_service_account
```