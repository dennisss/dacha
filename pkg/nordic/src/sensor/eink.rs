use nordic_proto::nordic::EinkConfig;
use peripherals::raw::p0::{P0, P0_REGISTERS};
use peripherals::raw::p1::P1;
use peripherals::raw::spim0::SPIM0;

use crate::spi::{SPIHost, SPIMode};
use crate::timer::*;
use crate::gpio::*;
use crate::pins::*;
use crate::rtc::RTC;

// TODO: Dedup with the button_tester binary.
// TODO: Need limits on the max amount of time this will run for.


const WIDTH: u32 = 122;
const HEIGHT: u32 = 250;
const BUFFER_SIZE: usize = ((WIDTH + 7) / 8 * HEIGHT) as usize; // 16 * 250 = 4000 bytes


pub struct EinkDriver {
    timer: RTC,
    reset_pin: GPIOPin,
    dc_pin: GPIOPin,
    on_pin: GPIOPin,
    busy_pin: GPIOPin,
    spi: SPIHost,
}

impl Drop for EinkDriver {
    fn drop(&mut self) {
        self.reset_pin.reset();
        self.dc_pin.reset();
        self.busy_pin.reset();
        self.on_pin.reset();
        self.spi.disable();
    }

}

impl EinkDriver {
    pub fn new(config: &EinkConfig, timer: RTC) -> Self {
        let mut gpio = GPIO::new(
            unsafe { P0::new() },
            unsafe { P1::new() }
        );


        let mut reset_pin = gpio.pin(IndexedPin::new(config.reset_pin()));
        reset_pin.reset()
            .write(PinLevel::Low)
            .set_direction(PinDirection::Output);

        let mut dc_pin = gpio.pin(IndexedPin::new(config.dc_pin()));
        dc_pin.reset()
            .write(PinLevel::Low)
            .set_direction(PinDirection::Output);

        let mut on_pin = gpio.pin(IndexedPin::new(config.on_pin()));
        on_pin.reset()
            .write(PinLevel::Low)
            .set_direction(PinDirection::Output);

        let mut busy_pin = gpio.pin(IndexedPin::new(config.busy_pin()));
        busy_pin.reset()
            .set_direction(PinDirection::Input);

        let spi = SPIHost::new::<_, DisconnectedPin, _>(
            unsafe { SPIM0::new().into() },
            1_000_000,
            Some(IndexedPin::new(config.mosi_pin())),
            None,
            Some(IndexedPin::new(config.sclk_pin())),
            Some(gpio.pin(IndexedPin::new(config.cs_pin()))),
            SPIMode::Mode0
        );

        Self {
            timer,
            reset_pin,
            dc_pin,
            on_pin,
            busy_pin,
            spi,
        }
    }

    /// Powers on and initializes the display from deep sleep or a cold boot [cite: 5150]
    pub async fn init(&mut self) {
        self.on_pin.write(PinLevel::High);
        self.timer.wait_ms(1000).await;

        log!("Z1");
        self.hardware_reset().await;
        log!("Z2");

        // SW Reset [cite: 5157]
        self.send_command(0x12).await;
        self.timer.wait_ms(10).await;
        self.wait_until_idle().await;

        // Driver Output control: 250 rows (250 - 1 = 249 = 0xF9) [cite: 4220, 4221]
        self.send_command(0x01).await;
        self.send_data(&[0xF9, 0x00, 0x00]).await;

        // Data Entry mode setting: Y increment, X increment [cite: 4653]
        self.send_command(0x11).await;
        self.send_data(&[0x03]).await;

        // Set RAM X-address Start/End position [cite: 4966]
        // X ranges from 0 to 15 (16 bytes = 128 bits)
        self.send_command(0x44).await;
        self.send_data(&[0x00, 0x0F]).await;

        // Set RAM Y-address Start/End position [cite: 4976]
        // Y ranges from 0 to 249 (0x00 to 0xF9)
        self.send_command(0x45).await;
        self.send_data(&[0x00, 0x00, 0xF9, 0x00]).await;

        // Border Waveform Control
        self.send_command(0x3C).await;
        self.send_data(&[0x05]).await;

        // Temperature Sensor Selection: Internal [cite: 4659]
        self.send_command(0x18).await;
        self.send_data(&[0x80]).await;

        log!("HERE!!");

        self.wait_until_idle().await;

    }

    /// Writes an image buffer to the display and triggers a screen refresh [cite: 4747, 4771]
    pub async fn update(&mut self, buffer: &[u8; BUFFER_SIZE]) {
        log!("X1");

        // Reset address counters to Start
        self.set_ram_address_counters().await;

        log!("X2");

        // Write RAM (Black & White) [cite: 4771]
        // 1 = White, 0 = Black [cite: 4776, 4778]
        self.send_command(0x24).await;

        log!("X3");
        self.send_data(buffer).await;
        log!("X4");

        // Display Update Control 2: Enable Clock, Enable Analog, Display Mode 1, Disable Analog, Disable OSC [cite: 4760, 4763]
        self.send_command(0x22).await;
        log!("X5");
        self.send_data(&[0xF7]).await;
        log!("X6");

        // Master Activation [cite: 4659]

        self.send_command(0x20).await;
        log!("X7");
        self.wait_until_idle().await;
    }

    /// Puts the display controller into Deep Sleep mode to save power [cite: 4647, 5175]
    pub async fn deep_sleep(&mut self) {
        self.send_command(0x10).await;
        self.send_data(&[0x01]).await; // Enter Deep Sleep Mode 1 [cite: 4647]

        self.timer.wait_ms(10).await;
        self.on_pin.write(PinLevel::Low);
    }

    // --- Internal Helpers --- //

    async fn hardware_reset(&mut self) {
        self.reset_pin.write(PinLevel::High);
        self.timer.wait_ms(10).await;
        self.reset_pin.write(PinLevel::Low);
        self.timer.wait_ms(10).await;
        self.reset_pin.write(PinLevel::High);
        self.timer.wait_ms(10).await;

        self.wait_until_idle().await;
    }

    async fn wait_until_idle(&mut self) {
        // High = Busy [cite: 4022]
        while self.busy_pin.read() == PinLevel::High {
            self.timer.wait_ms(10).await;
        }
    }

    async fn set_ram_address_counters(&mut self) {
        // Set RAM X address counter to 0 [cite: 4976]
        self.send_command(0x4E).await;
        self.send_data(&[0x00]).await;

        // Set RAM Y address counter to 0 [cite: 4976]
        self.send_command(0x4F).await;
        self.send_data(&[0x00, 0x00]).await;
    }

    async fn send_command(&mut self, cmd: u8) {
        self.dc_pin.write(PinLevel::Low); // Low for Command [cite: 4018, 4020]
        self.timer.wait_ms(4).await;
        
        let mut rx_buf = [0; 1];

        self.spi.transfer(&[cmd], &mut rx_buf).await;
        self.timer.wait_ms(4).await;
    }

    async fn send_data(&mut self, data: &[u8]) {
        self.dc_pin.write(PinLevel::High); // High for Data [cite: 4018, 4019]
        self.timer.wait_ms(4).await;

        let mut rx_buf = [0; 8];
        
        // Chunk the transfer if your SPI implementation has strict buffer limits.
        // Assuming your helper can handle standard slices here.
        for chunk in data.chunks(8) {
             self.spi.transfer(chunk, &mut rx_buf[0..chunk.len()]).await;
             self.timer.wait_ms(4).await;
        }
    }
}
