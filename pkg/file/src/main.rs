use common::{errors::*, io::Readable};

async fn run() -> Result<()> {
    let mut file = ::file::LocalFile::open("hello_world").await?;

    let mut buf = vec![];
    file.read_to_end(&mut buf).await?;

    println!("{:?}", std::str::from_utf8(&buf)?);

    Ok(())
}

fn main() -> Result<()> {
    executor::run(run())?
}
