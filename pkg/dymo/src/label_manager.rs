use std::sync::Arc;

use common::errors::*;

use crate::label_program::LabelProgramBuilder;

const VENDOR_ID: u16 = 0x0922;  // Dymo-CoStar Corp.
const PRODUCT_ID: u16 = 0x1001;  // LabelManager PnP

const USB_CONFIG: u8 = 1;
const USB_IFACE: u8 = 0;
const USB_IFACE_ALT_SETTING: u8 = 0;
const USB_EP_IN: u8 = 0x81;
const USB_EP_OUT: u8 = 0x01;

const USB_MAX_PACKET_SIZE: usize = 64;

const STATUS_LENGTH: usize = 8;

pub struct LabelManager {
    device: usb::Device
}

impl LabelManager {

    pub async fn open() -> Result<Self> {
        let context = usb::Context::create()?;
        Self::open_with_context(context).await
    }

    pub async fn open_with_context(context: Arc<usb::Context>) -> Result<Self> {

        let mut device = {
            let mut device = None;
    
            let entries = context.enumerate_devices().await?;
            for device_entry in entries {
                let device_desc = device_entry.device_descriptor()?;
                if device_desc.idVendor == VENDOR_ID && device_desc.idProduct == PRODUCT_ID {
                    device = Some(device_entry.open().await?);
                }
            }
    
            device.ok_or(err_msg("No device found"))?
        };
    
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
    
        // NOTE: Can't be changed while the kernel is also holding the second interface for
        // mass storage.
        // device.set_active_configuration(USB_CONFIG)?;
    
        if device.kernel_driver_active(USB_IFACE)? {
            println!("Detaching kernel driver.");
            device.detach_kernel_driver(USB_IFACE)?;
        }
    
        device.claim_interface(USB_IFACE)?;
        device.set_alternate_setting(USB_IFACE, USB_IFACE_ALT_SETTING)?;
    
        Ok(Self { device })
    }

    pub fn dpi(&self) -> usize { 180 }

    pub fn pixels_per_line(&self) -> usize { 64 }

    pub async fn read_status(&self) -> Result<Status> {
        let status_request = [0x1b, b'A'];
        self.device.write_interrupt(USB_EP_OUT, &status_request).await?;
        
        let mut status_response = [0u8; STATUS_LENGTH];
        let n = self.device.read_interrupt(USB_EP_IN, &mut status_response).await?;
        if n != status_response.len() {
            return Err(err_msg("Status response was wrong size"));
        }

        Ok(Status {
            raw_data: status_response
        })
    }

    /// TODO: We need to lock the device in order to send large multi-packet payloads.
    pub async fn print_label(&self, lines: &[Vec<u8>]) -> Result<()> {
        let program = LabelProgramBuilder::compile_lines(lines)?;

        for i in 0..common::ceil_div(program.len(), USB_MAX_PACKET_SIZE) {
            let start_i = i*64;
            let end_i = std::cmp::min(start_i + 64, program.len());
            let pkt = &program[start_i..end_i];
    
            self.device.write_interrupt(USB_EP_OUT, &pkt).await?;
        }

        Ok(())
    }
    
        // 'D' used to do bytes per line
    
        // 'B' used to skip the first N bytes in all following lines.
    
        // 0x16 is the line start character following by the actual line
    

}

#[derive(Debug)]
pub struct Status {
    raw_data: [u8; STATUS_LENGTH]
}
