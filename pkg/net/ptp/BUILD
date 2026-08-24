rust_binary(
    name = "ptp"
)

rust_binary(
    name = "ptp_test"
)

rust_binary(
    name = "ptp_enable_timestamping"
)
rust_binary(
    name = "ptp_service"
)

bundle(
    name = "ptp_service_bundle",
    deps = [
        ":ptp_service"
    ],
    configs = [
        # "//pkg/builder/config:x64",
        "//pkg/builder/config:rpi64"
    ]
)