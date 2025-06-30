

webpack(
    name = "app",
    entry = "js/index.tsx"
)

rust_binary(
    name = "cluster_adder_server"
)

bundle(
    name = "bundle",
    deps = [
        ":cluster_adder_server",
        ":app",
        "//pkg/web:web_data"
    ],
    configs = [
        "//pkg/builder/config:rpi64",
        "//pkg/builder/config:x64",
    ]
)