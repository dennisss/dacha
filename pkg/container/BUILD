

rust_binary(
    name = "cluster_node"
)

rust_binary(
    name = "cluster_manager"
)

bundle(
    name = "cluster_manager_bundle",
    deps = [
        ":cluster_manager"
    ],
    configs = [
        "//pkg/builder/config:x64",
	    "//pkg/builder/config:rpi64"
    ]
)

rust_binary(
    name = "cluster"
)

local_binary(
    name = "cluster_bin",
    deps = [":cluster"]
)