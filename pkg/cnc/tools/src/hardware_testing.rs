/*
cargo run --bin cnc_controller -- service \
    --config_name=voron0 \
    --port=8000

TODO: Don't immediately energize the motors when the printer is turned on.

cargo run --bin cnc_tools -- \
    test-led-strips

*/

use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use peripherals_service::device::PeripheralsDevice;
use cnc_controller_proto::cnc::*;

use crate::remote::*;

#[derive(Args)]
pub struct TestLedStripsCommand {

}

impl TestLedStripsCommand {

    async fn set_colors(
        machine: &mut RemoteMachineController,
        left: Option<[u8; 4]>,
        right: Option<[u8; 4]>,
        toolhead: Option<[u8; 4]>,
        bed: Option<[u8; 4]>,
    ) -> Result<()> {
        let mut req = ExecuteRequest::default();

        if let Some(c) = left {
            let cmd = req.new_commands();
            cmd.set_led_strip_mut().set_name("left");
            let data = cmd.set_led_strip_mut().data_mut();
            for i in 0..11 {
                data.extend_from_slice(&c);
            }
        }

        if let Some(c) = right {
            let cmd = req.new_commands();
            cmd.set_led_strip_mut().set_name("right");
            let data = cmd.set_led_strip_mut().data_mut();
            for i in 0..11 {
                data.extend_from_slice(&c);
            }
        }

        if let Some(c) = toolhead {
            let cmd = req.new_commands();
            cmd.set_led_strip_mut().set_name("toolhead");
            let data = cmd.set_led_strip_mut().data_mut();
            data.extend_from_slice(&[0, 0, 0]);
            for i in 0..2 {
                data.extend_from_slice(&c);
            }
        }

        if let Some(c) = bed {
            let cmd = req.new_commands();
            cmd.set_led_strip_mut().set_name("bed");
            let data = cmd.set_led_strip_mut().data_mut();
            data.extend_from_slice(&c);
        }

        machine.execute(&req).await?;

        Ok(())

    }

    pub async fn run(self) -> Result<()> {
        let mut machine = RemoteMachineController::create().await?;

        let blank = [0u8; 4];

        Self::set_colors(&mut machine, Some(blank), Some(blank), Some(blank), Some(blank)).await?;
        executor::sleep(Duration::from_millis(1000)).await?;

        Self::set_colors(&mut machine, Some([0, 0, 0xff, 0]), None, None, None).await?;
        executor::sleep(Duration::from_millis(1000)).await?;

        Self::set_colors(&mut machine, None, Some([0, 0, 0xff, 0]), None, None).await?;
        executor::sleep(Duration::from_millis(1000)).await?;

        Self::set_colors(&mut machine, None, None, Some([0, 0, 0xff, 0]), None).await?;
        executor::sleep(Duration::from_millis(1000)).await?;

        Self::set_colors(&mut machine, None, None, None, Some([0xff, 0, 0, 0])).await?;
        executor::sleep(Duration::from_millis(1000)).await?;

        // &[0, 0, 0xff, 0]

        

        /*
        let mut epoch = 0;
        loop {
            let mut req = ExecuteRequest::default();

            for name in ["left", "right"] {
                let cmd = req.new_commands();

                cmd.set_led_strip_mut().set_name(name);

                let data = cmd.set_led_strip_mut().data_mut();
                for i in 0..11 {
                    // GRBW
                    // data.extend_from_slice(&[0, 0, 0xff, 0]);

                    let color = match (i + epoch) % 3 {
                        0 => &[0, 0, 0xff, 0],
                        1 => &[0, 0xff, 0, 0],
                        2 => &[0xff, 0, 0, 0],
                        _ => panic!()
                    };
                    data.extend_from_slice(color);

                }
            }

            {
                let cmd = req.new_commands();

                cmd.set_led_strip_mut().set_name("toolhead");

                let data = cmd.set_led_strip_mut().data_mut();

                
                // GRB
                data.extend_from_slice(&[0, 0, 0]);

                // GRBW
                let mut color = [0u8; 4];
                color[epoch % 3] = 0xff;

                // GRBW
                data.extend_from_slice(&color);
                // GRBW
                data.extend_from_slice(&color);
            }


            {
                let cmd = req.new_commands();

                cmd.set_led_strip_mut().set_name("bed");

                let data = cmd.set_led_strip_mut().data_mut();

                // BGRW
                let mut color = [0u8; 4];
                color[epoch % 3] = 0xff;

                data.extend_from_slice(&color);
            }

            machine.execute(&req).await?;


            epoch += 1;
            executor::sleep(Duration::from_millis(250)).await?;
        }

        */







        Ok(())
    }
    
    async fn run_direct(self) -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"voron0_aux")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        let mut colors = vec![];
        // 
        for i in 0..11 {
            colors.extend_from_slice(&[00, 0, 0xff, 0]);
        }

        device.neopixel_transfer("leds_left", 0, &colors[..]).await?;
        device.neopixel_show("leds_left").await?;

        device.neopixel_transfer("leds_right", 0, &colors[..]).await?;
        device.neopixel_show("leds_right").await?;

        Ok(())
    }
}