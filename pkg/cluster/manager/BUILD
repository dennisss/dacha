
rust_binary(
    name = "cluster_manager"
)

bundle(
    name = "bundle",
    deps = [
        ":cluster_manager"
    ],
    configs = [
        "//pkg/builder/config:x64",
	    # "//pkg/builder/config:rpi64"
    ]
)