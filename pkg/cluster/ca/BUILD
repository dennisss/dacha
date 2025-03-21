

rust_binary(
    name = "cluster_ca"
)

bundle(
    name = "bundle",
    deps = [
        ":cluster_ca"
    ],
    configs = [
        "//pkg/builder/config:x64",
        # "//pkg/builder/config:rpi64"
    ]
)