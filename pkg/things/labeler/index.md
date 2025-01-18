
```
cargo run --bin labeler -- --port=8000 --tls_certificate=$PWD/cnc.crt --tls_key=$PWD/cnc.key

cargo run --bin builder -- build //pkg/things/labeler:app
```