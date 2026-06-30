
rust_binary(
    name = "frame_processing_test"
)

rust_binary(
    name = "mocap_tester"
)

rust_binary(
    name = "mocap_camera"
)

webpack(
    name = "app",
    entry = "js/index.tsx"
)

bundle(
    name = "mocap_camera_bundle",
    deps = [
        ":mocap_camera",
        ":app",
        "//pkg/web:web_data"
    ],
    configs = [
        # "//pkg/builder/config:x64",
        # "//pkg/builder/config:rpi64_pi5"
        "//pkg/builder/config:rpi64"
    ]
)
