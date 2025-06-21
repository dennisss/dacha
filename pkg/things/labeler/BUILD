webpack(
    name = "app",
    entry = "js/index.tsx"
)

rust_binary(
    name = "labeler"
)

bundle(
    name = "bundle",
    deps = [
        ":labeler",
        ":app",
        "//pkg/web:web_data"
    ],
    configs = [
        "//pkg/builder/config:rpi64"
    ]
)