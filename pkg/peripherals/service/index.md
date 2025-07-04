# Peripherals Service Protocol

Web/RPC interface to control MCU peripherals connected via USB to a server.


## Local Testing

```
cargo run --bin builder -- build //pkg/peripherals/service:app

cargo run --bin peripherals_service -- --port=8000 --config_name=nrf52840_itsybitsy
```


## Running on a cluster

```
cargo run --bin cluster_cli -- \
    labels set --node_id=[node-id] "overhead_light_controller=yes"

cargo run --bin cluster_cli -- \
    start_job pkg/peripherals/config/overhead_light_controller.job

```