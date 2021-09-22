

# TODO: Must compile for ARM
rust_binary {
    name: "main"
    profile: "release"
}

bundle {
    name: "bundle"
    absolute_srcs: [
        "built/pkg/sensor_monitor/main",
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