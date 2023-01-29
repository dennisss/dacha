#[macro_use]
extern crate macros;

use common::{errors::*, io::Readable};

#[executor_main]
async fn main() -> Result<()> {
    println!("{:#?}", file::read_dir(".")?);

    println!("{:?}", file::readlink("built")?);

    let mut file = ::file::LocalFile::open("hello_world")?;

    let mut buf = vec![];
    file.read_to_end(&mut buf).await?;

    println!("{:?}", std::str::from_utf8(&buf)?);

    Ok(())
}
