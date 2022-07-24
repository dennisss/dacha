
rust_binary(
    name = "main",
    bin = "blinkstick"
)

bundle(
    name = "bundle",
    deps = [
        ":main"
    ],
    configs = [
        # "//pkg/builder/config:rpi",
        "//pkg/builder/config:x64"
    ]
)