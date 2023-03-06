#[macro_use]
extern crate macros;

use common::errors::*;
use file::LocalPathBuf;
use graphics::image_show::ImageShow;

#[derive(Args)]
struct Args {
    #[arg(positional)]
    path: LocalPathBuf,

    #[arg(default = false)]
    raw: bool,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    if args.raw {
        let data = file::read(&args.path).await?;

        let width = 800;
        let height = 600;

        let mut image = image::Image::<u8>::zero(height, width, image::Colorspace::RGB);
        image.copy_from_rgb888(&data);

        image.show().await?;
    } else {
        let image = image::Image::read(&args.path).await?;
        image.show().await?;
    }

    Ok(())
}
