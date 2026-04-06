// Tester for the electronics on the button boards.
// This directly connects to the boards over USB and assumes they are flashed with the nordic_radio_dongle firmware.

#[macro_use]
extern crate base_args;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;
#[macro_use]
extern crate file;

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



#[derive(Args)]
struct Args {
    mode: Mode
}

define_arg_command!(Mode {
    TestButtonCommand = "test-button",
    TestMagnetCommand = "test-magnet",
    TestBatteryCommand = "test-battery",
    TestHDC2080Command = "test-hdc2080",
    TestAccelCommand = "test-accel",
    TestAccelOrientationCommand = "test-accel-orientation",
    TestEinkCommand = "test-eink"
});

async fn create_button_device() -> Result<Arc<PeripheralsDevice>> {
    let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

    let config = configs.remove(&"button")
        .ok_or_else(|| err_msg("No config with the given name"))?;

    let (mut device, _) = PeripheralsDevice::create(&config).await?;

    let device = Arc::new(device);
    Ok(device)
}

#[derive(Args)]
struct TestButtonCommand {}

impl TestButtonCommand {
    async fn run(self) -> Result<()> {
        let device = create_button_device().await?;
        loop {
            println!("{}", device.gpio_read("button").await?);
            executor::sleep(Duration::from_secs(1)).await?;
        }
    }
}

#[derive(Args)]
struct TestMagnetCommand {}

impl TestMagnetCommand {
    async fn run(self) -> Result<()> {
        let device = create_button_device().await?;
        loop {
            println!("{}", device.gpio_read("magnet").await?);
            executor::sleep(Duration::from_secs(1)).await?;
        }
    }
}


// cargo run --bin button_tester -- test-eink --text=""
// cargo run --bin button_tester -- test-eink --text="Weather:\nSunny!"
// cargo run --bin button_tester -- test-eink --text="Subscribe!"
// cargo run --bin button_tester -- test-eink --text="Hello\nWorld"
#[derive(Args)]
struct TestEinkCommand {
    text: String,
}

impl TestEinkCommand {
    async fn run(self) -> Result<()> {
        println!("gen image...");
        let mut image = nordic_bitmaps::DisplayBuffer::new();
        image.draw_text(&self.text.replace("\\n", "\n"));

        let device = create_button_device().await?;

        device.gpio_write("eink_on", true).await?;
        executor::sleep(Duration::from_secs(1)).await?;

        let display = GDEY0213B74::new(device);

        println!("init...");
        display.init().await?;

        println!("update...");
        display.update(&image.buffer).await?;

        Ok(())
    }
}

pub const WIDTH: u32 = 122;
pub const HEIGHT: u32 = 250;
pub const BUFFER_SIZE: usize = ((WIDTH + 7) / 8 * HEIGHT) as usize; // 16 * 250 = 4000 bytes

pub struct GDEY0213B74 {
    device: Arc<PeripheralsDevice>,
}

impl GDEY0213B74 {
    pub fn new(device: Arc<PeripheralsDevice>) -> Self {
        Self { device }
    }

    /// Powers on and initializes the display from deep sleep or a cold boot [cite: 5150]
    pub async fn init(&self) -> Result<()> {
        self.hardware_reset().await?;

        // SW Reset [cite: 5157]
        self.send_command(0x12).await?;
        executor::sleep(Duration::from_millis(10)).await;
        self.wait_until_idle().await?;

        // Driver Output control: 250 rows (250 - 1 = 249 = 0xF9) [cite: 4220, 4221]
        self.send_command(0x01).await?;
        self.send_data(&[0xF9, 0x00, 0x00]).await?;

        // Data Entry mode setting: Y increment, X increment [cite: 4653]
        self.send_command(0x11).await?;
        self.send_data(&[0x03]).await?;

        // Set RAM X-address Start/End position [cite: 4966]
        // X ranges from 0 to 15 (16 bytes = 128 bits)
        self.send_command(0x44).await?;
        self.send_data(&[0x00, 0x0F]).await?;

        // Set RAM Y-address Start/End position [cite: 4976]
        // Y ranges from 0 to 249 (0x00 to 0xF9)
        self.send_command(0x45).await?;
        self.send_data(&[0x00, 0x00, 0xF9, 0x00]).await?;

        // Border Waveform Control
        self.send_command(0x3C).await?;
        self.send_data(&[0x05]).await?;

        // Temperature Sensor Selection: Internal [cite: 4659]
        self.send_command(0x18).await?;
        self.send_data(&[0x80]).await?;

        Ok(())
    }

    /// Writes an image buffer to the display and triggers a screen refresh [cite: 4747, 4771]
    pub async fn update(&self, buffer: &[u8; BUFFER_SIZE]) -> Result<()> {
        // Reset address counters to Start
        self.set_ram_address_counters().await?;

        // Write RAM (Black & White) [cite: 4771]
        // 1 = White, 0 = Black [cite: 4776, 4778]
        self.send_command(0x24).await?;
        self.send_data(buffer).await?;

        // Display Update Control 2: Enable Clock, Enable Analog, Display Mode 1, Disable Analog, Disable OSC [cite: 4760, 4763]
        self.send_command(0x22).await?;
        self.send_data(&[0xF7]).await?;

        // Master Activation [cite: 4659]
        self.send_command(0x20).await?;
        self.wait_until_idle().await?;

        Ok(())
    }

    /// Puts the display controller into Deep Sleep mode to save power [cite: 4647, 5175]
    pub async fn deep_sleep(&self) -> Result<()> {
        self.send_command(0x10).await?;
        self.send_data(&[0x01]).await?; // Enter Deep Sleep Mode 1 [cite: 4647]
        Ok(())
    }

    // --- Internal Helpers --- //

    async fn hardware_reset(&self) -> Result<()> {
        self.device.gpio_write("eink_reset", true).await?;
        executor::sleep(Duration::from_millis(10)).await;
        self.device.gpio_write("eink_reset", false).await?;
        executor::sleep(Duration::from_millis(10)).await;
        self.device.gpio_write("eink_reset", true).await?;
        executor::sleep(Duration::from_millis(10)).await;
        Ok(())
    }

    async fn wait_until_idle(&self) -> Result<()> {
        // High = Busy [cite: 4022]
        while self.device.gpio_read("eink_busy").await? {
            executor::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    async fn set_ram_address_counters(&self) -> Result<()> {
        // Set RAM X address counter to 0 [cite: 4976]
        self.send_command(0x4E).await?;
        self.send_data(&[0x00]).await?;

        // Set RAM Y address counter to 0 [cite: 4976]
        self.send_command(0x4F).await?;
        self.send_data(&[0x00, 0x00]).await?;
        
        Ok(())
    }

    async fn send_command(&self, cmd: u8) -> Result<()> {
        self.device.gpio_write("eink_dc", false).await?; // Low for Command [cite: 4018, 4020]
        let mut rx_buf = [0; 1];
        self.device.spi_transfer("eink_spi", &[cmd], &mut rx_buf).await?;
        Ok(())
    }

    async fn send_data(&self, data: &[u8]) -> Result<()> {
        self.device.gpio_write("eink_dc", true).await?; // High for Data [cite: 4018, 4019]
        let mut rx_buf = [0; 1];
        
        // Chunk the transfer if your SPI implementation has strict buffer limits.
        // Assuming your helper can handle standard slices here.
        for chunk in data.chunks(8) {
             let mut chunk_rx = vec![0; chunk.len()];
             self.device.spi_transfer("eink_spi", chunk, &mut chunk_rx).await?;
        }
        Ok(())
    }
}


const HDC2080_ADDR: u8 = 0x40;

// cargo run --bin button_tester -- test-hdc2080
#[derive(Args)]
struct TestHDC2080Command {}

impl TestHDC2080Command {
    async fn run(self) -> Result<()> {
        let device = create_button_device().await?;

        loop {
                // 1. Trigger the measurement
                // Register 0x0F is the Measurement Configuration register.
                // Writing 0x01 sets MEAS_TRIG to 1 (Start measurement).
                let trigger_cmd = [0x0F, 0x01];
                let mut empty_read = [];
                device.i2c_transfer("i2c", HDC2080_ADDR, &trigger_cmd, &mut empty_read).await?;

                // 2. Wait for the conversion to complete
                // 14-bit Temp (610 us) + 14-bit Humidity (660 us) = ~1.3 ms max.
                executor::sleep(Duration::from_millis(2)).await?;

                // 3. Read the 4 bytes of data starting from register 0x00
                let pointer_cmd = [0x00];
                let mut raw_data = [0u8; 4];
                device.i2c_transfer("i2c", HDC2080_ADDR, &pointer_cmd, &mut raw_data).await?;

                // 4. Parse the raw 16-bit integers LSB first
                let temp_raw = (raw_data[1] as u16) << 8 | (raw_data[0] as u16);
                let hum_raw  = (raw_data[3] as u16) << 8 | (raw_data[2] as u16);

                // 5. Scale to f32 using the datasheet formulas
                // Temperature (°C) = (TEMP[15:0] / 2^16) * 165 - 40.5
                let temp_celsius = (temp_raw as f32 / 65536.0) * 165.0 - 40.5;
                
                // Humidity (%RH) = (HUMIDITY[15:0] / 2^16) * 100
                let humidity_rh = (hum_raw as f32 / 65536.0) * 100.0;

                println!(
                    "Raw Temp: {:05} | Scaled Temp: {:.2} °C\nRaw Hum:  {:05} | Scaled Hum:  {:.2} %RH\n",
                    temp_raw, temp_celsius, hum_raw, humidity_rh
                );

                // 6. Sleep for 30 seconds until the next cycle
                executor::sleep(Duration::from_secs(2)).await?;
            }
    }
}


// cargo run --bin button_tester -- test-accel
#[derive(Args)]
struct TestAccelCommand {}

impl TestAccelCommand {
    async fn run(self) -> Result<()> {
        let device = create_button_device().await?;

        let accel = Adxl362::new(device);

        if !accel.verify_connection().await? {
            return Err(err_msg("Bad connection!"));
        }

        accel.configure_motion_interrupt().await?;

        loop {
            println!("{}", accel.device.gpio_read("accel_int").await?);
            executor::sleep(Duration::from_secs(1)).await?;

            // accel.device.poll_gpio_interrupt("accel_int").await?;
            // println!("INT!");
        }

        Ok(())
    }
}

// cargo run --bin button_tester -- test-accel-orientation --output_path=accel.csv
#[derive(Args)]
struct TestAccelOrientationCommand {
    output_path: LocalPathBuf,
}

impl TestAccelOrientationCommand {
    async fn run(self) -> Result<()> {
        let device = create_button_device().await?;

        let accel = Adxl362::new(device);

        if !accel.verify_connection().await? {
            return Err(err_msg("Bad connection!"));
        }

        accel.configure_continuous_read().await?;

        file::write(&self.output_path, "time,x,y,z\n").await?;

        let mut n = 0;
        let mut buf = String::new();
        let mut start = Instant::now();

        loop {

            let t = Instant::now();
            let (x, y, z) = accel.read_xyz().await?;

            let t_elapsed = (t - start).as_secs_f64();
            buf.push_str(&format!("{},{},{},{}\n", t_elapsed, x, y, z));
            n += 1;

            if n % 30 == 0 {
                println!("{}", t_elapsed);
                file::append(&self.output_path, buf.as_bytes()).await?;
                buf.clear();
            }

            executor::sleep(Duration::from_millis(1000 / 40)).await?;
        }

        Ok(())
    }
}

/// A simple driver for the ADXL362 Accelerometer
pub struct Adxl362 {
    device: Arc<PeripheralsDevice>,
}

impl Adxl362 {
    pub fn new(device: Arc<PeripheralsDevice>) -> Self {
        Self { device }
    }

    /// Helper to write a single byte to a register.
    /// The ADXL362 SPI protocol uses 0x0A as the write command.
    async fn write_reg(&self, reg: u8, val: u8) -> Result<()> {
        let mut buf = [0u8; 3];
        // SPI Transfer: [Write Command, Register Address, Data]
        self.device
            .spi_transfer("accel_spi", &[0x0A, reg, val], &mut buf[..])
            .await?;
        Ok(())
    }

    /// Helper to read a single byte from a register.
    /// The ADXL362 SPI protocol uses 0x0B as the read command.
    async fn read_reg(&self, reg: u8) -> Result<u8> {
        let mut buf = [0u8; 3];
        // SPI Transfer: [Read Command, Register Address, Dummy Byte]
        // The dummy byte (0x00) clocks the MISO line so the ADXL362 can send the data back.
        self.device
            .spi_transfer("accel_spi", &[0x0B, reg, 0x00], &mut buf[..])
            .await?;
        
        // buf[0] = during command transfer (ignore)
        // buf[1] = during address transfer (ignore)
        // buf[2] = data returned from the register
        Ok(buf[2])
    }

    /// Verifies the SPI connection by reading the device IDs and comparing them to expected values.
    pub async fn verify_connection(&self) -> Result<bool> {
        let devid_ad = self.read_reg(0x00).await?;
        let devid_mst = self.read_reg(0x01).await?;
        let partid = self.read_reg(0x02).await?;

        // Expected values from the ADXL362 datasheet:
        let is_valid = devid_ad == 0xAD && devid_mst == 0x1D && partid == 0xF2;
        
        Ok(is_valid)
    }

    /// Configures the ADXL362 to act as a low-power motion switch triggering INT1
    pub async fn configure_motion_interrupt(&self) -> Result<()> {
        // Define thresholds and timings as u16 constants (1 LSB = 1 mg)
        const THRESH_ACT: u16 = 125;   // 250 mg activity threshold
        const THRESH_INACT: u16 = 75; // 150 mg inactivity threshold
        const TIME_INACT: u16 = 30;    // 30 samples inactivity timer (~5 seconds at 6Hz)

        // 1. Set Activity Threshold to 250 mg (0xFA). 
        // The default range is ±2g where 1 LSB = 1 mg.
        self.write_reg(0x20, (THRESH_ACT & 0xFF) as u8).await?; // THRESH_ACT_L
        self.write_reg(0x21, (THRESH_ACT >> 8) as u8).await?;   // THRESH_ACT_H

        // 2. Set Inactivity Threshold to 150 mg (0x96).
        self.write_reg(0x23, (THRESH_INACT & 0xFF) as u8).await?; // THRESH_INACT_L
        self.write_reg(0x24, (THRESH_INACT >> 8) as u8).await?;   // THRESH_INACT_H
    
        // 3. Set Inactivity Timer to 30 samples.
        self.write_reg(0x25, (TIME_INACT & 0xFF) as u8).await?; // TIME_INACT_L
        self.write_reg(0x26, (TIME_INACT >> 8) as u8).await?;   // TIME_INACT_H

        // 4. Configure Loop Mode, referenced activity, and referenced inactivity.
        // 0x3F = 0b00111111 -> LINKLOOP=11 (Loop Mode), INACT_REF=1, INACT_EN=1, ACT_REF=1, ACT_EN=1
        self.write_reg(0x27, 0x3F).await?; // ACT_INACT_CTL

        // 5. Map the AWAKE bit to the INT1 pin.
        // 0x40 = Bit 6 (AWAKE status) in the INTMAP1 register.
        self.write_reg(0x2A, 0x40).await?; // INTMAP1

        // 6. Begin measurement in Wake-Up Mode (Lowest power state: 270 nA).
        // 0x0A = 0b00001010 -> WAKEUP (Bit 3) = 1, MEASURE (Bits 1:0) = 10
        self.write_reg(0x2D, 0x0A).await?; // POWER_CTL

        Ok(())
    }

    /// Configures the ADXL362 for continuous XYZ readout at 50 Hz.
    /// This mode CAN be used alongside motion interrupts, but it disables Wake-Up mode
    /// (increasing power draw to ~1.5 µA) to allow for the faster 50 Hz sample rate.
    pub async fn configure_continuous_read(&self) -> Result<()> {
        // 1. Set Output Data Rate (ODR) to 50 Hz.
        // Register 0x2C (FILTER_CTL): Bits [2:0] control ODR. 0x02 = 50 Hz.
        // (Default is 0x03 = 100 Hz).
        self.write_reg(0x2C, 0x02).await?;

        // Note: You can optionally set up the activity/inactivity interrupts here 
        // just like in configure_motion_interrupt(). They work fine in this mode.

        // 2. Begin measurement in standard Measurement Mode (NOT Wake-Up Mode).
        // 0x02 = 0b00000010 -> MEASURE=10, WAKEUP=0
        self.write_reg(0x2D, 0x02).await?; // POWER_CTL

        Ok(())
    }

    /// Reads the current X, Y, and Z acceleration vectors.
    /// Uses an SPI burst read to fetch all 6 bytes in a single transaction.
    /// Returns (X, Y, Z) in raw LSBs (1 LSB = 1 mg if operating in ±2g range).
    pub async fn read_xyz(&self) -> Result<(i16, i16, i16)> {
        // ADXL362 12-bit data registers start at 0x0E (XDATA_L) and go sequentially
        // up to 0x13 (ZDATA_H).
        // To read them all, we send the read command (0x0B), the starting address (0x0E),
        // and then clock out 6 dummy bytes (0x00) to receive the data.
        let tx_buf = [
            0x0B, // SPI Read Command
            0x0E, // Starting Register (XDATA_L)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00 // 6 Dummy bytes to clock MISO
        ];
        
        let mut rx_buf = [0u8; 8];

        self.device
            .spi_transfer("accel_spi", &tx_buf, &mut rx_buf)
            .await?;

        // rx_buf[0] is junk (received during the 0x0B command byte)
        // rx_buf[1] is junk (received during the 0x0E address byte)
        // rx_buf[2..=7] contains our sequential data payload.

        // The ADXL362 outputs data in little-endian format. It is natively a 12-bit 
        // two's complement value, but the chip conveniently sign-extends it into 16-bit 
        // registers so we can parse it directly into standard Rust `i16` types.
        let x = i16::from_le_bytes([rx_buf[2], rx_buf[3]]);
        let y = i16::from_le_bytes([rx_buf[4], rx_buf[5]]);
        let z = i16::from_le_bytes([rx_buf[6], rx_buf[7]]);

        Ok((x, y, z))
    }
}

#[derive(Args)]
struct TestBatteryCommand {
    
}

impl TestBatteryCommand {
    async fn run(self) -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"battery_tester")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        {
            let v1 = device.analog_read("sense").await?;
            println!("{}", v1);
        }

        return Ok(());


        let output_path = project_path!("cr2032_voltage_curve.csv");

        if !file::exists(&output_path).await? {
            file::write(&output_path, b"v1,v2\n").await?;
        }

        device.gpio_write("load", false).await?;

        loop {
            executor::sleep(Duration::from_millis(10)).await?;

            let v1 = device.analog_read("sense").await?;

            device.gpio_write("load", true).await?;

            executor::sleep(Duration::from_millis(2)).await?;

            let v2 = device.analog_read("sense").await?;

            println!("{} : {}", v1, v2);
            file::append(&output_path, format!("{},{}\n", v1, v2).as_bytes()).await?;

            executor::sleep(Duration::from_secs(30)).await?;

            device.gpio_write("load", false).await?;
        }

        Ok(())
    }
}




#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    args.mode.run().await
}