
rust_binary {
    name: "adder_server"
}

bundle {
    name: "adder_server_bundle"
    deps: [
        ":adder_server"
    ]
    configs: [
        "//pkg/builder/config:rpi",
        "//pkg/builder/config:x64"
    ]
}
