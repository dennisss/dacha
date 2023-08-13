#[macro_use]
extern crate common;
extern crate elf;
extern crate parsing;
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    let elf = elf::ELF::read("target/debug/sys").await?;

    println!("Build ID: {:x?}", elf.build_id()?);

    elf.print()?;

    let mut total_size = 0;
    for program_header in &elf.program_headers {
        if program_header.typ != 1 {
            // PT_LOAD
            continue;
        }

        total_size += program_header.file_size;
    }

    println!("Total Loaded Size: {}", total_size);

    Ok(())
}
