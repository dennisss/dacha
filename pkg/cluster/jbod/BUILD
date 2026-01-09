webpack(
    name = "app",
    entry = "js/index.tsx"
)

rust_binary(
    name = "cluster_jbod"
)

bundle(
    name = "bundle",
    deps = [
        ":cluster_jbod",
        ":app",
        ":configs",
        "//pkg/web:web_data"
    ],
    configs = [
        # "//pkg/builder/config:rpi64",
        "//pkg/builder/config:x64",
    ]
)

filegroup(
    name = "configs",
    srcs = [
        "//pkg/peripherals/config/boards",
    ]
)