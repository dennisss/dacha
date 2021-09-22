
rust_binary {
    name: "adder_server.armv7"
    bin: "adder_server"

    use_cross: true
    profile: "release"
    target: "armv7-unknown-linux-gnueabihf"
}

bundle {
    name: "adder_server_bundle"
    absolute_srcs: [
        "built/pkg/rpc_test/adder_server.armv7"
    ]
    deps: [
        ":adder_server.armv7"
    ]
}