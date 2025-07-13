rust_binary(
    name = "cluster_acme"
)

bundle(
    name = "bundle",
    deps = [
        ":cluster_acme",
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