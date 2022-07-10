#[macro_use]
extern crate common;
extern crate elf;
extern crate usb;

use common::errors::*;

/*
Features to add:
- UF2 input
- Use builder to find file (or maybe build the target)
    - Also useful as the builder can give us file format metadata
    - Also store flashing profiles in a standard place.
- RP2040 picoboot support.

TODO: DFU Bootloaders need to be queryable for the flash range they are editing so we can cross validate that the binary was built correctly.

*/

// TODO: Also bring in support for

async fn run() -> Result<()> {
    let elf = elf::ELF::read(project_path!(
        "built-rust/50f2512e741290a0/thumbv7em-none-eabihf/release/nordic_bootloader"
    ))
    .await?;

    let flash_start = 0x10000000;
    let mut flash_end = flash_start;
    let mut flash_contents = vec![];

    // let boot2 =
    //     common::async_std::fs::read(project_path!("third_party/tiny2040-boot2.
    // bin")).await?; flash_contents.extend_from_slice(&boot2);
    // flash_end += boot2.len();

    for program_header in &elf.program_headers {
        if program_header.typ != 1 {
            // PT_LOAD
            continue;
        }

        if program_header.vaddr > 1048576 {
            println!("Skip: {:?}", program_header);

            // for section in elf.section_headers {

            // }

            continue;
        }

        /*
        if (program_header.vaddr as usize) < 0x10000000 {
            continue;
            // return Err(err_msg("Segment less than flash start"));
        }

        if (program_header.vaddr as usize) != flash_end {
            return Err(err_msg("Expected program headers to be contigous"));
        }
        */

        println!("{:?}", program_header);

        if program_header.mem_size != program_header.file_size {
            return Err(format_err!(
                "Expected mem size and file size to be equal: {} vs {}",
                program_header.mem_size,
                program_header.file_size
            ));
        }

        assert_eq!(program_header.vaddr, program_header.paddr);

        println!(
            "Found segment: 0x{:x} of size {}",
            program_header.vaddr, program_header.file_size
        );

        let data = &elf.file[(program_header.offset as usize)
            ..(program_header.offset as usize + program_header.file_size as usize)];

        // println!("{:x?}", data);

        flash_contents.extend_from_slice(data);
        // flash_end += (program_header.mem_size as usize);
    }

    println!("Found bytes: {}", flash_contents.len());

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
