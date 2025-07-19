

Testing

```

# Put this in /etc/hosts
127.0.0.1       dacha.dev
127.0.0.1       auth.dacha.dev

# Modify the frontend config to point the auth job to 'localhost:8001'

# Then run

cargo run --bin builder -- build //pkg/cluster/auth:app

cargo run --bin cluster_auth -- --port=8001


cargo run --bin cluster_frontend -- \
    --port=8000 \
    --public_credentials_object_prefix=letsencrypt_prod/out \
    --config=pkg/cluster/config/frontend_config.txtpb

```