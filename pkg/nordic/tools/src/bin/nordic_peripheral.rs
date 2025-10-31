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
use peripherals_proto::peripherals::*;

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
    
    let mut selector = usb::DeviceSelector::default();
    selector.vendor_id = Some(0x8888);
    selector.product_id = Some(0x0004);

    let mut dev = nordic_tools::usb_radio::USBRadio::find(&selector).await?;



    // Unconfiugre
    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(0 as u32);
        req.unconfigure_all_mut();
        dev.send_request(&req).await?;
    }

    // {
    //     let mut req = PeripheralRequest::default();
    //     req.set_peripheral_index(0 as u32);
    //     req.configure_stepper_mut().set_step_pin((15 + 32) as u32);
    //     req.configure_stepper_mut().set_dir_pin(31 as u32);
    //     dev.send_request(&req).await?;
    // }

    // Neopixel data
    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(1 as u32);
        req.configure_neopixel_mut().set_pin(16 as u32);
        dev.send_request(&req).await?;
    }

    // Neopixel power
    {
        // neopixel power
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(2 as u32);
        req.configure_gpio_mut().set_is_input(false);
        req.configure_gpio_mut().set_pin((32 + 14) as u32);
        dev.send_request(&req).await?;
    }

        /*

pins {
    name: "D10"
    alias: "P0.27"
}
pins {
    name: "D9"
    alias: "P0.26"
}
    */
    // {
    //     let mut req = PeripheralRequest::default();
    //     req.set_peripheral_index(3 as u32);
    //     req.configure_uart_mut().set_tx_pin(26u32);
    //     req.configure_uart_mut().set_rx_pin(27u32);
    //     req.configure_uart_mut().set_baud_rate(115200u32);
    //     dev.send_request(&req).await?;
    // }

    // Finalize config
    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(0 as u32);
        req.finalize_config_mut();
        dev.send_request(&req).await?;
    }

    {
        // 1.14

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(2 as u32);
        req.set_gpio_level_mut().set_high(true);
        dev.send_request(&req).await?;
    }

    println!("xxx");

    // executor::sleep(Duration::from_secs(10)).await?;

    for i in 0..255 {
        println!("{}", i);

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(1 as u32);

        // G R B
        req.neopixel_transfer_mut().data_mut().extend_from_slice(&[
            0, 0, i, 0,
        ]);
        dev.send_request(&req).await?;

        executor::sleep(Duration::from_secs(1)).await;

    }
    {

    }

    /*
    loop {


        {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(3 as u32);
            req.uart_transmit_mut().data_mut().extend_from_slice(b"hello");
            req.uart_transmit_mut().rx_after_tx_mut().set_num_bytes(2u32);
            let res = dev.send_request(&req).await?;

            println!("UART: {:?}", res);
        }
        
    }
    */





    /*
pins {
    name: "RED_LED"
    alias: "P1.15"
}
pins {
    name: "NEOPIXEL"
    alias: "P0.16"
}
    */


    let mut last_value = 0;
    loop {
        let time = {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(0 as u32);
            req.set_get_clock_time(true);
            let res = dev.send_request(&req).await?;
            println!("{:?}", res);
            last_value = res.uint_val();
            res.uint_val()
        };


        /*

        let start_time = time + 16_000_000;

        {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(0 as u32);
            
            let m = req.enqueue_stepper_motion_mut();
            m.set_next_time(start_time);
            m.set_num_steps(16u32);
            m.set_next_velocity(4_000_000u32);

            let res = dev.send_request(&req).await?;
            println!("enqueue: {:?}", res);
        }

        executor::sleep(Duration::from_secs(6)).await;

        {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(0 as u32);
            req.set_get_stepper_motor_status(true);
            let res = dev.send_request(&req).await?;
            println!("status: {:?}", res);
        }
        */

        executor::sleep(Duration::from_secs(2)).await;

    }

    
    /*
    name: "LED1"
    alias: "P0.06"

    31
    */


    let pwm_pins: Vec<u32> = vec![12, 26, 32 + 8, 24];

    let tachometer_pins: Vec<u32> = vec![11, 4, 7, 28, 14, 16, 25, 20];


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
