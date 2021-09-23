
rust_binary {
    name: "main.armv7"
    bin: "blinkstick"

    use_cross: true
    profile: "release"
    target: "armv7-unknown-linux-gnueabihf"
}

bundle {
    name: "bundle"
    absolute_srcs: [
        "built/pkg/blinkstick/main.armv7"
    ]
    deps: [
        ":main.armv7"
    ]
}