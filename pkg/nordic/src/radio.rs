use executor::interrupts::wait_for_irq;
use peripherals::raw::{Interrupt, RegisterRead, RegisterWrite};

use crate::log;

/// NOTE: This requires that the HFXO is started already before using.
///
/// While send() or receive() isn't being actively called, the radio is held in
/// a DISABLED state.
///
/// TODO: We should support keeping the radio in TXIDLE or
/// RXIDLE if we anticipate doing many TX/RX operations in a row.
pub struct Radio {
    periph: peripherals::raw::radio::RADIO,
}

impl Radio {
    pub fn new(mut periph: peripherals::raw::radio::RADIO) -> Self {
        // NOTE: The POWER register is 1 at boot so we shouldn't need to turn on the
        // peripheral.

        periph.frequency.write_with(|v| v.set_frequency(5)); // Exactly (2400 + 5) MHz
        periph.txpower.write_0dbm(); // TODO: +8 dBm (max power)
        periph.mode.write_nrf_2mbit();

        // 1 LENGTH byte (8 bits). 0 S0, S1 bits. 8-bit preamble.
        periph
            .pcnf0
            .write_with(|v| v.set_lflen(8).set_plen_with(|v| v.set_8bit()));

        // MAXLEN=255. STATLEN=0, BALEN=3 (so we have 4 byte addresses), little endian
        periph.pcnf1.write_with(|v| v.set_maxlen(255).set_balen(3));

        periph.base0.write(0xAABBCCDD);
        periph.prefix0.write_with(|v| v.set_ap0(0xEE));

        periph.txaddress.write(0); // Transmit on address 0

        // Receive from address 0
        periph
            .rxaddresses
            .write_with(|v| v.set_addr0_with(|v| v.set_enabled()));

        // Copies the 802.15.4 mode.
        periph.crccnf.write_with(|v| {
            v.set_len_with(|v| v.set_two())
                .set_skipaddr_with(|v| v.set_ieee802154())
        });
        periph.crcpoly.write(0x11021);
        periph.crcinit.write(0);

        // TODO: Verify the radio is currently disabled and all events are not
        // generated.
        periph.intenset.write_with(|v| v.set_end().set_disabled());

        // TASKS_RXEN and TASKS_TXEN will trigger TASKS_START.
        periph
            .shorts
            .write_with(|v| v.set_ready_start_with(|v| v.set_enabled()));

        Self { periph }
    }

    /// Blocks until a packet is received. Returns the number of bytes received.
    ///
    /// TODO: Figure out if I should use CRCSTATUS.
    pub async fn receive(&mut self, out: &mut [u8]) -> usize {
        let mut packet = [0u8; 256];

        self.periph
            .packetptr
            .write(unsafe { core::mem::transmute(&packet) });

        loop {
            let mut guard = RadioStateGuard::new(&mut self.periph);
            guard.periph.tasks_rxen.write_trigger();
            guard.wait_for_end().await;
            guard.disable().await;

            if !self.periph.crcstatus.read().is_crcok() {
                log!(b"!\n");
                continue;
            }

            log!(b"RX ");
            log!(crate::num_to_slice(packet[0] as u32).as_ref());
            log!(b"\n");

            if packet[0] <= 64 {
                break;
            }
        }

        let len = packet[0] as usize;

        out[0..len].copy_from_slice(&packet[1..(1 + len)]);
        len
    }

    pub async fn send(&mut self, message: &[u8]) {
        // TODO: Just have a global buffer given that only one that can be copied at a
        // time anyway.
        let mut packet = [0u8; 256];
        packet[0] = message.len() as u8;
        packet[1..(1 + message.len())].copy_from_slice(message);

        self.periph
            .packetptr
            .write(unsafe { core::mem::transmute(&packet) });

        let mut guard = RadioStateGuard::new(&mut self.periph);
        guard.periph.tasks_txen.write_trigger();
        guard.wait_for_end().await;
        guard.disable().await;
    }
}

/// Scope for using the radio in states other than DISABLED.
///
/// On drop this object will block until the radio is DISABLED. This ensures
/// that EasyDMA isn't accessing memory that may soon be dropped. It also
/// ensures that the next call to Radio::send() or Radio::recieve() starts in a
/// well defined state.
///
/// Based on the datasheet @ NRF_2Mbit, it should take up to 6us to disable the
/// ratio.
///
/// Users should prefer to call RadioStateGuard::disable() to ensure that we
/// don't busy loop for a long time on disabling the radio.
///
/// TODO: Ideally replace the drop() mechanism with some form of async
/// cancellation mechanism.
struct RadioStateGuard<'a> {
    periph: &'a mut peripherals::raw::radio::RADIO,
}

impl<'a> Drop for RadioStateGuard<'a> {
    fn drop(&mut self) {
        if self.periph.state.read().is_disabled() {
            return;
        }

        self.periph.tasks_disable.write_trigger();
        while !self.periph.state.read().is_disabled() {}

        self.clear_all_events();
    }
}

impl<'a> RadioStateGuard<'a> {
    fn new(periph: &'a mut peripherals::raw::radio::RADIO) -> Self {
        assert!(periph.state.read().is_disabled());

        Self { periph }
    }

    async fn wait_for_end(&mut self) {
        while self.periph.events_end.read().is_notgenerated() {
            wait_for_irq(Interrupt::RADIO).await;
        }
        self.periph.events_end.write_notgenerated();
    }

    async fn disable(mut self) {
        if self.periph.state.read().is_disabled() {
            return;
        }

        self.periph.tasks_disable.write_trigger();
        while !self.periph.state.read().is_disabled() {
            wait_for_irq(Interrupt::RADIO).await;
            self.clear_all_events();
        }
    }

    /// Clears all events which we are using for interrupts.
    fn clear_all_events(&mut self) {
        self.periph.events_end.write_notgenerated();
        self.periph.events_disabled.write_notgenerated();
    }
}
