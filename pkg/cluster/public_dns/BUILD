rust_binary(
    name = "cluster_public_dns"
)

bundle(
    name = "bundle",
    deps = [
        ":cluster_public_dns",
        ":public_tls_registry"
    ],
    configs = [
        "//pkg/builder/config:x64",
        # "//pkg/builder/config:rpi64"
    ]
)

filegroup(
    name = "public_tls_registry",
    srcs = [
        "//third_party/chromium/root_store.bin"
    ]
)