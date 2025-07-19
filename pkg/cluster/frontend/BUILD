
rust_binary(
    name = "cluster_frontend"
)

bundle(
    name = "bundle",
    deps = [
        ":cluster_frontend",
        ":config"
    ],
    configs = [
        "//pkg/builder/config:x64"
    ]
)

filegroup(
    name = "config",
    srcs = [
        "//pkg/cluster/config/frontend_config.txtpb"
    ]
)