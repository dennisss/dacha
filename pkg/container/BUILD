

rust_binary {
    name: "cluster_node"
}

rust_binary {
    name: "cluster_manager"
}

bundle {
    name: "cluster_manager_bundle"
    absolute_srcs: [
        "built/pkg/container/cluster_manager"
    ]
    deps: [
        ":cluster_manager"
    ]
}