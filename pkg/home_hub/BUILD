

rust_binary {
    name: "main.armv7"
    bin: "home_hub"

    use_cross: true
    profile: "release"
    target: "armv7-unknown-linux-gnueabihf"
}

bundle {
    name: "bundle"
    absolute_srcs: [
        "built/pkg/home_hub/main.armv7",
        "pkg/home_hub/icons"
    ]
    deps: [
        ":main.armv7"
    ]
}