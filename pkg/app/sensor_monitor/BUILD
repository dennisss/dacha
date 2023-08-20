

# TODO: Must compile for ARM
rust_binary(
    name = "main"
)

bundle(
    name = "bundle",
    deps = [
        ":web",
        ":main",
        "//pkg/web:web_data"
    ],
    configs = [
        "//pkg/builder/config:x64",
	    "//pkg/builder/config:rpi64"
    ]
)

webpack(
    name = "web",
    entry = "web/index.tsx"
)