

# TODO: Must compile for ARM
rust_binary {
    name: "main"
}

bundle {
    name: "bundle"
    deps: [
        ":web",
        ":main",
        "//pkg/web:web_data"
    ]
}

webpack {
    name: "web"
    entry: "web/index.tsx"
}