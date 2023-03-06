use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ControlIds {
    controls: Vec<BTreeMap<String, Control>>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Control {
    /// The C++ type identifier for the value stored in this control.
    #[serde(alias = "type")]
    typ: String,

    description: String,

    #[serde(alias = "enum")]
    enum_values: Option<Vec<EnumValue>>,

    size: Option<Vec<serde_yaml::Value>>,

    /// TODO: Handle this appropriately
    #[serde(default)]
    draft: bool,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct EnumValue {
    name: String,
    value: u32,
    description: String,
}

fn compile_controls(namespace: &str, in_path: &str, out_dir: &Path) {
    let control_ids: ControlIds = {
        let yaml = std::fs::read_to_string(in_path).unwrap();
        serde_yaml::from_str(&yaml).unwrap()
    };

    let mut out = String::new();

    for control in control_ids.controls {
        assert_eq!(control.len(), 1);
        let (control_name, control) = control.first_key_value().unwrap();

        let (primitive_type, enum_allowed) = match control.typ.as_str() {
            "int32_t" => ("i32", true),
            "int64_t" => ("i64", true),
            "bool" => ("bool", true),
            "float" => ("f32", false),
            "Rectangle" => ("Rectangle", false),
            "Size" => ("Size", false),
            "string" => ("String", false),
            _ => panic!("Unsupported control type: {}", control.typ),
        };

        let mut control_type = primitive_type.to_string();

        if let Some(enum_values) = &control.enum_values {
            assert!(
                enum_allowed,
                "Control type {} not allowed to be an enum",
                control.typ
            );

            control_type = format!("{}Enum", control_name);

            out.push_str(&format!(
                "control_enum!({}Enum {} {{\n",
                control_name, primitive_type
            ));

            for value in enum_values {
                out.push_str(&format!(
                    "    /// {}\n",
                    value.description.replace("\n", " ")
                ));
                out.push_str(&format!("    {} = {},\n", value.name, value.value));
            }

            out.push_str("});\n\n");
        }

        if let Some(dims) = &control.size {
            let mut is_static_size = true;
            let mut size = 1;
            for dim in dims {
                match dim {
                    serde_yaml::Value::Number(n) => {
                        size *= n.as_u64().unwrap();
                    }
                    serde_yaml::Value::String(_) => {
                        is_static_size = false;
                        break;
                    }
                    _ => panic!("Unexpected dimension type in control size: {:?}", dim),
                }
            }

            if is_static_size {
                control_type = format!("[{}; {}]", control_type, size);
            } else {
                control_type = format!("[{}]", control_type);
            }
        }

        let subnamespace = if control.draft { "::draft" } else { "" };

        out.push_str(&format!(
            "control!(\n/// {}\n{}, {}, {}{});\n\n",
            control.description.replace("\n", " "),
            control_name,
            control_type,
            namespace,
            subnamespace
        ));
    }

    std::fs::write(out_dir.join(format!("{}.rs", namespace)), out).unwrap();
}

fn main() {
    /*
    cd /opt/dacha/pi/rootfs/usr/lib/aarch64-linux-gnu
    ln -sf /opt/dacha/pi/rootfs/lib/aarch64-linux-gnu/libpthread.so.0 libpthread.so

    cargo run --bin file --release -- copy /media/dennis/rootfs /opt/dacha/pi/rootfs --skip_permission_denied --symlink_root=/opt/dacha/pi/rootfs


    PKG_CONFIG_PATH_aarch64_unknown_linux_gnu=/opt/dacha/pi/rootfs/usr/lib/aarch64-linux-gnu/pkgconfig PKG_CONFIG_SYSROOT_DIR_aarch64_unknown_linux_gnu=/opt/dacha/pi/rootfs BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu="--sysroot=/opt/dacha/pi/rootfs"  cargo build --target aarch64-unknown-linux-gnu --release --bin libcamera


    scp -i ~/.ssh/id_cluster target/aarch64-unknown-linux-gnu/release/libcamera cluster-user@10.1.0.112:~

    scp -i ~/.ssh/id_cluster cluster-user@10.1.0.112:~/image.rgb .

    */

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let lib = pkg_config::probe_library("libcamera").unwrap();

    let include_paths = lib
        .include_paths
        .iter()
        .map(|p| p.to_str().unwrap())
        .collect::<Vec<_>>();

    compile_controls("controls", "repo/src/libcamera/control_ids.yaml", &out_path);
    compile_controls(
        "properties",
        "repo/src/libcamera/property_ids.yaml",
        &out_path,
    );

    let bindings = bindgen::Builder::default()
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .header("src/wrapper.h")
        .clang_args(include_paths.iter().map(|path| format!("-I{}", path)))
        .clang_args(["-x", "c++", "-std=c++2a"])
        .enable_cxx_namespaces()
        .newtype_enum(".*")
        .allowlist_type("libcamera.*StreamRole")
        .allowlist_type("libcamera.*Request.*Status")
        .allowlist_type("libcamera.*Request.*ReuseFlag")
        .allowlist_type("libcamera.*FrameMetadata.*Status")
        .allowlist_type("libcamera.*FrameMetadata.*Plane")
        .allowlist_type("libcamera.*CameraConfiguration.*Status")
        .allowlist_type("libcamera.*PixelFormat")
        .allowlist_type("libcamera.*SizeRange")
        .allowlist_type("libcamera.*Size")
        .allowlist_type("libcamera.*ControlType")
        .allowlist_type("libcamera.*Rectangle")
        .allowlist_type("libcamera.*ControlValuePrimitive")
        .allowlist_var("libcamera.*controls.*")
        .allowlist_var("libcamera.*properties.*")
        .allowlist_var("libcamera.*ColorSpace.*")
        .opaque_type("libcamera.*Control.*")
        .no_debug("libcamera.*PixelFormat")
        .no_debug("libcamera.*ColorSpace")
        .blocklist_function(".*")
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    cxx_build::bridge("src/ffi.rs") // returns a cc::Build
        .file("src/ffi.cc")
        .includes(include_paths)
        .flag("-std=c++2a")
        .compile("libcamera-cxx");

    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/wrappers.cc");
    println!("cargo:rerun-if-changed=src/wrappers.h");
}
