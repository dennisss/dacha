extern crate common;
extern crate executor;

use common::errors::*;

fn main() -> Result<()> {
    executor::run(async {
        println!("Hello world!");

        let mut file = executor::LocalFile::open("hello_world").unwrap();

        let mut buf = [0u8; 32];
        let n = file.read(&mut buf).await.unwrap();

        println!("Read {} : {:?}", n, buf);
    })?;

    Ok(())
}
