

group(
    name = "cluster_node_deps",
    deps = [
        ":cluster_node",
        ":container_init",
        ":newcgroup",
    ]
)

rust_binary(
    name = "cluster_node"
)

rust_binary(
    name = "container_init"
)

rust_binary(
    name = "newcgroup"
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