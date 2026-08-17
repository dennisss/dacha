use common::line_builder::LineBuilder;
use file::{LocalPath, LocalPathBuf};

const INPUT_PATHS: &'static [&'static str] = &[
    "pkg/vision/mocap/config/manager.txtpb",
    "built/pkg/vision/mocap/manager/app.js",

    "pkg/web/index.html",
    "pkg/web/style.css",
    "node_modules/bootstrap/dist/css/bootstrap.min.css",
    "node_modules/bootstrap/dist/css/bootstrap.min.css.map",
    "third_party/noto_sans/font_normal.ttf",
    "third_party/noto_sans/font_mono_normal.ttf",
    "third_party/material_icons/material_symbols_outlined.woff2",
];

fn main() {
    let project_dir = file::project_dir();

    let mut lines = LineBuilder::new();

    for rel_path in INPUT_PATHS {
        let input_path = project_dir.join(rel_path);

        // // Not needed since we aren't dealing with directories and include_bytes!()
        // // should handle checking individual files.
        // println!("cargo:rerun-if-changed={}", input_path.display());

        lines.add(format!(
            r#"
            {{
                const DATA: &[u8] = include_bytes!("{}");
                file::register_asset("{}", DATA)?;
            }}
            "#,
            input_path.as_str(),
            rel_path
        ));
    }

    let out = format!(
        r#"
        pub fn register_assets() -> Result<()> {{
            {}
            Ok(())
        }}
        "#,
        lines.to_string()
    );

    let output_dir = LocalPathBuf::from(std::env::var("OUT_DIR").unwrap());

    std::fs::write(output_dir.join("register_assets.rs"), out).unwrap();
}