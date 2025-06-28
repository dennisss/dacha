# Label Printer Interface

This is a web UI and RPC service for printing out labels from text data. Currently 

## Running on a cluster

Add a label to the nodes which have label makers plugged in via USB and then start the job:

```
cargo run --bin cluster_cli -- \
    labels set --node_id=[node-id] "labelers=pt-p700"

cargo run --bin cluster_cli -- \
    start_job pkg/things/labeler/config/main.job
```

the UI will be accessible via the link in 'cluster_cli list workers'

## Local Testing

```
cargo run --bin builder -- build //pkg/things/labeler:app

cargo run --bin labeler -- --port=8000
```


