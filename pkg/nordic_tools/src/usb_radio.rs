use std::time::Duration;

use common::errors::*;
use nordic_proto::packet::PacketBuffer;
use nordic_proto::proto::log::LogEntry;
use nordic_proto::proto::net::*;
use nordic_proto::request_type::ProtocolRequestType;
use protobuf::{Message, StaticMessage};
use usb::{descriptors::SetupPacket, registry::OUR_VENDOR_ID};

// TODO: Every single USB transfer should have some timeout.
pub struct USBRadio {
    device: usb::Device,
}

impl USBRadio {
    pub async fn find(device_selector: &usb::DeviceSelector) -> Result<Self> {
        let ctx = usb::Context::create()?;

        let mut device = {
            let mut found_device = None;

            for dev in ctx.enumerate_devices().await? {
                if !device_selector.matches(&dev)? {
                    continue;
                }

                let id = format!("{}.{}", dev.bus_num(), dev.dev_num());
                println!("Device: {}", id);

                found_device = Some(dev.open().await?);
            }

            found_device.ok_or_else(|| err_msg("No device selected"))?
        };

        println!("Device opened!");

        device.reset()?;
        println!("Device reset!");

        Ok(Self::new(device))
    }

    pub fn new(device: usb::Device) -> Self {
        Self { device }
    }

    pub async fn set_network_config(&mut self, config: &NetworkConfig) -> Result<()> {
        let proto = config.serialize()?;
        self.device
            .write_control(
                SetupPacket {
                    bmRequestType: 0b01000000,
                    bRequest: ProtocolRequestType::SetNetworkConfig.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: proto.len() as u16,
                },
                &proto,
            )
            .await?;
        Ok(())
    }

    pub async fn get_network_config(&mut self) -> Result<Option<NetworkConfig>> {
        let mut read_buffer = [0u8; 256];
        // TODO: Set a timeout on this and reset the device on failure.
        let n = self
            .device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b11000000,
                    bRequest: ProtocolRequestType::GetNetworkConfig.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: read_buffer.len() as u16,
                },
                &mut read_buffer,
            )
            .await?;

        if n == 0 {
            return Ok(None);
        }

        Ok(Some(NetworkConfig::parse(&read_buffer[0..n])?))
    }

    pub async fn send_packet(&mut self, packet: &PacketBuffer) -> Result<()> {
        // TODO: Support retrying this (must consider the idempotence of actions).
        self.device
            .write_control(
                SetupPacket {
                    bmRequestType: 0b01000000,
                    bRequest: ProtocolRequestType::Send.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: packet.as_bytes().len() as u16,
                },
                packet.as_bytes(),
            )
            .await?;

        Ok(())
    }

    /// NOTE: Does not block if a packet isn't currently available.
    pub async fn recv_packet(&mut self) -> Result<Option<PacketBuffer>> {
        let mut packet_buffer = PacketBuffer::new();

        let mut num_bytes = None;
        for attempt in 0..4 {
            match executor::timeout(
                Duration::from_millis(5),
                self.device.read_control(
                    SetupPacket {
                        bmRequestType: 0b11000000,
                        bRequest: ProtocolRequestType::Receive.to_value(),
                        wValue: 0,
                        wIndex: 0,
                        wLength: packet_buffer.raw_mut().len() as u16,
                    },
                    packet_buffer.raw_mut(),
                ),
            )
            .await
            {
                Ok(Ok(n)) => {
                    num_bytes = Some(n);
                    break;
                }
                Err(_) => {
                    // Timeout
                    println!("Retrying read_control {}", attempt);
                    continue;
                }

                Ok(Err(e)) => {
                    // Internal USB error
                    return Err(e);
                }
            }
        }

        let num_bytes = num_bytes.ok_or_else(|| err_msg("Ran out of USB retries"))?;

        if num_bytes > 0 {
            Ok(Some(packet_buffer))
        } else {
            Ok(None)
        }
    }

    pub async fn read_log_entries(&mut self) -> Result<Vec<LogEntry>> {
        let mut buffer = [0u8; 256];
        let n = self
            .device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b11000000,
                    bRequest: ProtocolRequestType::ReadLog.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: buffer.len() as u16,
                },
                &mut buffer,
            )
            .await?;

        let mut out = vec![];

        let mut i = 0;
        while i < n {
            let len = buffer[i] as usize;
            i += 1;

            if i + len > n {
                return Err(err_msg("Log entry larger than buffer length"));
            }

            let data = &buffer[i..(i + len)];
            i += len;

            out.push(LogEntry::parse(data)?);
        }

        Ok(out)
    }
}
