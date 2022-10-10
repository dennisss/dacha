use common::errors::*;
use image::{BinaryImage, Color, Image};
use usb::descriptors::SetupPacket;

use crate::bit_packing::pack_line_bits;
use crate::command::*;
use crate::MediaType;
use crate::PhaseType;
use crate::Status;
use crate::StatusType;

// For a PT-P700
const VENDOR_ID: u16 = 0x04f9;
const PRODUCT_ID: u16 = 0x2061;

/*
PT-H500 : 0x205E
PT-E500: 0x205F
PT-P700 : 0x2061
*/

// TODO: Need validation of min/max allowed tape widths.

const USB_IFACE: u8 = 0;

// TODO: Have an automated way to find this.
const BULK_OUT_ENDPOINT: u8 = 0x02;
const BULK_IN_ENDPOINT: u8 = 0x81;

// Applicable to PT-H500/P700/E500
const DPI: usize = 180;

// 24mm tape has 128 pixel width and 3mm margins by default
// 180 DPI?

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
    pub async fn open() -> Result<Self> {
        let context = usb::Context::create()?;

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

        println!("READ: {:?}", common::bytes::Bytes::from(&buffer[0..n]));
        Ok(())
    }

    pub async fn get_status(&mut self) -> Result<Status> {
        self.device
            .write_bulk(BULK_OUT_ENDPOINT, &[0x1b, 0x69, 0x53])
            .await?;

        let status = self
            .poll_status()
            .await?
            .ok_or_else(|| err_msg("Received no response to status request"))?;
        if status.status_type != StatusType::ReplyToStatusRequest {
            return Err(err_msg("Incorrect status type received"));
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
    pub async fn print(&mut self, image: &Image<u8>) -> Result<()> {
        /*
        On the PT-P700, one raster line is '128 pins'
        - Uncompressed this is encoded as 16 bytes where each bit is a pixel
        - Order of pins is from MSB of first octet to LSB of last octet.
        - The tape is centered in the pins
            - If using a small tape (<24 mm), then we still need to send 16 bytes of info but with the left/right sides zeroed out.
        */

        let status = self.get_status().await?;
        status.check_can_start_printing()?;

        // TODO: Verify that the printer is ready for printing.
        // - No errors
        // - We have valid media (need to use the media data to determine tape width
        //   constraints)

        // TODO: Make this dynamic based on tape media width.
        if image.height() != 128 {
            return Err(err_msg("Image height should match print area height"));
        }

        // TODO: Also check min width.

        // Construct binary transposed image.
        let mut raster_image = BinaryImage::zero(image.width(), image.height());
        for x in 0..image.width() {
            for y in 0..image.height() {
                let color = image.get(y, x);
                if color != Color::hex(0xFFFFFF) {
                    raster_image.set(x, y, 1);
                }
            }
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
                image.width(),
                true,
            )
            .set_advanced_mode_settings(AdvancedModeSettings::NO_CHAIN_PRINTING)
            .set_various_mode_settings(VariousModeSettings::AUTO_CUT)
            .set_cut_interval(1)
            .set_feed_margin(14) // 2mm at 180 DPI
            .set_compression_mode(CompressionMode::TIFF);

        for i in 0..image.width() {
            let data = raster_image.row_data(i);

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

        command_buffer.print_with_feeding();

        self.device
            .write_bulk(BULK_OUT_ENDPOINT, &command_buffer.as_ref())
            .await?;

        // TODO: Set a timeout on this.
        loop {
            let status = self.poll_status().await?;
            if let Some(status) = status {
                status.check_for_errors()?;

                if status.status_type == StatusType::PrintingComplete {
                    break;
                } else if status.phase_type != PhaseType::PrintingState {
                    return Err(err_msg(
                        "Printer not printing, but never received completion notification",
                    ));
                }
            } else {
                common::async_std::task::sleep(std::time::Duration::from_millis(100)).await;
            }
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
}
