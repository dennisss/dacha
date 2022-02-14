
build_config {
    name: "rpi"
    platform {
        os: LINUX
        architecture: ARM32v7
    }
    rust_binary {
        compiler: CROSS
        profile: "release"
        target: "armv7-unknown-linux-gnueabihf"
    }
}

build_config {
    name: "rpi64"
    platform {
        os: LINUX
        architecture: AArch64
    }
    rust_binary {
        compiler: CROSS
        profile: "release"
        target: "aarch64-unknown-linux-gnu"
    }
}

build_config {
    name: "x64"
    platform {
        os: LINUX
        architecture: AMD64
    }
    rust_binary {
        compiler: CARGO
        profile: "release"
        target: "x86_64-unknown-linux-gnu"
    }
}
