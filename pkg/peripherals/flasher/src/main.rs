#[macro_use]
extern crate common;
extern crate elf;
extern crate uf2;
extern crate usb;
#[macro_use]
extern crate macros;
extern crate file;

mod attiny;

use attiny::ATTinyProgrammer;
use common::{ceil_div, errors::*};
use file::LocalPathBuf;
use peripherals::gpio::*;
use peripherals::spi::SPIDevice;
use uf2::*;

/*
Usage:
cargo run --bin builder --  build //pkg/nordic:nordic_blink --config=//pkg/nordic:nrf52840
cargo run --bin flasher built/pkg/nordic/nordic_blink uf2-dfu

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

    protocol: Protocol,

    usb_selector: usb::DeviceSelector,
}

#[derive(Args, Clone)]
enum Protocol {
    #[arg(name = "uf2-dfu")]
    UF2OverDFU,

    #[arg(name = "attiny")]
    ATTiny {
        reset_pin: u32,
        spi_device: LocalPathBuf,
    },
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

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let data = file::read(&args.path).await?;
    let elf = elf::ELF::parse(data)?;

    let mut total_written = 0;

    let mut segments = vec![];

    for (i, program_header) in elf.program_headers.iter().enumerate() {
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

        let data = elf.program_data(i);

        segments.push((program_header.paddr as u32, data));

        total_written += data.len();
    }

    println!("Flash Space Used: {}", total_written);

    match args.protocol {
        Protocol::UF2OverDFU => {
            let mut firmware_builder = UF2Builder::new();
            for (offset, data) in segments {
                firmware_builder.write(offset, data);
            }

            println!("Firmware UF2 size: {}", firmware_builder.data.len());

            let mut host = usb::dfu::DFUHost::create(args.usb_selector)?;

            host.download(&firmware_builder.data).await?;
        }
        Protocol::ATTiny {
            reset_pin,
            spi_device,
        } => {
            let gpio = GPIOChip::default_chip().unwrap();

            let pin = gpio.pin(reset_pin)?;

            let mut programmer = ATTinyProgrammer::new(SPIDevice::open("/dev/spidev0.0")?, pin)?;

            programmer.enter_programming_mode().await?;

            programmer.erase_chip().await?;

            let page_size = programmer.flash_page_size_bytes()?;

            let mut last_offset = 0;
            for (offset, data) in segments {
                if offset > last_offset {
                    return Err(err_msg("Overlapping writes"));
                }

                if offset % (page_size as u32) != 0 {
                    return Err(err_msg("Unaligned write"));
                }

                let mut padded_data = vec![0u8; ceil_div(data.len(), page_size) * page_size];
                padded_data[0..data.len()].copy_from_slice(data);

                programmer
                    .flash_write(offset as usize, &padded_data)
                    .await?;

                let mut read_data = vec![0u8; padded_data.len()];
                programmer
                    .flash_read(offset as usize, &mut read_data)
                    .await?;

                assert_eq!(&read_data, &padded_data);

                last_offset = offset + (padded_data.len() as u32);
            }

            programmer.exit_programming_mode().await?;
        }
    }

    Ok(())
}
