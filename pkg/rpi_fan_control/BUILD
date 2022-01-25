
rust_binary {
    name: "rpi_fan_control"
}

bundle {
    name: "rpi_fan_control_bundle"
    deps: [
        ":rpi_fan_control",
        "//pkg/web:web_data"
    ]
    configs: [
        "//pkg/builder/config:rpi"
    ]
}

webpack {
    name: "app"
    entry: "web/index.tsx"
}