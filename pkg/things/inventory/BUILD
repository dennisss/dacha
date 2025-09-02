webpack(
    name = "app",
    entry = "js/index.tsx"
)

rust_binary(
    name = "inventory"
)

bundle(
    name = "bundle",
    deps = [
        ":inventory",
        ":app",
        "//pkg/web:web_data"
    ],
    configs = [
        "//pkg/builder/config:x64"
    ]
)