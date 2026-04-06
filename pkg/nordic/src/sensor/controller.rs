
use common::errors::*;
use executor::sync::{AsyncMutex, AsyncMutexGuard, AsyncMutexReadOnlyGuard};
use nordic_proto::nordic::*;
use peripherals::raw::{PinDirection, PinLevel};
use nordic_wire::packet::PacketBuffer;
use protobuf::{Message, StaticMessage};
use peripherals::raw::spim0::SPIM0;
use peripherals::raw::twim0::TWIM0;

use crate::pins::IndexedPin;
use crate::gpio::*;
use crate::gpiote::*;
use crate::radio_socket::RadioController;
use crate::rtc::RTC;
use crate::rng::*;
use crate::twim::TWIM;
use crate::sensor::eink::*;

const HDC2080_ADDR: u8 = 0x40;


pub struct SensorController {
    config: SensorConfig,
    rtc: RTC,
    state: AsyncMutex<State>
}

struct State {
    radio_controller: RadioController,
    prng: Xoshiro128PlusPlus,
    gpio: GPIO,
    state_snapshot: SensorPacket,
}

impl SensorController {
    pub fn new(
        config: SensorConfig,
        rtc: RTC,
        radio_controller: RadioController,
        prng: Xoshiro128PlusPlus,
        gpio: GPIO
    ) -> Self {
        Self {
            config,
            rtc,
            state: AsyncMutex::new(State {
                radio_controller,
                prng,
                gpio,
                state_snapshot: SensorPacket::default()
            })
        }
    }

    // TODO: NEed to support re-starting the controller if the config changes.
    #[inline(never)]
    pub fn start(&'static self) {
        SensorHeartbeatThread::start(self);
        SensorDriverThread::start(self);
    }

    async fn run_driver(&self) {
        if self.config.has_edge_trigger() {
            self.run_edge_trigger(self.config.edge_trigger()).await;
        }

        if self.config.has_hdc2080() {
            self.run_hdc2080(self.config.hdc2080()).await;
        }

        if self.config.has_eink() {
            self.run_eink(self.config.eink()).await;
        }
    }

    async fn run_edge_trigger(&self, config: &EdgeTriggerSensorConfig) {
        let mut pin = lock_async!(state <= self.state.lock().await.unwrap(), {
            state.gpio.pin(IndexedPin::new(config.pin()))
        });

        let mut port_waiter = unsafe { GPIOPortWaiter::new() };
        let mut timer = self.rtc.clone();

        {
            // TODO: Wrap into a single instruction
            pin
            .reset()
            .set_direction(PinDirection::Input);
            if config.pull_up() {
                pin.set_resistor(Resistor::PullUp);
            }

            // Wait for stabilization after pin setup.
            timer.wait_ms(1).await;
        }

        let mut current_level = pin.read();

        // Set initial state.
        lock_async!(state <= self.state.lock().await.unwrap(), {
            state.state_snapshot.edge_trigger_mut().set_level(level_to_bool(current_level));
        });

    
        loop {
            let mut next_level = invert_pin_level(current_level);

            {
                // TODO: Wrap into a single instruction
                pin
                    .reset()
                    .set_direction(PinDirection::Input)
                    .set_sense(Some(next_level));
                if config.pull_up() {
                    pin.set_resistor(Resistor::PullUp);
                }

                // Wait for stabilization after pin setup.
                timer.wait_ms(1).await;
            }

            let debounce_ms = match next_level {
                PinLevel::High => config.debounce_rising_ms(),
                PinLevel::Low => config.debounce_falling_ms(),
                PinLevel::Unknown(_) => panic!()
            };

            loop {
                let _ = port_waiter.pending_event();

                // Wait for level change
                port_waiter.wait().await;

                // Debounce period.
                timer.wait_ms(debounce_ms).await;

                // Verify we are still at the next level.
                if pin.read() == next_level {
                    break;
                }
            }

            // Stop for now (reduces power consumption)
            pin.reset();

            current_level = next_level;
            lock_async!(state <= self.state.lock().await.unwrap(), {
                state.state_snapshot.edge_trigger_mut().set_level(level_to_bool(current_level));
            });

            let triggered = match next_level {
                PinLevel::High => config.rising_trigger(),
                PinLevel::Low => config.falling_trigger(),
                PinLevel::Unknown(_) => panic!()
            };

            if triggered {
                let mut pkt = nordic_proto::nordic::SensorPacket::default();
                pkt.edge_trigger_mut().set_triggered(true);
                pkt.edge_trigger_mut().set_level(level_to_bool(current_level));

                lock_async!(state <= self.state.lock().await.unwrap(), {
                    self.send_radio_packet(&pkt, &mut *state).await;
                });

                if config.trigger_cooldown_ms() != 0 {
                    timer.wait_ms(config.trigger_cooldown_ms()).await;
                }
            }
        }
    }

    async fn run_hdc2080(&self, config: &HDC2080SensorConfig) {
        let mut timer = self.rtc.clone();

        // Make sure the sensor finishes initial startup.
        timer.wait_ms(10).await;

        loop {
            crate::clock::reference_hfclk();
            self.run_hdc2080_single_sample(config).await;
            crate::clock::unreference_hfclk();

            timer.wait_ms(config.sample_period_ms()).await;
        }

    }

    async fn run_hdc2080_single_sample(&self, config: &HDC2080SensorConfig) -> Result<()> {

        let twim_config = TWIM::configure(100_000usize).unwrap();
        let twim0 = unsafe { TWIM0::new() };

        let mut inst = TWIM::new(
            twim0,
            IndexedPin::new(config.scl_pin()),
            IndexedPin::new(config.sda_pin()),
            twim_config,
        );

        // 1. Trigger the measurement
        // Register 0x0F is the Measurement Configuration register.
        // Writing 0x01 sets MEAS_TRIG to 1 (Start measurement).
        let trigger_cmd = [0x0F, 0x01];
        inst.write(HDC2080_ADDR, &trigger_cmd).await?;

        // 2. Wait for the conversion to complete
        // 14-bit Temp (610 us) + 14-bit Humidity (660 us) = ~1.3 ms max.
        self.rtc.clone().wait_ms(2).await;

        // 3. Read the 4 bytes of data starting from register 0x00
        let pointer_cmd = [0x00];
        let mut raw_data = [0u8; 4];
        inst.write_then_read(HDC2080_ADDR, Some(&pointer_cmd), Some(&mut raw_data)).await?;

        // 4. Parse the raw 16-bit integers LSB first
        let temp_raw = (raw_data[1] as u16) << 8 | (raw_data[0] as u16);
        let hum_raw  = (raw_data[3] as u16) << 8 | (raw_data[2] as u16);

        let mut pkt = SensorPacket::default();
        pkt.hdc2080_mut().set_temperature_raw(temp_raw as u32);
        pkt.hdc2080_mut().set_humidity_raw(hum_raw as u32);

        lock_async!(state <= self.state.lock().await.unwrap(), {
            self.send_radio_packet(&pkt, &mut *state).await;
        });

        Ok(())
    }

    async fn run_eink(&self, config: &EinkConfig) {
        let mut timer = self.rtc.clone();

        timer.wait_ms(1000).await;

        loop {
            let mut image = nordic_bitmaps::DisplayBuffer::new();
            image.draw_text("Hello\nWorld!");

            crate::clock::reference_hfclk();

            let mut driver = EinkDriver::new(config, timer.clone());
            driver.init().await;
            driver.update(&image.buffer).await;

            driver.deep_sleep().await;
            drop(driver);

            crate::clock::unreference_hfclk();

            timer.wait_ms(5000).await;
        }


    }

    async fn send_radio_packet(&self, proto: &SensorPacket, state: &mut State) {
        let mut data = common::fixed::vec::FixedVec::<u8, 256>::new();
        let _ = proto.serialize_to(&protobuf::SerializeOptions::default(), &mut data);

        // Clear prior received packets.
        let mut rx_packet = PacketBuffer::new();
        while state.radio_controller.socket().dequeue_rx(&mut rx_packet).await {}

        crate::clock::reference_hfclk();

        // Max 3 attempts (~300ms)
        const MAX_ATTEMPTS: usize = 3;
        for i in 0..MAX_ATTEMPTS {
            // TODO: Always re-use the same packet data to avoid counter incrementing and re-encryption time (and to make things more idempotent.)
            // (though this will require allowing the receiver to re-ack the same packet counter)
            self.send_radio_packet_once(data.as_ref(), state).await;

            let mut timer1 = self.rtc.clone();
            let timeout = async {
                timer1.wait_ms(2).await;
                false
            };

            let rx = async {
                state.radio_controller.receive_once().await;
                // RADIO_SOCKET.wait_for_rx().await;
                true
            };

            let got_ack = race!(timeout, rx).await;

            if got_ack {
                break;
            }

            if i + 1 < MAX_ATTEMPTS {
                let wait_time = state.prng.range(1000, 5000); // 1ms - 5ms
                self.rtc.clone().wait_micros(wait_time).await;
            }
        }

        crate::clock::unreference_hfclk();
    }

    async fn send_radio_packet_once(&self, data: &[u8], state: &mut State) {
        let mut packet = PacketBuffer::new();
        packet.set_counter(0); // Set in the RADIO_SOCKET
        packet.resize_data(data.len());
        packet.data_mut().copy_from_slice(data);

        // Send to the first link if configured
        {
            let config_guard = state.radio_controller.socket().lock_network_config().await;
            let config = match config_guard.get() {
                Some(v) => v,
                None => return,
            };

            let link = match config.links().get(0) {
                Some(l) => l,
                None => return,
            };
            packet.remote_address_mut().copy_from_slice(link.address());
        }

        let _ = state.radio_controller.socket().enqueue_tx(&mut packet).await;
        state.radio_controller.transmit_packet().await;
    }
}



fn invert_pin_level(level: PinLevel) -> PinLevel {
    match level {
        PinLevel::High => PinLevel::Low,
        PinLevel::Low => PinLevel::High,
        PinLevel::Unknown(_) => panic!()
    }
}

fn level_to_bool(level: PinLevel) -> bool {
    match level {
        PinLevel::High => true,
        PinLevel::Low => false, 
        PinLevel::Unknown(_) => panic!()
    }
}



define_thread!(
    SensorHeartbeatThread,
    sensor_heartbeat_thread_fn,
    controller: &'static SensorController
);
async fn sensor_heartbeat_thread_fn(controller: &'static SensorController) {
    if controller.config.heartbeat_ms() == 0 {
        return;
    }

    let mut timer = controller.rtc.clone();

    loop {
        timer.wait_ms(controller.config.heartbeat_ms()).await;

        lock_async!(state <= controller.state.lock().await.unwrap(), {

            let mut pkt = state.state_snapshot.clone();
            // TODO: Collect battery votlage.

            controller.send_radio_packet(&pkt, &mut *state).await;
        });
    }
}

define_thread!(
    SensorDriverThread,
    sensor_driver_thread_fn,
    controller: &'static SensorController
);
async fn sensor_driver_thread_fn(controller: &'static SensorController) {
    controller.run_driver().await
}
