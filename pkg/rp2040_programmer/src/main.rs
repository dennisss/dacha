#[macro_use]
extern crate common;
extern crate parsing;
extern crate usb;

mod elf;

use common::async_std::task;
use common::errors::*;
use crypto::hasher::Hasher;
use usb::descriptors::TransferType;

struct PicobootClient {
    device: usb::Device,
    bulk_in: u8,
    bulk_out: u8,
    last_token: u32,
}

impl PicobootClient {
    pub async fn open() -> Result<Self> {
        let ctx = usb::Context::create()?;
        let mut dev = ctx.open_device(0x2e8a, 0x003).await?;

        let mut in_vendor_iface = false;
        let mut previously_seen_iface = None;

        let mut bulk_out = None;
        let mut bulk_in = None;

        for desc in dev.descriptors() {
            match desc {
                usb::Descriptor::Interface(iface) => {
                    let matched = iface.bInterfaceClass == 0xff && // Vendor Specific
                        iface.bInterfaceSubClass == 0 &&
                        iface.bInterfaceProtocol == 0;

                    if matched && previously_seen_iface.is_some() {
                        return Err(err_msg("Found multiple ifaces matching protocol"));
                    }

                    in_vendor_iface = matched;

                    if matched {
                        previously_seen_iface = Some(iface.bInterfaceNumber);
                    }
                }
                usb::Descriptor::Endpoint(ep) => {
                    if !in_vendor_iface {
                        continue;
                    }

                    if ep.transfer_type() != TransferType::Bulk {
                        return Err(err_msg(
                            "Expected only bulk endpoints in the picoboot interface",
                        ));
                    }

                    if ep.is_in() {
                        if bulk_in.is_some() {
                            return Err(err_msg("Duplicate input endpoint"));
                        }

                        bulk_in = Some(ep.bEndpointAddress);
                    } else {
                        if bulk_out.is_some() {
                            return Err(err_msg("Duplicate output endpoint"));
                        }

                        bulk_out = Some(ep.bEndpointAddress);
                    }
                }
                _ => {}
            }
        }

        let bulk_in = bulk_in.ok_or_else(|| err_msg("Missing bulk in"))?;
        let bulk_out = bulk_out.ok_or_else(|| err_msg("Missing bulk out"))?;

        dev.claim_interface(previously_seen_iface.unwrap())?;

        println!("IN {:02x}", bulk_in);
        println!("OUT {:02x}", bulk_out);

        Ok(Self {
            device: dev,
            bulk_in,
            bulk_out,
            last_token: 0,
        })
    }

    fn new_command(&mut self, command_id: u8) -> Command {
        self.last_token += 1;

        Command {
            token: self.last_token,
            command_id,
            command_size: 0,
            transfer_length: 0,
            args: [0u8; 16],
        }
    }

    pub async fn read(&mut self, addr: u32, out: &mut [u8]) -> Result<()> {
        let mut cmd = self.new_command(0x84);
        cmd.transfer_length = out.len() as u32;
        cmd.arg_u32(addr).arg_u32(out.len() as u32);

        self.device
            .write_bulk(self.bulk_out, &cmd.serialize())
            .await?;

        let n = self.device.read_bulk(self.bulk_in, out).await?;
        if n != out.len() {
            return Err(err_msg("Didn't read the entire requested range"));
        }

        // Complete the command.
        self.device.write_bulk(self.bulk_out, &[]).await?;

        Ok(())
    }

    // TODO: If doing flash, must be erased first.

    pub async fn write(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        let mut cmd = self.new_command(0x05);
        cmd.transfer_length = data.len() as u32;
        cmd.arg_u32(addr).arg_u32(data.len() as u32);

        println!("COMMAND");
        self.device
            .write_bulk(self.bulk_out, &cmd.serialize())
            .await?;

        println!("COMMAND DATA");
        self.device.write_bulk(self.bulk_out, data).await?;

        println!("COMMAND PKT");
        self.device.read_bulk(self.bulk_in, &mut []).await?;

        Ok(())
    }

    pub async fn enter_xip(&mut self) -> Result<()> {
        let cmd = self.new_command(0x07);

        self.device
            .write_bulk(self.bulk_out, &cmd.serialize())
            .await?;

        self.device.read_bulk(self.bulk_in, &mut []).await?;
        Ok(())
    }

    pub async fn exit_xip(&mut self) -> Result<()> {
        let cmd = self.new_command(0x06);

        self.device
            .write_bulk(self.bulk_out, &cmd.serialize())
            .await?;

        self.device.read_bulk(self.bulk_in, &mut []).await?;
        Ok(())
    }

    pub async fn flash_erase(&mut self, addr: u32, size: u32) -> Result<()> {
        assert!(addr % 4096 == 0);
        assert!(size % 4096 == 0);

        let mut cmd = self.new_command(0x03);
        cmd.arg_u32(addr).arg_u32(size);

        self.device
            .write_bulk(self.bulk_out, &cmd.serialize())
            .await?;

        self.device.read_bulk(self.bulk_in, &mut []).await?;
        Ok(())
    }

    pub async fn exec(&mut self, addr: u32) -> Result<()> {
        let mut cmd = self.new_command(0x03);
        cmd.arg_u32(addr);

        self.device
            .write_bulk(self.bulk_out, &cmd.serialize())
            .await?;
        Ok(())
    }

    pub async fn reboot(&mut self) -> Result<()> {
        let mut cmd = self.new_command(0x02);
        cmd.arg_u32(0).arg_u32(0).arg_u32(100);

        self.device
            .write_bulk(self.bulk_out, &cmd.serialize())
            .await?;

        self.device.read_bulk(self.bulk_in, &mut []).await?;

        Ok(())
    }
}

struct Command {
    token: u32,

    // NOTE: THe top bit implies the direction
    // 0x80 = IN
    command_id: u8,
    command_size: u8,
    transfer_length: u32,
    args: [u8; 16],
}

impl Command {
    fn serialize(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        *array_mut_ref![buf, 0, 4] = (0x431fd10b as u32).to_le_bytes();
        *array_mut_ref![buf, 4, 4] = self.token.to_le_bytes();
        buf[8] = self.command_id;
        buf[9] = self.command_size;
        // reserved 2 bytes
        *array_mut_ref![buf, 0xc, 4] = self.transfer_length.to_le_bytes();
        *array_mut_ref![buf, 0x10, 16] = self.args;
        buf
    }

    fn arg_u32(&mut self, value: u32) -> &mut Self {
        *array_mut_ref![self.args, self.command_size as usize, 4] = value.to_le_bytes();
        self.command_size += 4;
        self
    }
}

async fn run() -> Result<()> {
    let elf = elf::ELF::open(project_path!("target/thumbv6m-none-eabi/release/rp2040")).await?;

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

        if (program_header.vaddr as usize) < 0x10000000 {
            continue;
            // return Err(err_msg("Segment less than flash start"));
        }

        if (program_header.vaddr as usize) != flash_end {
            return Err(err_msg("Expected program headers to be contigous"));
        }

        if program_header.mem_size != program_header.file_size {
            return Err(err_msg("Expected mem size and file size to be equal"));
        }

        println!(
            "Found segment: {:x} of size {}",
            program_header.vaddr, program_header.file_size
        );

        let data = &elf.file[(program_header.offset as usize)
            ..(program_header.offset as usize + program_header.file_size as usize)];

        println!("{:x?}", data);

        flash_contents.extend_from_slice(data);
        flash_end += (program_header.mem_size as usize);
    }

    println!("Flash Range: {:x} - {:x}", flash_start, flash_end);

    /*
    let mut uf2 = vec![];

    {
        let total_num_blocks = common::ceil_div(flash_end - flash_start, 256);

        let mut flash_i = 0;
        while flash_i < flash_contents.len() {
            uf2.extend_from_slice(&(0x0A324655 as u32).to_le_bytes());
            uf2.extend_from_slice(&(0x9E5D5157 as u32).to_le_bytes());
            uf2.extend_from_slice(&(0 as u32).to_le_bytes()); // flags
            uf2.extend_from_slice(&((flash_start + flash_i) as u32).to_le_bytes());
            uf2.extend_from_slice(&(256 as u32).to_le_bytes());
            uf2.extend_from_slice(&((flash_i / 256) as u32).to_le_bytes());
            uf2.extend_from_slice(&(total_num_blocks as u32).to_le_bytes());
            uf2.extend_from_slice(&(0 as u32).to_le_bytes());

            let mut data = [0u8; 476];
            let n = std::cmp::min(256, flash_contents.len() - flash_i);
            data[0..n].copy_from_slice(&flash_contents[flash_i..(flash_i + n)]);
            uf2.extend_from_slice(&data);

            uf2.extend_from_slice(&(0x0AB16F30 as u32).to_le_bytes());

            flash_i += 256;
        }
    }

    common::async_std::fs::write(project_path!("rp2040.uf2"), &uf2).await?;
    */

    let remainder = common::block_size_remainder(4096, flash_end as u64) as usize;
    let flash_block_end = flash_end + remainder;

    // third_party/tiny2040-boot2.bin
    // return Ok(());

    // TODO: Read using a buffer that is a multiple of the USB max packet size of
    // the bulk endpoints.

    let mut client = PicobootClient::open().await?;

    // This seems to be required if reading from flash.
    client.exit_xip().await?;

    /*
    {
        println!("READ");

        let mut second_stage = vec![0u8; 256];
        client.read(0x10000000, &mut second_stage).await?;

        // 0x10000000: XIP

        println!("{:02x?}", second_stage);

        std::fs::write(project_path!("third_party/pico-boot2.bin"), &second_stage)?;

        // println!("{:?}", &second_stage[0x10..(0x10 + 3)]);

        for i in 0..252 {
            second_stage[i] = second_stage[i].reverse_bits();
        }

        let checksum = {
            let mut hasher = crypto::checksum::crc::CRC32Hasher::new();
            hasher.update(&second_stage[0..(256 - 4)]);
            !hasher.finish_u32().reverse_bits()
        };

        let expected_checksum = u32::from_le_bytes(*array_ref![second_stage, 252, 4]);

        println!("{:08x} {:08x}", checksum, expected_checksum);

        return Ok(());
    }
    */

    println!(
        "Flash Erase Range: {:x} - {:x}",
        flash_start, flash_block_end
    );

    client
        .flash_erase(flash_start as u32, (flash_block_end - flash_start) as u32)
        .await?;

    client.write(flash_start as u32, &flash_contents).await?;

    let mut read_flash = vec![0u8; flash_contents.len()];
    client.read(flash_start as u32, &mut read_flash).await?;
    assert_eq!(read_flash, flash_contents);

    client.reboot().await?;

    // client.enter_xip().await?;
    // client.exec(0x10000109).await?;

    // TODO: Now restart it.

    // let mut random = [0u8; 256];
    // for i in (0..random.len()).step_by(2) {
    //     random[i] = i as u8;
    // }

    // println!("WRITE");
    // client.write(0x2000002c, &random).await?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
