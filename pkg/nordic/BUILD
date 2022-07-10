
rust_binary {
    name: "nordic_radio_dongle"
}

rust_binary {
    name: "nordic_bootloader"
}

rust_binary {
    name: "nordic"
}

rust_binary {
    name: "nordic_blink"
}

build_config {
    name: "nrf52840"
    platform {
        os: UNKNOWN
        architecture: UNKNOWN
    }
    rust_binary {
        compiler: CARGO
        profile: "release"
        no_default_features: true
        target: "thumbv7em-none-eabihf"
        rustflags: "-C link-arg=-Tpkg/nordic/link.x"
        cfg: ['target_model="nrf52840"', 'target_label="cortex_m"']
    }
}

build_config {
    name: "nrf52840_bootloader"
    platform {
        os: UNKNOWN
        architecture: UNKNOWN
    }
    rust_binary {
        compiler: CARGO
        profile: "release"
        no_default_features: true
        target: "thumbv7em-none-eabihf"
        rustflags: "-C link-arg=-Tpkg/nordic/link_bootloader.x"
        cfg: ['target_model="nrf52840"', 'target_label="cortex_m"']
    }
}
