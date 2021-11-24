

rust_binary {
    name: "metastore"
}

bundle {
    name: "metastore_bundle"
    absolute_srcs: [
        "built/pkg/datastore/metastore"
    ]
    deps: [
        ":metastore"
    ]
    configs: [
        "//pkg/builder/config:x64",
        "//pkg/builder/config:rpi",
    ]
}