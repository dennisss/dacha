

Implementing SWD:
- Two pins:
    - CLK : Up to 60MHZ
        - Default to 1
    - IO - Host sets on rising edge and device samples on falling edge. 

- LSB first.

```
cargo run --package builder -- build //pkg/swd_prog:swd_prog --config=//pkg/builder/config:rpi64
scp -i ~/.ssh/id_cluster /home/dennis/workspace/dacha/built-rust/7d1839c53e4e6579/aarch64-unknown-linux-gnu/release/swd_prog pi@10.1.0.88:~/swd_prog

```

        swd_prog
        "//pkg/builder/config:rpi64"