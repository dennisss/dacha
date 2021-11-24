

rust_binary {
    name: "cluster_node.armv7"
    bin: "cluster_node"
    
    compiler: CROSS
    profile: "release"
    target: "armv7-unknown-linux-gnueabihf"
}

rust_binary {
    name: "cluster_manager"
    profile: "release"
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