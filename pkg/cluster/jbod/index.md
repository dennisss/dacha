# JBOD Management Service

This is a server application that is designed to monitor and control a JBOD ([//pkg/cluster/machines/jbod](/pkg/cluster/machines/jbod/index.md)) over USB. The control interface is either other RPC or through a web UI.

Note that this service is designed to run in a managed cluster and so is dependent on running on a local machine or node that has the [cluster runtime installed](/pkg/cluster/index.md).

## Local Testing

Build the web UI:

```
cargo run --bin builder -- build //pkg/cluster/jbod:app
```

Run the server:

```
cargo run --bin cluster_jbod -- --port=8001
```

## Production Deployment

```
# Label the node you want it run on.
cargo run --bin cluster_cli --  labels set --node_id=xxx "name=nas"

# Push/update the server.
cargo run --bin cluster_cli -- start_job pkg/cluster/jbod/config/cluster_jbod.job
```
