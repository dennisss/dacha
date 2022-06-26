#[macro_use]
extern crate common;
extern crate parsing;
extern crate elf;

use common::async_std::task;
use common::errors::*;

async fn run() -> Result<()> {
    let elf =
        elf::ELF::read("/home/dennis/workspace/dacha/target/release/sys")
            .await?;

    elf.print()?;

    let mut total_size = 0;
    for program_header in &elf.program_headers {
        if program_header.typ != 1 {
            // PT_LOAD
            continue;
        }

        total_size += program_header.file_size;
    }

    println!("Total Size: {}", total_size);

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
