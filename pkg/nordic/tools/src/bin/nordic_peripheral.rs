extern crate common;
extern crate usb;
#[macro_use]
extern crate macros;

use std::fmt::Write;
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::errors::*;
use nordic_proto::nordic::*;
use nordic_wire::request_type::ProtocolRequestType;
use protobuf::{Message, StaticMessage};
use usb::descriptor_iter::DescriptorIter;
use usb::descriptors::SetupPacket;
use usb::DescriptorSet;

struct PeripheralDevice {
    device: usb::Device,
    last_sequence: u32,
}

impl PeripheralDevice {
    pub async fn create() -> Result<Self> {
        let ctx = usb::Context::create()?;

        let mut device = ctx.open_device(0x8888, 0x0004).await?;

        println!("Opened");

        device.reset()?;

        Ok(Self {
            device,
            last_sequence: 0,
        })
    }

    pub async fn send_request(
        &mut self,
        request: &PeripheralRequest,
    ) -> Result<PeripheralResponse> {
        // TODO: Reset the sequence after a while.

        let mut request = request.clone();
        self.last_sequence += 1;
        request.set_request_sequence(self.last_sequence);

        let proto = request.serialize()?;
        // println!("Send: {}", packet.len());

        let mut packet = vec![];
        packet.push(proto.len() as u8);
        packet.extend_from_slice(&proto);
        if packet.len() < 64 {
            packet.resize(64, 0);
        }

        // if packet.len() < 9 {
        //     packet.resize(9, 0);
        // }

        // let packet2 = packet.clone();
        // packet.extend_from_slice(&packet2);

        // println!("{:?}", packet);
        // {
        //     let request2 = PeripheralRequest::parse(&packet)?;
        //     println!("REQ: {:?}", request2);
        // }

        let start = Instant::now();

        // println!("TX>");

        // TODO: Support retrying this (must consider the idempotence of actions).
        self.device
            .write_control(
                SetupPacket {
                    bmRequestType: 0b01000000,
                    bRequest: ProtocolRequestType::PeripheralRequest.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: packet.len() as u16,
                },
                &packet,
            )
            .await?;

        let mut res_buffer = [0u8; 256];

        // TODO: Need to ignore empty responses.

        // println!("RX>");

        let mut nread = 0;

        loop {
            nread = self
                .device
                .read_control(
                    SetupPacket {
                        bmRequestType: 0b11000000,
                        bRequest: ProtocolRequestType::PeripheralResponse.to_value(),
                        wValue: 0,
                        wIndex: 0,
                        wLength: res_buffer.len() as u16,
                    },
                    &mut res_buffer,
                )
                .await?;

            if nread != 0 {
                break;
            }

            executor::sleep(Duration::from_millis(10)).await?;
        }

        //

        let end = Instant::now();

        let response = PeripheralResponse::parse(&res_buffer[0..nread])?;

        // TODO: Verify it has the same sequence.

        // println!("Response: {} | {:?} | {:?}", nread, end - start, response);

        Ok(response)
    }
}

/*
- Top row: Fan 1, Fan 3, Fan 5, Fan 6, Fan 7
- Bottom row: Fan 2, Fan 4, Fan 8

- Must pull up tachometer inputs

- D13 / P0.12 : Fan 1/2 PWM
- D12 / P0.11 : Fan 1 Tachometer
- D11 / P0.26 : Tan 3/4 PWM
- D9 / P0.07 : Fan 3 Tachometer
- D7 / P1.08 : Fan 5/6 PWM
- SCL / P0.14 : Fan 5 Tachometer
- SDA / P0.16 : Fan 6 Tachometer
- D1 / P0.24 : Fan 7/8 PWM
- D0 / P0.25 : Fan 7 Tachometer
- A0 / P0.04 : Fan 2 Tachometer
- A2 / P0.28 : Fan 4 Tachometer
- MISO / P0.20 : Fan 8 Tachometer


First application:

- Setup all pins
- Loop over fan controls
    - Set to static value
        - 50% overall
- Loop over fans
    - Measure tachometer value
- Sleep to limit amax rate.


*/

#[executor_main]
async fn main() -> Result<()> {
    // TODO: Verify that unconfiguring the PWM actually sets the thing back to 0

    let mut dev = PeripheralDevice::create().await?;

    let pwm_pins: Vec<u32> = vec![12, 26, 32 + 8, 24];

    let tachometer_pins: Vec<u32> = vec![11, 4, 7, 28, 14, 16, 25, 20];

    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(0 as u32);
        req.unconfigure_all_mut();
        dev.send_request(&req).await?;
    }

    for (i, pin) in pwm_pins.iter().cloned().enumerate() {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(i as u32);
        req.configure_pwm_mut();
        req.configure_pwm_mut().set_pin(pin);
        req.configure_pwm_mut().set_inverted(true);
        req.configure_pwm_mut()
            .set_default_value(((u16::MAX as f32) * 0.8) as u32);
        req.configure_pwm_mut().set_frequency(25000 as u32);
        req.configure_pwm_mut().set_timeout_millis(10000 as u32);
        dev.send_request(&req).await?;
    }

    for (i, pin) in tachometer_pins.iter().cloned().enumerate() {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index((pwm_pins.len() + i) as u32);
        req.configure_gpio_mut().set_is_input(true);
        req.configure_gpio_mut().set_pin(pin);
        req.configure_gpio_mut().set_pull_up(true);
        dev.send_request(&req).await?;
    }

    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(0 as u32);
        req.finalize_config_mut();
        dev.send_request(&req).await?;
    }

    println!("===");

    /*
    TODO: For some reason, sending exactly 9 bytes breaks things.

    */

    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(0 as u32);
        req.set_measure_mcu_temperature(true);
        dev.send_request(&req).await?;
    }

    loop {
        println!("CYCLE ===");

        for i in 0..pwm_pins.len() {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(i as u32);
            req.set_pwm_mut()
                .set_value(((((1 << 16) - 1) as f32) * (0.5)) as u32);
            dev.send_request(&req).await?;
        }

        let mut samples = vec![];
        for i in 0..tachometer_pins.len() {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index((pwm_pins.len() + i) as u32);
            req.read_tachometer_mut();
            let mut res = dev.send_request(&req).await?;
            samples.push(res.uint_val());
        }

        println!("Speed: {:?}", samples);

        executor::sleep(Duration::from_secs(2)).await;
    }

    return Ok(());

    //     executor::sleep(Duration::from_millis(1000)).await?;
    // }

    /*
    {

    }
    */

    loop {
        /*
        {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(0 as u32);
            req.set_gpio_level_mut().set_high(false);
            dev.send_request(&req).await?;
        }
        executor::sleep(Duration::from_secs(1)).await?;

        {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(0 as u32);
            req.set_gpio_level_mut().set_high(true);
            dev.send_request(&req).await?;
        }
        executor::sleep(Duration::from_secs(1)).await?;
        */

        for i in 0..50 {
            {
                let mut req = PeripheralRequest::default();
                req.set_peripheral_index(0 as u32);
                req.set_pwm_mut()
                    .set_value(((((1 << 16) - 1) as f32) * ((i as f32) / (50 as f32))) as u32);
                dev.send_request(&req).await?;
            }

            executor::sleep(Duration::from_millis(100)).await?;
        }
    }

    //

    Ok(())
}
