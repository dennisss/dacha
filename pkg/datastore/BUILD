

rust_binary {
    name: "metastore.x64"
    bin: "metastore"

    # use_cross: true
    # profile: "release"
    # target: "armv7-unknown-linux-gnueabihf"
}

bundle {
    name: "metastore_bundle"
    absolute_srcs: [
        "built/pkg/datastore/metastore.x64"
    ]
    deps: [
        ":metastore.x64"
    ]
}