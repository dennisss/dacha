use std::time::Duration;
use std::time::Instant;

use base_error::*;
use image::{BinaryImage, Color, Image};
use macros::hex_ref;
use usb::descriptors::SetupPacket;

use crate::bit_packing::pack_line_bits;
use crate::command;
use crate::command::*;
use crate::MediaType;
use crate::PhaseType;
use crate::Status;
use crate::StatusType;

// For a PT-P700
const VENDOR_ID: u16 = 0x04f9;
const PRODUCT_ID: u16 = 0x2061;

// TODO: Need validation of min/max allowed tape widths.

const USB_IFACE: u8 = 0;

// TODO: Have an automated way to find this.
const BULK_OUT_ENDPOINT: u8 = 0x02;
const BULK_IN_ENDPOINT: u8 = 0x81;

/// Number of dots in one scan line. We always send this many dots per line of
/// data to the printer but for smaller tapes, this may contain a lot of
/// padding.
const PRINT_HEAD_SIZE: usize = 128;

/// TODO: Properly implement all the error flows in
/// cv_pth500p700e500_eng_raster_111
///
/// TODO: Also need proper dot alignment for non-width sized tapes.
///
/// TODO: Need to implement timeouts for all commands.
///
/// TODO: If a command ever fails, require a full invalidate/initialize before
/// the next command is sent.
pub struct LabelMaker {
    device: usb::Device,
}

impl LabelMaker {
    pub fn is_supported_device(device_entry: &usb::DeviceEntry) -> Result<bool> {
        let device_desc = device_entry.device_descriptor()?;
        if device_desc.idVendor != VENDOR_ID {
            return Ok(false);
        }

        /*
        PT-H500 : 0x205E
        PT-E500: 0x205F
        PT-P700 : 0x2061
        */
        match device_desc.idProduct {
            0x205E | 0x205F | 0x2061 => Ok(true),
            _ => Ok(false),
        }
    }

    pub async fn open() -> Result<Self> {
        let context = usb::Context::create()?;

        // TODO: Need to support there existing multiple of these.
        let mut device = context.open_device(VENDOR_ID, PRODUCT_ID).await?;

        // TODO: Set 1 second timeout
        let languages = device.read_languages().await?;
        if languages.len() != 1 {
            return Err(err_msg("Expected only a single language"));
        }

        println!("Languages: {:?}", languages);

        // TODO: Set 1 second timeout
        let product_name = device.read_product_string(languages[0]).await?;
        println!("Product name: {}", product_name);

        Self::open_existing(device).await
    }

    pub fn short_name(&self) -> String {
        match self.device.device_descriptor().idProduct {
            0x205E => "PT-H500",
            0x205F => "PT-E500",
            0x2061 => "PT-P700",
            _ => todo!(),
        }
        .to_string()
    }

    pub async fn open_existing(mut device: usb::Device) -> Result<Self> {
        device.reset()?;

        if device.kernel_driver_active(USB_IFACE)? {
            println!("Detaching kernel driver.");
            device.detach_kernel_driver(USB_IFACE)?;
        }

        device.claim_interface(USB_IFACE)?;

        // Reset to a known good state.
        let mut command_buffer = CommandBuffer::new();
        command_buffer.invalidate().initialize();
        device
            .write_bulk(BULK_OUT_ENDPOINT, command_buffer.as_ref())
            .await?;

        Ok(Self { device })
    }

    /// Reads out a string that will look like:
    ///
    /// "\0QMFG:Brother;CMD:PT-CBP;MDL:PT-P700;CLS:PRINTER;CID:Brother
    /// LabelPrinter TypeA1;"
    pub async fn get_info(&mut self) -> Result<()> {
        let mut buffer = [0u8; 256];
        let n = self
            .device
            .read_control(
                SetupPacket {
                    bmRequestType: 0xA1,
                    bRequest: 0,
                    wValue: 0,
                    wIndex: 0,
                    wLength: buffer.len() as u16,
                },
                &mut buffer,
            )
            .await?;

        // println!("READ: {:?}", common::bytes::Bytes::from(&buffer[0..n]));
        Ok(())
    }

    pub async fn get_status(&mut self) -> Result<Status> {
        // Sometimes the status doesn't return anything, so we give it a few attempts.

        let mut status = None;
        for _ in 0..3 {
            // "ESC i S"
            self.device
                .write_bulk(BULK_OUT_ENDPOINT, &[0x1b, 0x69, 0x53])
                .await?;

            status = self.poll_status().await?;

            if status.is_some() {
                break;
            }

            executor::sleep(Duration::from_millis(10)).await?;
        }

        let status = status.ok_or_else(|| err_msg("Received no response to status request"))?;

        if status.status_type != StatusType::ReplyToStatusRequest {
            // This will have 'ErrorOccured' if the cover is open.
            return Err(format_err!(
                "Incorrect status type received: {:?}",
                status.status_type
            ));
        }

        Ok(status)
    }

    async fn poll_status(&mut self) -> Result<Option<Status>> {
        let mut buffer = [0u8; 64];
        let n = self.device.read_bulk(BULK_IN_ENDPOINT, &mut buffer).await?;
        if n == 0 {
            return Ok(None);
        }

        Ok(Some(Status::parse(&buffer[0..n])?))
    }

    /// Prints the given image. The image should be the same size as the
    /// printable area excluding any margins.
    ///
    /// Any color in the image that is not equal to 0xFFFFFF is considered to be
    /// black.
    ///
    /// The label maker will print the image from x=0 to x=n with the ordering
    /// of bits sent per column being from y=0 to y=m.
    ///
    /// TODO: Support cancelling a print by sending an Invalidate/Initialize
    /// command sequence.
    pub async fn print(&mut self, pages: &[Image<u8>]) -> Result<()> {
        /*
        On the PT-P700, one raster line is '128 pins'
        - Uncompressed this is encoded as 16 bytes where each bit is a pixel
        - Order of pins is from MSB of first octet to LSB of last octet.
        - The tape is centered in the pins
            - If using a small tape (<24 mm), then we still need to send 16 bytes of info but with the left/right sides zeroed out.
        */

        let status = self.get_status().await?;
        status.check_can_start_printing()?;

        let tape = status
            .tape()
            .ok_or_else(|| err_msg("Unsupported tape type"))?;

        // TODO: Also check min width.

        // Construct binary transposed image.
        let mut raster_images = vec![];

        let mut total_width = 0;
        for page in pages {
            if page.height() != tape.print_area {
                return Err(err_msg("Image height should match print area height"));
            }

            let mut raster_image = BinaryImage::zero(page.width(), PRINT_HEAD_SIZE);

            let pad = (PRINT_HEAD_SIZE - tape.print_area) / 2;

            let white = match page.channels() {
                1 => Color::hex(0xff0000),
                3 => Color::hex(0xffffff),
                _ => return Err(err_msg("Unsupported number of channels")),
            };

            for x in 0..page.width() {
                for y in 0..page.height() {
                    let color = page.get(y, x);
                    if color != white {
                        raster_image.set(x, pad + y, 1);
                    }
                }
            }

            total_width += page.width();

            raster_images.push(raster_image);
        }

        let mut command_buffer = CommandBuffer::new();
        command_buffer
            .invalidate()
            .set_command_mode(CommandMode::RASTER_MODE)
            .initialize()
            .set_print_info(
                Some(status.media_type),
                Some(status.media_width),
                None,
                total_width,
                true,
            )
            .set_advanced_mode_settings(AdvancedModeSettings::NO_CHAIN_PRINTING)
            .set_various_mode_settings(VariousModeSettings::AUTO_CUT)
            .set_cut_interval(1)
            .set_feed_margin(tape.margin as u16)
            .set_compression_mode(CompressionMode::TIFF);

        for (page_i, page_image) in raster_images.iter().enumerate() {
            for i in 0..page_image.height() {
                let data = page_image.row_data(i);

                let all_zero = {
                    let mut yes = true;
                    for v in data {
                        if *v != 0 {
                            yes = false;
                            break;
                        }
                    }

                    yes
                };

                if all_zero {
                    command_buffer.raster_zero();
                } else {
                    let compressed = pack_line_bits(data);
                    command_buffer.raster_transfer(&compressed);
                }
            }

            if page_i == raster_images.len() - 1 {
                command_buffer.print_with_feeding();
            } else {
                command_buffer.print();
            }

            self.device
                .write_bulk(BULK_OUT_ENDPOINT, &command_buffer.as_ref())
                .await?;

            let mut start_time = Instant::now();
            let mut started_printing = false;
            loop {
                let status = self.poll_status().await?;
                if let Some(status) = status {
                    status.check_for_errors()?;

                    if status.status_type == StatusType::PrintingComplete {
                        break;
                    } else if status.phase_type == PhaseType::PrintingState {
                        // NOTE: We may temporarily be in the 'EditingStatus' state before
                        // transitioning to the printing stage.
                        started_printing = true;
                    }
                } else {
                    executor::sleep(std::time::Duration::from_millis(100)).await;
                }

                let mut now = Instant::now();
                if !started_printing && now - start_time > Duration::from_secs(5) {
                    return Err(err_msg(
                        "Time out while waiting for printing of page to start.",
                    ));
                }

                if now - start_time > Duration::from_secs(30) {
                    return Err(err_msg("Taking a very long time to print."));
                }
            }

            command_buffer.clear();
        }

        // Brother software writes does but I don't know why.
        {
            let mut command_buffer = CommandBuffer::new();
            command_buffer.set_command_mode(CommandMode::UNKNOWN_FF);

            self.device
                .write_bulk(BULK_OUT_ENDPOINT, &command_buffer.as_ref())
                .await?;
        }

        Ok(())
    }

    /// Ensures the device has the following settings:
    /// - 'Power On when Plugged in': 'Enable'
    /// - 'Auto power off hwen AC adapter is connected': 'None'
    ///
    /// and the rest using default settings.
    ///
    /// WARNING: This is only known to do the right thing for a PT-P700
    pub async fn configure_settings(&self) -> Result<()> {
        let packets = [
            hex_ref!("1b696101"), // Set to raster mode
            hex_ref!("1b6955700001"),
            hex_ref!("1b695541000000"), // This is the 'Never turn off' setting??
            hex_ref!("1b69586f0000"),
            hex_ref!("550b030d64"),
            hex_ref!("1b69557001"),
            hex_ref!("1b6955410100"),
            hex_ref!("1b69586f01"),
            hex_ref!("550b040d"),
        ];

        for packet in packets {
            self.device.write_bulk(BULK_OUT_ENDPOINT, packet).await?;
        }

        Ok(())
    }
}
