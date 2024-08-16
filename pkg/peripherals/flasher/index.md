


```
cargo run --bin flasher -- \
    target/attiny85/debug/avr.elf attiny \
    --reset_pin=18 --spi_device=/dev/spidev0.0
```
