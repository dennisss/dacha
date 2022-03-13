use std::time::Duration;

use common::errors::*;
use nordic_proto::packet::PacketBuffer;
use nordic_proto::proto::net::*;
use nordic_proto::usb::ProtocolUSBRequestType;
use protobuf::Message;
use usb::descriptors::SetupPacket;

// TODO: Every single USB transfer should have some timeout.
pub struct USBRadio {
    device: usb::Device,
}

impl USBRadio {
    pub async fn find(selected_id: Option<&str>) -> Result<Self> {
        let ctx = usb::Context::create()?;

        let mut device = {
            let mut found_device = None;

            for dev in ctx.enumerate_devices().await? {
                let desc = dev.device_descriptor()?;
                if desc.idVendor != 0x8888 || desc.idProduct != 0x0001 {
                    continue;
                }

                let id = format!("{}:{}", dev.bus_num(), dev.dev_num());
                println!("Device: {}", id);

                if selected_id.is_none()
                    || selected_id.unwrap() == id
                    || selected_id.unwrap() == "any"
                {
                    found_device = Some(dev.open().await?);
                }
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
                    bRequest: ProtocolUSBRequestType::SetNetworkConfig.to_value(),
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
                    bRequest: ProtocolUSBRequestType::GetNetworkConfig.to_value(),
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
                    bRequest: ProtocolUSBRequestType::Send.to_value(),
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
            match common::async_std::future::timeout(
                Duration::from_millis(5),
                self.device.read_control(
                    SetupPacket {
                        bmRequestType: 0b11000000,
                        bRequest: ProtocolUSBRequestType::Receive.to_value(),
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
}
