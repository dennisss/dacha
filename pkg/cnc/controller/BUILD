rust_binary(
    name = "cnc_controller"
)

bundle(
    name = "bundle",
    deps = [
        ":cnc_controller",
        ":configs"
    ],
    configs = [
        "//pkg/builder/config:rpi64"
        # "//pkg/builder/config:x64"
    ]
)

filegroup(
    name = "configs",
    srcs = [
        "//pkg/peripherals/config/boards",
        "//pkg/cnc/controller/config"
    ]
)