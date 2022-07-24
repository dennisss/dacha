

rust_binary(
    name = "metastore"
)

bundle(
    name = "metastore_bundle",
    deps = [
        ":metastore"
    ],
    configs = [
        "//pkg/builder/config:x64",
        "//pkg/builder/config:rpi64"
    ]
)
