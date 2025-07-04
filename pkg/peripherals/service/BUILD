webpack(
    name = "app",
    entry = "js/index.tsx"
)

rust_binary(
    name = "peripherals_service"
)

bundle(
    name = "bundle",
    deps = [
        ":peripherals_service",
        ":app",
        "//pkg/web:web_data",
        ":configs",
    ],
    configs = [
        "//pkg/builder/config:rpi64"
    ]
)

filegroup(
    name = "configs",
    srcs = [
        "//pkg/peripherals/config/boards"
    ]
)