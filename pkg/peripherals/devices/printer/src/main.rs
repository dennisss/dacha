#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use common::errors::*;
use usb::descriptors::SetupPacket;

/*


cargo build --bin printer --target=aarch64-unknown-linux-gnu
  
scp -i ~/.ssh/id_cluster target/aarch64-unknown-linux-gnu/debug/printer cluster-user@10.1.1.2:~/


"usblp" is the usual kernel driver for this.
- Probably exposes under "/dev/usb/lp0"??

Normal device id is "\x00\x99MFG:Brother;CMD:PJL,PCL,PCLXL,URF;MDL:HL-L2460DW;CLS:PRINTER;CID:Brother Laser Type1;URF:W8,CP1,IS4-1,MT1-3-4-5-8,OB10,PQ3-4-5,RS300-600-1200,V1.5,DM1;"


/home/dennis/Downloads/letter-portrait-vector-color.pdf

scp -i ~/.ssh/id_cluster /home/dennis/Downloads/letter-portrait-vector-color.pdf cluster-user@10.1.1.2:~/

/home/cluster-user/letter-portrait-vector-color.pdf


480 : @PJL INFO CONFIG\r\nIN TRAYS [1 TABLES]\r\n\x09INTRAY2 PC\r\nOUT TRAYS [1 ENUMERATED]\r\n\x09NORMAL FACEDOWN\r\nPAPERS [20 ENUMERATED]\r\n\x09LETTER\r\n\x09LEGAL\r\n\x09A4\r\n\x09EXECUTIVE\r\n\x09COM10\r\n\x09DL\r\n\x09JISB5\r\n\x09B5\r\n\x09A5\r\n\x09A6\r\n\x09MONARCH\r\n\x09C5\r\n\x09FOLIO\r\n\x09P3X5\r\n\x09A5L\r\n\x09JISB6\r\n\x09SIXTEENK195X270\r\n\x09MEXICANLEGAL\r\n\x09A4SHORT\r\n\x09INDIALEGAL\r\nLANGUAGES [5 ENUMERATED]\r\n\x09PCL\r\n\x09PCLXL\r\n\x09BVR\r\n\x09JPC\r\n\x09HBP\r\nUSTATUS [4 ENUMERATED]\r\n\x09DEVICE\r\n\x09JOB\r\n\x09PAGE\r\n\x09TIMED\r\nMEMORY=134217728\r\nDISPLAY LINES=1\r\nDISPLAY CHARACTER SIZE=16\r\nLOCAL=ENGLISH\r\n\x0C


Reference PCL generator:
- https://github.com/pdewacht/brlaser/blob/master/src/job.cc

- PJL referene
    - https://developers.hp.com/system/files/attachments/PJLReference%282003%29_0.pdf

- Random brother info about PCL
    - https://download.brother.com/welcome/doc002907/Tech_Manual_AD.pdf

https://www.hp.com/ctg/Manual/bpl13205.pdf
https://www.hp.com/ctg/Manual/bpl13205.pdf
- Quick reference

Basically need to create a minimal raster image using PCL and send that over PJL to the printer

Desktop integration:
- Locally expose an IPP server that proxies into the cluster
    - https://datatracker.ietf.org/doc/html/rfc8011
    - https://datatracker.ietf.org/doc/html/rfc2911

    


Useful PCL is:
- <esc>*b#M
- <esc>*b#W


Need to rely on the CUPS PPD file to determine what the printable region is.

TODO: Printer turning off after some time.

*/

#[executor_main]
async fn main() -> Result<()> {
    let ctx = usb::Context::create()?;

    println!("AA");

    let mut device = ctx.open_device(0x04f9, 0x058b).await?;

    println!("B");

    if device.kernel_driver_active(0)? {
        println!("Removing kernel driver..");
        device.detach_kernel_driver(0)?;
    }

    println!("BB");

    /*
    for desc in device.descriptors() {
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
    */

    // let bulk_in = bulk_in.ok_or_else(|| err_msg("Missing bulk in"))?;
    // let bulk_out = bulk_out.ok_or_else(|| err_msg("Missing bulk out"))?;

    device.claim_interface(0)?;

    let mut buffer = [0u8; 512];

    device
        .read_control(
            SetupPacket {
                bmRequestType: 0b10100001,
                bRequest: 0,
                wValue: 1, // config index
                wIndex: 0, // interface and alternative setting
                wLength: buffer.len() as u16,
            },
            &mut buffer,
        )
        .await?;

    println!("{}", base_util::format::format_bytes(&buffer));

    // let data = file::read("/home/cluster-user/letter-portrait-vector-color.pdf").await?;

        // <ESC> Escape character (ASCII 27).

    // let data = b"\x1B%-12345X@PJL INFO CONFIG\r\n\x1B%-12345X";
    let data = b"\x1B%-12345X@PJL INFO VARIABLES\r\n\x1B%-12345X";

    device
    .write_bulk(0x01, &data[..])
    .await?;

    let mut out = vec![0u8; 512];

    loop {
        let n = device.read_bulk(0x82, &mut out[..]).await?;
        if n == 0 {
            executor::sleep(std::time::Duration::from_millis(100)).await?;
            continue;
        }

        println!("{}", std::str::from_utf8(&out[0..n])?);

        // println!("{} : {}", n, base_util::format::format_bytes(&out[0..n]));
    }


    /*
    let devices = ctx.enumerate_devices().await?;

    for dev in devices {
        // TODO: If a manufacturer is not available, look up from a database of known
        // vendors.

        let mut manufacturer = dev.manufacturer().await?.unwrap_or_default();
        if !manufacturer.is_empty() {
            manufacturer = format!("[{}] ", manufacturer);
        }

        let mut product = dev.product().await?.unwrap_or_default();
        if !product.is_empty() {
            product = format!("{} ", product);
        }

        let mut serial = dev.serial().await?.unwrap_or_default();
        if !serial.is_empty() {
            serial = format!("({})", serial);
        }

        let desc = dev.device_descriptor()?;

        println!(
            "Bus {:3}, Dev {:3}, Id {:04x}:{:04x} | {}{}{}",
            dev.bus_num(),
            dev.dev_num(),
            desc.idVendor,
            desc.idProduct,
            manufacturer,
            product,
            serial
        );
    }
    */

    Ok(())
}