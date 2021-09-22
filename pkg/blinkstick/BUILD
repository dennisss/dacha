
rust_binary {
    name: "main"
    bin: "blinkstick"

    # use_cross: true
    # profile: "release"
    # target: "armv7-unknown-linux-gnueabihf"
}

bundle {
    name: "bundle"
    absolute_srcs: [
        "built/pkg/blinkstick/main"
    ]
    deps: [
        ":main"
    ]
}