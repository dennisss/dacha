#[macro_use]
extern crate common;
extern crate elf;
extern crate uf2;
extern crate usb;
#[macro_use]
extern crate macros;

use common::errors::*;
use uf2::*;

/*
Usage:
cargo run --bin builder --  build //pkg/nordic:nordic_blink --config=//pkg/nordic:nrf52840
cargo run --bin flasher built/pkg/nordic/nordic_blink

da build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840_bootloader
cargo run --bin flasher

Features to add:
- UF2 input
- Use builder to find file (or maybe build the target)
    - Also useful as the builder can give us file format metadata
    - Also store flashing profiles in a standard place.
- RP2040 picoboot support.

TODO: DFU Bootloaders need to be queryable for the flash range they are editing so we can cross validate that the binary was built correctly.

*/

#[derive(Args)]
struct Args {
    #[arg(positional)]
    path: String,

    usb_selector: usb::DeviceSelector,
}

// TODO: Also bring in support for

struct UF2Builder {
    /// All the data blocks formed so far.
    data: Vec<u8>,
    next_block_number: u32,
}

impl UF2Builder {
    fn new() -> Self {
        Self {
            data: vec![],
            next_block_number: 0,
        }
    }

    fn write(&mut self, mut target_address: u32, data: &[u8]) {
        assert!(data.len() % 4 == 0 && target_address % 4 == 0);

        for chunk in data.chunks(256) {
            let mut block = UF2Block::default();

            block.block_number = self.next_block_number;
            self.next_block_number += 1;

            block.target_addr = target_address;
            target_address += chunk.len() as u32;

            block.payload_size = chunk.len() as u32;
            block.data[0..chunk.len()].copy_from_slice(chunk);

            self.data.extend_from_slice(block.as_bytes());
        }
    }
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let elf = elf::ELF::read(&args.path).await?;

    let mut firmware_builder = UF2Builder::new();

    let mut total_written = 0;

    for program_header in &elf.program_headers {
        if program_header.typ != elf::ProgramHeaderType::PT_LOAD.to_value() {
            continue;
        }

        if program_header.mem_size != program_header.file_size {
            return Err(err_msg("Expected mem size and file size to be equal"));
        }

        println!(
            "Write {:08x} - {:08x}",
            program_header.paddr,
            program_header.paddr + program_header.file_size
        );

        let data = &elf.file[(program_header.offset as usize)
            ..(program_header.offset as usize + program_header.file_size as usize)];

        firmware_builder.write(program_header.paddr as u32, data);

        total_written += data.len();
    }

    println!("Flash Space Used: {}", total_written);
    println!("Firmware UF2 size: {}", firmware_builder.data.len());

    let mut host = usb::dfu::DFUHost::create(args.usb_selector)?;

    host.download(&firmware_builder.data).await?;

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
