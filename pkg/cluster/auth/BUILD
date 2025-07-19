webpack(
    name = "app",
    entry = "js/index.tsx"
)

rust_binary(
    name = "cluster_auth"
)

bundle(
    name = "bundle",
    deps = [
        ":cluster_auth",
        ":app",
        "//pkg/web:web_data"
    ],
    configs = [
        "//pkg/builder/config:x64"
    ]
)