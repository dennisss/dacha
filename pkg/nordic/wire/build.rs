use std::path::PathBuf;

use common::errors::*;
use common::line_builder::LineBuilder;
use usb::descriptor_builders::DescriptorSetBuilder;
use usb::descriptors::*;
use usb::hid::*;
use usb::registry::*;

pub const EMPTY_STRING_INDEX: u8 = 0;

// TODO: Deprecate this one in favor of niche configs per device type.
fn generate_protocol_usb_descriptors(
    struct_name: &str,
    product_name: &str,
    product_id: u16,
) -> Result<String> {
    let mut builder = DescriptorSetBuilder::new();

    let manufacturer_string = builder.add_string("da!");
    let product_string = builder.add_string(product_name);

    let mut builder = builder.with_device(DeviceDescriptor {
        bLength: 0,         // Set by builder
        bDescriptorType: 0, // Set by builder
        bcdUSB: 0x0200,     // 2.0
        bDeviceClass: 0,
        bDeviceSubClass: 0,
        bDeviceProtocol: 0,
        bMaxPacketSize0: 64,
        idVendor: OUR_VENDOR_ID,
        idProduct: product_id,
        bcdDevice: 0x0100, // 1.0,
        iManufacturer: manufacturer_string,
        iProduct: product_string,
        iSerialNumber: EMPTY_STRING_INDEX,
        bNumConfigurations: 0, // Set by builder
    });

    let mut config_builder = builder.add_config(ConfigurationDescriptor {
        bLength: 0,
        bDescriptorType: 0,
        wTotalLength: 0,
        bNumInterfaces: 0,
        bConfigurationValue: 0,
        iConfiguration: EMPTY_STRING_INDEX,
        // TODO: Double check this
        bmAttributes: 0xa0, // Bus Powered : Remote wakeup
        bMaxPower: 50,
    });

    /*
    config_builder
        .add_interface(
            "",
            InterfaceDescriptor {
                bLength: 0,
                bDescriptorType: 0,
                bInterfaceNumber: 0,
                bAlternateSetting: 0,
                bNumEndpoints: 0,
                bInterfaceClass: 0, // TODO
                bInterfaceSubClass: 0,
                bInterfaceProtocol: 0,
                iInterface: 0,
            },
        )
        .add_endpoint(
            "",
            EndpointDescriptor {
                bLength: core::mem::size_of::<EndpointDescriptor>() as u8,
                bDescriptorType: DescriptorType::ENDPOINT as u8,
                bEndpointAddress: 0x81, // EP IN 1
                bmAttributes: 0b11,     // Interrupt
                wMaxPacketSize: 64,
                bInterval: 64, // TODO: Check me.
            },
        )
        .add_endpoint(
            "",
            EndpointDescriptor {
                bLength: core::mem::size_of::<EndpointDescriptor>() as u8,
                bDescriptorType: DescriptorType::ENDPOINT as u8,
                bEndpointAddress: 0x02, // EP OUT 2
                bmAttributes: 0b11,     // Interrupt
                wMaxPacketSize: 64,
                bInterval: 64, // TODO: Check me.
            },
        );
    */

    config_builder.add_dfu_runtime_interface();

    drop(config_builder);

    builder.generate_code(struct_name)
}

fn generate_bootloader_usb_descriptors() -> Result<String> {
    let mut builder = DescriptorSetBuilder::new();

    let manufacturer_string = builder.add_string("da!");
    let product_string = builder.add_string("bootloader");

    let mut builder = builder.with_device(DeviceDescriptor {
        bLength: 0,         // Set by builder
        bDescriptorType: 0, // Set by builder
        bcdUSB: 0x0200,     // 2.0
        bDeviceClass: 0,
        bDeviceSubClass: 0,
        bDeviceProtocol: 0,
        bMaxPacketSize0: 64,
        idVendor: OUR_VENDOR_ID,
        idProduct: OUR_BOOTLOADER_PRODUCT_ID,
        bcdDevice: 0x0100, // 1.0,
        iManufacturer: manufacturer_string,
        iProduct: product_string,
        iSerialNumber: EMPTY_STRING_INDEX,
        bNumConfigurations: 0, // Set by builder
    });

    builder
        .add_config(ConfigurationDescriptor {
            bLength: 0,
            bDescriptorType: 0,
            wTotalLength: 0,
            bNumInterfaces: 0,
            bConfigurationValue: 0,
            iConfiguration: EMPTY_STRING_INDEX,
            // TODO: Double check this
            bmAttributes: 0xa0, // Bus Powered : Remote wakeup
            bMaxPower: 50,
        })
        .add_dfu_host_interface();

    builder.generate_code("BootloaderUSBDescriptors")
}

fn generate_keyboard_usb_descriptors() -> Result<String> {
    let mut builder = DescriptorSetBuilder::new();

    let manufacturer_string = builder.add_string("da!");
    let product_string = builder.add_string("keyboard");

    let mut builder = builder.with_device(DeviceDescriptor {
        bLength: 0,         // Set by builder
        bDescriptorType: 0, // Set by builder
        bcdUSB: 0x0200,     // 2.0
        bDeviceClass: 0,
        bDeviceSubClass: 0,
        bDeviceProtocol: 0,
        bMaxPacketSize0: 64,
        idVendor: OUR_VENDOR_ID,
        idProduct: OUR_KEYBOARD_PRODUCT_ID,
        bcdDevice: 0x0100, // 1.0,
        iManufacturer: manufacturer_string,
        iProduct: product_string,
        iSerialNumber: EMPTY_STRING_INDEX,
        bNumConfigurations: 0, // Set by builder
    });

    let mut config_builder = builder.add_config(ConfigurationDescriptor {
        bLength: 0,
        bDescriptorType: 0,
        wTotalLength: 0,
        bNumInterfaces: 0,
        bConfigurationValue: 0,
        iConfiguration: EMPTY_STRING_INDEX,
        bmAttributes: 0xa0, // Bus Powered : Remote wakeup
        bMaxPower: 250,     // 500mA
    });

    let report_descriptor = standard_keyboard_report_descriptor();

    config_builder
        .add_interface(
            "::usb::hid::HIDInterfaceNumberTag",
            InterfaceDescriptor {
                bLength: 0,
                bDescriptorType: 0,
                bInterfaceNumber: 0,
                bAlternateSetting: 0,
                bNumEndpoints: 0,
                bInterfaceClass: InterfaceClass::HID.to_value(),
                bInterfaceSubClass: HIDInterfaceSubClass::Boot.to_value(),
                bInterfaceProtocol: HIDInterfaceBootProtocol::Keyboard.to_value(),
                iInterface: 0,
            },
        )
        .add_generic_descriptor(HIDDescriptor {
            bLength: core::mem::size_of::<HIDDescriptor>() as u8,
            bDescriptorType: HIDDescriptorType::HID.to_value(),
            bcdHID: 0x0101,
            bCountryCode: HIDCountryCode::US.to_value(),
            bNumDescriptors: 1,
            bReportDescriptorType: HIDDescriptorType::Report.to_value(),
            wReportDescriptorLength: report_descriptor.len() as u16,
        })
        .add_endpoint(
            "::usb::hid::HIDInterruptInEndpointTag",
            EndpointDescriptor {
                bLength: core::mem::size_of::<EndpointDescriptor>() as u8,
                bDescriptorType: DescriptorType::ENDPOINT as u8,
                bEndpointAddress: 0x81, // EP IN 1
                bmAttributes: 0b11,     // Interrupt
                wMaxPacketSize: 8,      // TODO: Keep this in sync with the keyboard report size.
                bInterval: 1,           // Poll every 1ms for a key change.
            },
        );

    config_builder.add_dfu_runtime_interface();

    drop(config_builder);

    let mut lines = LineBuilder::new();

    lines.add(builder.generate_code("KeyboardUSBDescriptors")?);

    lines.add(format!(
        "pub const KEYBOARD_HID_REPORT_DESCRIPTOR: &'static [u8] = &{:?};",
        report_descriptor
    ));

    Ok(lines.to_string())
}

fn generate_usb_descriptors() -> Result<()> {
    let input_dir = std::env::current_dir()?;
    let output_dir = PathBuf::from(std::env::var("OUT_DIR")?);

    let mut lines = LineBuilder::new();
    lines.add(generate_protocol_usb_descriptors(
        "RadioDongleUSBDescriptors",
        "radio dongle",
        OUR_RADIO_DONGLE_PRODUCT_ID,
    )?);
    lines.add(generate_protocol_usb_descriptors(
        "RadioSerialUSBDescriptors",
        "radio serial",
        OUR_RADIO_SERIAL_PRODUCT_ID,
    )?);
    lines.add(generate_protocol_usb_descriptors(
        "BlinkUSBDescriptors",
        "blink",
        OUR_BLINK_PRODUCT_ID,
    )?);
    lines.add(generate_bootloader_usb_descriptors()?);
    lines.add(generate_keyboard_usb_descriptors()?);

    std::fs::create_dir_all(output_dir.join("src"));
    std::fs::write(output_dir.join("src/usb_descriptors.rs"), lines.to_string())?;

    /*
    std::fs::write(input_dir.join("src/usb_descriptors.rs"), lines.to_string())?;
    */

    Ok(())
}

fn main() {
    generate_usb_descriptors().unwrap();
}
