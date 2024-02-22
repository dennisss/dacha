/*
Tool for verifying that all images stored in the git repository are well formatted.

Well formatted meaning a lack of metadata, using good compression, etc.

Run:
cargo run --release --bin image_linter -- --write=true
*/

#[macro_use]
extern crate macros;

use base_error::*;
use file::{project_path, LocalPathBuf};
use image::Image;

#[derive(Args)]
struct Args {
    #[arg(default = false)]
    write: bool,
}

const MAX_PIXEL_AREA: usize = 1_000_000;
const MAX_FILE_SIZE: usize = 600 * 1024;

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut image_files = vec![];

    // TODO: Expact to all non-gitignored files.
    let input_dirs = vec![project_path!("doc"), project_path!("pkg")];

    // TODO: Support more than just JPG and support more extensions including with
    // case insensitivity.
    for input_dir in input_dirs {
        file::recursively_list_dir(&input_dir, &mut |path| {
            if path.extension() == Some("jpg") {
                image_files.push(path.to_owned());
            }
        })?;
    }

    for path in image_files {
        println!("{}", path.as_str());

        let input_data = file::read(&path).await?;
        let jpg = image::format::jpeg::JPEG::parse(&input_data)?;

        let mut violations = vec![];

        // TODO: Have a more robust way to check this.
        if !jpg.trailer_data.is_empty() || !jpg.unknown_segments.is_empty() {
            violations.push(format!("Image has extra non-pixel metadata"));
        }

        let original_area = jpg.image.width() * jpg.image.height();
        if original_area > MAX_PIXEL_AREA {
            violations.push(format!("Pixel area is too large: {}", original_area))
        }

        if input_data.len() > MAX_FILE_SIZE {
            violations.push(format!("File too big: {}", input_data.len()));
        }

        if violations.is_empty() {
            println!("=> Good!");
            continue;
        }

        println!("=> Violations: {:#?}", violations);

        let mut img = jpg.image;

        if img.width() * img.height() > MAX_PIXEL_AREA {
            let aspect_ratio = (img.height() as f32) / (img.width() as f32);

            // Re-scale to fit into the max area.
            let height = ((MAX_PIXEL_AREA as f32) * aspect_ratio).sqrt() as usize;
            let width = ((MAX_PIXEL_AREA as f32) / (height as f32)) as usize;

            img = img.resize(height, width);
        }

        let encoder = image::format::jpeg::encoder::JPEGEncoder::new(90);
        let mut output_data = vec![];
        encoder.encode(&img, &mut output_data)?;

        if output_data.len() > MAX_FILE_SIZE {
            println!("=> STILL TOO BIG AFTER RE-ENCODING!");
            continue;
        }

        if args.write {
            file::write(&path, &output_data).await?;
        }
    }

    Ok(())
}
