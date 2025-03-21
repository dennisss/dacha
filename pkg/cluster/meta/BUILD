rust_binary(
    name = "cluster_meta"
)

bundle(
    name = "cluster_meta_bundle",
    deps = [
        ":cluster_meta"
    ],
    configs = [
        "//pkg/builder/config:x64",
        "//pkg/builder/config:rpi64"
    ]
)