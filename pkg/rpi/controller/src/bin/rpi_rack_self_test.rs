#[macro_use]
extern crate macros;

use std::collections::HashSet;
use std::time::Duration;

use common::errors::*;
use common::io::Readable;
use common::InRange;
use peripherals::i2c::I2CHostController;
use peripherals_devices::ds3231::*;
use peripherals_devices::trust_m::*;
use rpi::fan::FanTachometerReader;
use rpi::fan::FAN_PWM_FREQUENCY;
use rpi::gpio::*;
use rpi::pwm::*;
use rpi::ws2812::*;

/*

ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.120

cargo build --target aarch64-unknown-linux-gnu --release --bin pi_rack_self_test

scp -i ~/.ssh/id_cluster target/aarch64-unknown-linux-gnu/release/pi_rack_self_test cluster-user@10.1.0.120:/home/cluster-user/pi_rack_self_test

*/

const MAX_FAN_RPM: usize = 5000;

async fn list_i2c_devices(bus: &mut I2CHostController) -> HashSet<u8> {
    let mut devices = HashSet::default();
    for addr in 0..128 {
        let valid = bus.test(addr).await.is_ok();
        if valid {
            devices.insert(addr);
        }
    }

    devices
}

#[executor_main]
async fn main() -> Result<()> {
    let mut gpio = GPIO::open()?;
    let mut rtc_power_pin = gpio.pin(4);
    let fan_tach_pin = gpio.pin(17);
    let fan_pwm_pin = gpio.pin(18);
    let led_serial_pin = gpio.pin(21);
    let mut i2c_bus = I2CHostController::open("/dev/i2c-1")?;

    let mut stdin = file::Stdin::get();

    println!("");
    println!("### TPM Tests ###");
    {
        let mut trust_m = TrustM::open(&i2c_bus).await?;
        let uid = trust_m.read_coprocessor_uid().await?;
        println!("=> TPM UID: {:02x?}", uid);
    }

    println!("");
    println!("### Fan Tests ###");
    {
        let mut fan_pwm = SysPWM::open(fan_pwm_pin).await?;

        let mut fan_tach = FanTachometerReader::create(fan_tach_pin);

        for i in 4..11 {
            let duty = 0.1 * (i as f32);
            fan_pwm.write(FAN_PWM_FREQUENCY, 1.0 - duty).await?;

            // Wait for the fan to finish changing speed.
            executor::sleep(Duration::from_secs(4)).await?;

            const NUM_SAMPLES: usize = 10;
            let mut rpm = 0;
            for i in 0..NUM_SAMPLES {
                rpm += fan_tach.read().await?;
            }
            rpm /= NUM_SAMPLES;

            let expected_rpm = (duty * (MAX_FAN_RPM as f32)) as usize;
            println!("Duty: {}, RPM: {} (expected: {})", duty, rpm, expected_rpm);

            let diff = ((expected_rpm as isize) - (rpm as isize)).abs();
            if diff > 400 {
                return Err(format_err!(
                    "RPM is too far from expectations: {} actual vs {} expected (|diff|: {})",
                    rpm,
                    expected_rpm,
                    diff
                ));
            }
        }
    }

    println!("");
    println!("### LED Tests ###");
    {
        let test_cases = [
            ("Top Red", &[0xFF0000, 0x000000]),
            ("Bottom Blue", &[0x000000, 0x0000FF]),
            ("Both White", &[0xFFFFFF, 0xFFFFFF]),
            ("Both Off", &[0x000000, 0x000000]),
        ];

        let mut led = WS2812Controller::create(led_serial_pin).await?;

        for (desc, colors) in test_cases {
            println!("Please verify test Case: {}", desc);
            led.write(colors)?;

            println!("= Waiting for user to press [Enter] =");
            let mut buf = [0];
            stdin.read(&mut buf).await?;
        }
    }

    println!("");
    println!("### RTC Tests ###");
    {
        println!("- Turning RTC off explicitly.");

        // Setup and ensure powered off.
        rtc_power_pin
            .set_mode(Mode::Output)
            .set_resistor(Resistor::None) // This is an external pullup.
            .write(true);

        // Wait for the RTC to turn off.
        executor::sleep(Duration::from_secs(1)).await?;

        // Verify that the I2C device doesn't show up.
        {
            let devs = list_i2c_devices(&mut i2c_bus).await;
            if devs.contains(&DS3231::I2C_ADDRESS) {
                return Err(err_msg(
                    "RTC still on despite being turned off (did you remove the battery?)",
                ));
            }
        }

        // Turn the RTC back on.
        println!("- Turning RTC back on");
        rtc_power_pin.write(false);

        // Wait for the RTC to turn on.
        executor::sleep(Duration::from_secs(1)).await?;

        {
            let devs = list_i2c_devices(&mut i2c_bus).await;
            if !devs.contains(&DS3231::I2C_ADDRESS) {
                return Err(err_msg("RTC hasn't been detected after applying power"));
            }
        }

        let mut clock = DS3231::open(&i2c_bus);

        // Verify that temperature is a reasonable number
        let temp = clock.read_temperature().await?;
        println!("- RTC Temp: {} C", temp);
        if !temp.in_range(18.0, 40.0) {
            return Err(err_msg("Extreme temperature in RTC"));
        }

        println!("- Testing passage of time for 20s");
        clock
            .write_time(&DS3231Time::from_atomic_seconds(100))
            .await?;
        executor::sleep(Duration::from_secs(20)).await?;

        let new_time = clock.read_time().await?.to_atomic_seconds();
        println!("=> 100s + 20s = {}", new_time);
        if !new_time.in_range(119, 121) {
            return Err(err_msg("Bad clock"));
        }

        // Verify that turning off the RTC clears the time.
        println!("- Power cycle RTC without battery");
        rtc_power_pin.write(true);
        executor::sleep(Duration::from_secs(1)).await?;
        rtc_power_pin.write(false);
        executor::sleep(Duration::from_secs(1)).await?;

        let new_time = clock.read_time().await?.to_atomic_seconds();
        println!("- Reset time: {}", new_time);
        if new_time > 4 {
            return Err(err_msg("Time did not reset after power cycling"));
        }

        println!("= Please install the RTC battery and press [Enter] =");
        let mut buf = [0];
        stdin.read(&mut buf).await?;

        clock
            .write_time(&DS3231Time::from_atomic_seconds(100))
            .await?;

        println!("- Power cycle with battery");
        rtc_power_pin.write(true);
        executor::sleep(Duration::from_secs(1)).await?;
        rtc_power_pin.write(false);
        executor::sleep(Duration::from_secs(1)).await?;

        let new_time = clock.read_time().await?.to_atomic_seconds();
        println!("- Latest time: {}", new_time);

        if new_time < 100 {
            return Err(err_msg(
                "RTC did not preserve time after power cycle with battery backup",
            ));
        }
    }

    println!("=> All tests passed! (verify human steps yourself)");

    Ok(())
}
