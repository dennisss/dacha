
rust_binary(
    name = "rpi_controller"
)

bundle(
    name = "bundle",
    deps = [
        ":rpi_controller",
        ":app",
        "//pkg/web:web_data"
    ],
    configs = [
        "//pkg/builder/config:rpi64"
    ]
)

webpack(
    name = "app",
    entry = "web/index.tsx"
)