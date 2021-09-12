use std::collections::{HashMap, HashSet};

use common::errors::*;

use crate::descriptor_iter::Descriptor;
use crate::descriptors::{SetupPacket, StandardRequestType};
use crate::endpoint::is_in_endpoint;
use crate::hid::descriptors::*;
use crate::hid::item::*;
use crate::linux::Device;

pub struct HIDDevice {
    device: Device,

    report_ids: HashSet<u8>,

    /// Number of the interface which is sent HID commands.
    iface: u8,

    /// Interrupt input endpoint which is used to receive reports
    /// asynchronously.
    in_endpoint: u8,

    // Optional interrupt used to send out Output reports.
    out_endpoint: Option<u8>,

    reports: Vec<Report>,
}

impl HIDDevice {
    pub async fn open_with_existing(mut device: Device) -> Result<Self> {
        if device.descriptor().bNumConfigurations != 1 {
            return Err(err_msg(
                "Currently only devices with one config are supported.",
            ));
        }

        // TODO: Validate in the USB driver that there are no duplicate interface
        // numbers.

        let mut selected_iface_hid = None;

        let mut endpoints_per_iface = HashMap::new();

        let mut current_iface = None;
        for desc in device.descriptors() {
            match desc {
                Descriptor::Interface(iface) => {
                    current_iface = Some((iface.bInterfaceNumber, iface.bAlternateSetting));
                }
                Descriptor::HID(hid) => {
                    if selected_iface_hid.is_some() {
                        return Err(err_msg("Multiple HID interfaces in device"));
                    }
                    selected_iface_hid = Some((current_iface.unwrap(), *hid));
                }
                Descriptor::Endpoint(ep) => {
                    let iface_key = current_iface
                        .clone()
                        .ok_or_else(|| err_msg("Saw endpoint descriptor output of interface"))?;

                    let mut list = endpoints_per_iface.remove(&iface_key).unwrap_or(vec![]);
                    list.push(*ep);

                    endpoints_per_iface.insert(iface_key, list);
                }
                _ => {}
            }
        }

        let ((iface, alt_setting), hid) = selected_iface_hid
            .ok_or_else(|| err_msg("Failed to find an HID interface on the device."))?;

        if device.kernel_driver_active(iface)? {
            println!("Detaching kernel driver.");
            device.detach_kernel_driver(iface)?;
        }

        device.claim_interface(iface)?;
        device.set_alternate_setting(iface, alt_setting)?;

        if hid.bReportDescriptorType != HIDDescriptorType::Report as u8 {
            return Err(err_msg(
                "Expected first HID descriptor to be the Report descriptor",
            ));
        }

        // Read out the report desriptor
        let report_items = {
            let mut report_desc_buffer = vec![];
            report_desc_buffer.resize(hid.wReportDescriptorLength as usize, 0);

            // NOTE: report descriptors only have an index of 0.
            let index = 0;

            let pkt = SetupPacket {
                bmRequestType: 0b10000001, // Specific to HID
                bRequest: StandardRequestType::GET_DESCRIPTOR as u8,
                wValue: ((HIDDescriptorType::Report as u16) << 8) | (index as u16),
                wIndex: iface as u16,
                wLength: hid.wReportDescriptorLength,
            };

            let nread = device.read_control(pkt, &mut report_desc_buffer).await?;
            if nread != report_desc_buffer.len() {
                return Err(err_msg("Failed to read full report descriptor"));
            }

            parse_items(&report_desc_buffer)?
        };

        let reports = parse_reports(&report_items)?;

        let mut report_ids = HashSet::new();

        // TODO: Verify that report ids are only 1 byte in length.
        for item in report_items {
            item.visit_all(&mut |item| {
                if let Item::Global { tag, value } = item {
                    if *tag == GlobalItemTag::ReportId {
                        report_ids.insert(*value as u8);
                    }
                }
            });
        }

        if report_ids.contains(&0) {
            return Err(err_msg("Device has an invalid report with id of 0"));
        }

        let endpoints = endpoints_per_iface
            .remove(&(iface, alt_setting))
            .unwrap_or_default();
        let mut in_endpoint = None;
        let mut out_endpoint = None;

        for endpoint in endpoints {
            if endpoint.bmAttributes & 0b11 != 0b11 {
                return Err(err_msg(
                    "Expected only interrupt endpoints on HID interface",
                ));
            }

            if is_in_endpoint(endpoint.bEndpointAddress) {
                if in_endpoint.is_some() {
                    return Err(err_msg("Multiple input endpoints on HID device"));
                }

                in_endpoint = Some(endpoint.bEndpointAddress);
            } else {
                if out_endpoint.is_some() {
                    return Err(err_msg("Multiple output endpoints on HID device"));
                }

                out_endpoint = Some(endpoint.bEndpointAddress);
            }
        }

        let in_endpoint =
            in_endpoint.ok_or_else(|| err_msg("HID interface missing required input endpoint"))?;

        // TODO: Use the report descriptor items to
        // 1. validate the presence of the report_id (otherwise must always be zero)
        // 2. validate the size of data in reports
        // 3. validate the ReportType attempted to be transfered.

        // TODO: Read the report descriptor
        // if hid.bReportDescriptorType

        Ok(HIDDevice {
            device,
            report_ids,
            iface,
            in_endpoint,
            out_endpoint,
            reports,
        })
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn reports(&self) -> &[Report] {
        &self.reports
    }

    pub async fn set_report(
        &self,
        report_id: u8,
        report_type: ReportType,
        data: &[u8],
    ) -> Result<()> {
        let extended_data = {
            if (report_id == 0) != self.report_ids.is_empty() {
                return Err(err_msg(
                    "A report id of 0 must be specified iff there are no reports configured",
                ));
            }

            if self.report_ids.is_empty() {
                // TODO: Verify that there are no reports.
                data.to_vec()
            } else {
                if !self.report_ids.contains(&report_id) {
                    return Err(format_err!("No report defined with id: {}", report_id));
                }

                let mut out = vec![];
                out.push(report_id);
                out.extend_from_slice(data);
                out
            }
        };

        // TODO: Check that the report type is present in the report descriptor

        if report_type == ReportType::Output {
            if let Some(ep) = self.out_endpoint.clone() {
                return self.device().write_interrupt(ep, &extended_data).await;
            }
        }

        self.device
            .write_control(
                SetupPacket {
                    bmRequestType: 0x21, // Constant value from HID spec.
                    bRequest: HIDRequestType::SET_REPORT.to_value(),
                    wValue: ((report_type.to_value() as u16) << 8) | (report_id as u16),
                    wIndex: self.iface as u16,
                    wLength: extended_data.len() as u16,
                },
                &extended_data,
            )
            .await
    }

    pub async fn get_report(
        &self,
        report_id: u8,
        report_type: ReportType,
        data: &mut [u8],
    ) -> Result<()> {
        if (report_id == 0) != self.report_ids.is_empty() {
            return Err(err_msg(
                "A report id of 0 must be specified iff there are no reports configured",
            ));
        }

        let data_offset = if self.report_ids.is_empty() { 0 } else { 1 };
        let mut expanded_data = vec![0u8; data.len() + data_offset];

        let nread = self
            .device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b10100001,
                    bRequest: HIDRequestType::GET_REPORT.to_value(),
                    wValue: ((report_type.to_value() as u16) << 8) | (report_id as u16),
                    wIndex: self.iface as u16,
                    wLength: expanded_data.len() as u16,
                },
                &mut expanded_data,
            )
            .await?;

        if data_offset == 1 {
            if expanded_data[0] != report_id {
                return Err(err_msg("GET_REPORT response missing report_id"));
            }
        }

        if nread != data.len() + data_offset {
            return Err(err_msg("Read too little"));
        }

        data.copy_from_slice(&expanded_data[data_offset..]);
        Ok(())
    }
}
