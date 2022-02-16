

rust_binary {
    name: "main"
    bin: "home_hub"
}

bundle {
    name: "bundle"
    deps: [
        ":main",
        ":icons"
    ]
    configs: [
        "//pkg/builder/config:rpi64"
        # "//pkg/builder/config:x64"
    ]
}

filegroup {
    name: "icons",
    srcs: [
        "icons"
    ]
}