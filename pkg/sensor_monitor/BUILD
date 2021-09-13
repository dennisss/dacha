

rust_binary {
    name: "main"
    profile: "release"
}

bundle {
    name: "bundle"
    absolute_srcs: [
        "target/debug/sensor_monitor",
        # "pkg/sensor_monitor/main",
        # "pkg/sensor_monitor/static",
        "built/pkg/sensor_monitor/web",
        "node_modules/bootstrap/dist/css/bootstrap.min.css"
    ]
    deps: [
        ":web",
        ":main"
    ]
}

webpack {
    name: "web"
}