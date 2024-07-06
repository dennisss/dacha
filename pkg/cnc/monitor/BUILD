webpack(
    name = "app",
    entry = "js/index.tsx"
)

rust_binary(
    name = "cnc_monitor"
)

bundle(
    name = "bundle",
    deps = [
        ":cnc_monitor",
        ":app",
        ":presets",
        "//pkg/web:web_data"
    ],
    configs = [
        "//pkg/builder/config:rpi64",
        "//pkg/builder/config:x64",
    ]
)

# TODO: Explicitly only include the txtpb files.
filegroup(
    name = "presets",
    srcs = [
        "presets"
    ]
)