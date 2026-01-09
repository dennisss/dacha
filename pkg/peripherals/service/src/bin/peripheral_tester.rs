#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::{collections::HashMap, sync::Arc, time::Instant};
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

use base_error::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use peripherals_service::config::*;
use peripherals_service::device::*;
use peripherals_proto::peripherals::PeripheralRequest;
use peripherals_service::utilization_tracker::*;

/*
cargo run --bin builder -- build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840

cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle uf2-dfu --usb_device_id=8888:

cargo run --bin peripheral_tester
*/

#[executor_main]
async fn main() -> Result<()> {

    let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

    // let config = configs.remove(&"hall_tile")
    //     .ok_or_else(|| err_msg("No config with the given name"))?;
    let config = configs.remove(&"breadboard_toolhead")
        .ok_or_else(|| err_msg("No config with the given name"))?;
    // let config = configs.remove(&"voron0_main")
    //     .ok_or_else(|| err_msg("No config with the given name"))?;


    let (mut device, _) = PeripheralsDevice::create(&config).await?;

    let device = Arc::new(device);

    // let mut req = PeripheralRequest::default();
    // protobuf::text::parse_text_proto(r#"
    //     peripheral_index: 8
    //     enqueue_stepper_motion {
    //         next_step_time: 495054323
    //         next_step_duration: 10992
    //         step_duration_increment: 84
    //         num_steps_minus_one: 12
    //     }
    // "#, &mut req)?;

    // let res = device.raw().send_request(&req).await?;

    // println!("{:?}", res);

    // return Ok(());

    /*


    */

    /*
    loop {
        // let value = device.gpio_

        match executor::timeout(Duration::from_millis(1000), device.poll_gpio_interrupt("button")).await {
            Ok(r) => {
                r?;
                println!("Hit!");
                executor::sleep(Duration::from_millis(100)).await;
            }
            Err(_) => {
                println!("Timeout");
            }
        }

    }
    */

    /*
    loop {
        let start = Instant::now();
        let value = device.analog_read_window("value", "buf1").await?;
        let end = Instant::now();
        println!("Sample: {:?}", end - start);

        let start = Instant::now();
        let buf = device.analog_fetch_window("value", "buf1").await?;
        let end = Instant::now();

        println!("Fetch: {:?}", end - start);

        println!("{}", buf[0]);
        executor::sleep(Duration::from_millis(500)).await?;
    }
    */

    /*
    loop {
        let value = device.analog_read("value").await?;

        println!("{}", value);
        executor::sleep(Duration::from_millis(500)).await?;
    }
    */

    let util_tracker = RemoteUtilizationTracker::create();
    util_tracker.add_device("mcu", device.clone()).await?;

    let mut counter = Arc::new(AtomicU64::new(0));
    
    /*
    for i in 0..0 {
        let device2 = device.clone();
        let counter = counter.clone();
        executor::spawn(async move {
            // TODO: Ideally try to use biffer requests here.
            let mut req = PeripheralRequest::default();
            // req.set_peripheral_index(20u32);
            req.set_noop(true);

            loop {
                device2.send_request(&req).await.unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
    }
        */



    loop {
        let t = device.get_usb_sof_time().await?;
        let t2 = device.get_usb_sof_time().await?;


        // println!("{:?}", t);

        println!("get_usb_sof_time rtt: {:?} : {} : {}", t.timing.local_response_time - t.timing.local_request_time, t.frame_counter, t2.frame_counter);


        executor::sleep(Duration::from_millis(1000)).await?;
    }

    /*
    loop {
        let t = device.get_clock_time().await?;

        println!("get_clock_time rtt: {:?}", t.local_response_time - t.local_request_time);


        let t1 = Instant::now();
        let c1 = counter.fetch_add(0, Ordering::SeqCst);

        executor::sleep(Duration::from_millis(1000)).await?;

        let t2 = Instant::now();
        let c2 = counter.fetch_add(0, Ordering::SeqCst);

        println!("noop rate: {:.1}", ((c2 - c1) as f64) / (t2 - t1).as_secs_f64());
    }
        */

    /*
    loop {
        device.gpio_write("led", false).await?;
        executor::sleep(Duration::from_millis(1000)).await?;

        device.gpio_write("led", true).await?;
        executor::sleep(Duration::from_millis(1000)).await?;
    }
    */



    Ok(())
}
