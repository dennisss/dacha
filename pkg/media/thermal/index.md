# Thermal Camera Interface Utilities

```
cargo run --bin media_thermal --release -- record --output_path=video.log

cargo run --bin media_thermal --release -- \
    encode-mp4 \
    --input_path=video.log \
    --output_path=video.mp4
```