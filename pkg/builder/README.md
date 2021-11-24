
Files
- `built/` contains the contacts of the 
- `built-config/[config_name]/`

- `//builder/config:raspberry_pi`

- `built-config/raspberry_pi-443did81a`

- `built-rust/`



Step 1:
- Need to know which profile is in use

- 


`cross build --target=armv7-unknown-linux-gnueabihf --bin cluster_node --release --target-dir=/home/dennis/workspace/dacha/built-rust/1`


/*
rustc --crate-name usb_device --edition 2018 --crate-type rlib -o build/test.rlib pkg/usb_device/src/lib.rs

*/


Leverage BUILD files.
- For now, auto-add binaries for Cargo projects.


/*
    Every build runs with an ambient:
    - BuildConfig {}

    - Target specific parameters take priority
    - Followed by bundle configs
    - Followed by user provided flags.

    - So yes, we can build a single rule with many different settings
*/
