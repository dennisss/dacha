#[macro_use]
extern crate macros;

use common::errors::*;
use graphics::image_show::ImageShow;

#[derive(Args)]
struct Args {
    #[arg(positional)]
    path: String,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    let image = image::Image::read(&args.path).await?;
    image.show().await?;
    Ok(())
}
