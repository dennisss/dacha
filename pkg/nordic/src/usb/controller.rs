/*
12 Mbps

Two control (1 IN, 1 OUT)
14 bulk/interrupt (7 IN, 7 OUT)
Two isochronous (1 IN, 1 OUT)

64 bytes buffer size for each bulk/interrupt endpoint


USBDETECTED and USBREMOVED


Start up (based on figure 3 on the USBD section):
1. Wait for USBDETECRED
2. Set ENABLE
3. Make sure HFCLK is starting
4. Will get a USBEVENT CAUSE=READY
5. Wait for USBPWRRDY event
6. Assuming the HFCLK is also on, we can set USBPULLUP=ENABLED

    Upon detecting VBUS removal, it is recommended to wait for ongoing EasyDMA transfers to finish before disabling USBD (relevant ENDEPIN[n], ENDISOIN, ENDEPOUT[n], or ENDISOOUT events, see EasyDMA). The USBREMOVED event, described in USB supply, signals when the VBUS is removed. Reading the ENABLE register will return Enabled until USBD is completely disabled.


7. Wait for USBRESET (also wait for this loner term)
    - All endpoints are disabled and USBADDR is reset to 0 on reset.


8. Configure endpoitns:
    EPINEN and EPOUTEN to enable them

9. Listen for EP0SETUP

    - Don't need to configure the address ourselves (just need to update our state machine)

    Trigger STARTEPIN[i] to trigger data sending.
    Later trigger EP0STATUS to enter the status stage.

"After the device has connected to the USB bus (i.e. after VBUS is applied), the device shall not respond to any traffic from the time the pull-up is enabled until it has seen a USB reset condition. This is automatically ensured by the USBD."

*/

use common::struct_bytes::struct_bytes;
use executor::futures;
use executor::interrupts::wait_for_irq;
use peripherals::raw::EventState;
use peripherals::raw::Interrupt;
use peripherals::raw::RegisterRead;
use peripherals::raw::RegisterWrite;
use usb::descriptors::*;

use crate::log;
use crate::usb::descriptors::*;
use crate::usb::handler::USBDeviceHandler;

/*
API for implementing custom handlers:
- For control packets:
    - If bmRequestType::recipient is not the device, we will forward it to the handler
    - We know based on the bmRequestType direction whether we will be sending or receiving data.
    - Receiving Data:
        // Should return whether or not we are ok to proceed (false will stale the request)
        - start_control_recieve(pkt: SetupPacket) -> bool;
        // Called after 'start_control_recieve' with each packet that occurs
        - perform_control_recieve(data: &[u8], done; bool);
        //
        - end_control_receive(complete: bool)

        - control_receive(pkt: SetupPacket) -> Option<&mut >

- For bulk transfers
    -

API for working with interrupt requests:
- Two operations (both async)
    - receive(data: &[u8]) : Called when we get an interrupt OUT
    - send(out: &mut [u8]) :

USBReceiver
*/

/// TODO: Rename to not
pub struct USBDeviceController {
    periph: peripherals::raw::usbd::USBD,
    power: peripherals::raw::power::POWER,
    state: State,
}

#[derive(Clone, Copy)]
enum State {
    /// Initial state. Waiting for USB power to be detected.
    Disconnected,

    Starting,

    PendingReset,

    Active,
}

#[derive(PartialEq, Clone, Copy)]
enum Event {
    PowerDetected,
    PowerReady,
    PowerRemoved,

    USBEvent,
    EP0Setup,
    USBReset,
    EndEpIN0,
    EndEpOUT0,
    EP0DataDone,
}

impl USBDeviceController {
    pub fn new(
        mut periph: peripherals::raw::usbd::USBD,
        mut power: peripherals::raw::power::POWER,
    ) -> Self {
        // NOTE: We assume that initially all the corresponding EVENT registers are not
        // set.

        // TODO: Clear these interrupts on Drop.

        periph.intenset.write_with(|v| {
            v.set_usbreset()
                .set_ep0setup()
                .set_usbevent()
                .set_endepin0()
                .set_endepout0()
                .set_ep0datadone()
        });

        power
            .intenset
            .write_with(|v| v.set_usbdetected().set_usbpwrrdy().set_usbremoved());

        Self {
            periph,
            power,
            state: State::Disconnected,
        }
    }

    pub async fn run<H: USBDeviceHandler>(&mut self, mut handler: H) {
        loop {
            let event = self.wait_for_event().await;

            /*
            // In all cases, if we detect USBREMOVED, power off the device.
            // TODO: Also reset all events in this case and disable HFCLK?
            //
            // TODO: If there are active transfers, wait for them to finish.
            if self.power.usbregstatus.read().vbusdetect().is_novbus() {
                self.state = State::Disconnected;
                self.periph.enable.write_disabled();
                continue;
            }
            */

            match self.state {
                State::Disconnected => {
                    // Step 1: Enable USB peripheral on power USBDETECTED event.
                    // TODO: At this point also start up the HFCLK is it is not already starting.
                    if let Event::PowerDetected = event {
                        log!(b"1\n");

                        // Errata #187: Part 1
                        // https://infocenter.nordicsemi.com/topic/errata_nRF52840_Rev3/ERR/nRF52840/Rev3/latest/anomaly_840_187.html
                        unsafe {
                            core::ptr::write_volatile(0x4006EC00 as *mut u32, 0x00009375);
                            core::ptr::write_volatile(0x4006ED14 as *mut u32, 0x00000003);
                            core::ptr::write_volatile(0x4006EC00 as *mut u32, 0x00009375);
                        }

                        self.periph.enable.write_enabled();
                        self.state = State::Starting;
                    }
                }
                State::Starting => {
                    if self.power.usbregstatus.read().vbusdetect().is_novbus() {
                        log!(b"R\n");
                        self.state = State::Disconnected;
                        self.periph.enable.write_disabled();
                        continue;
                    }

                    // Step 2: Once all of:
                    // 1. HFCLK is running
                    // 2. USBPWRREADY is received
                    // 3. USBEVENT is recieved with EVENTCAUSE=READY
                    //
                    // we can enable the pull up.
                    if self.power.usbregstatus.read().outputrdy().is_ready()
                        && self.periph.eventcause.read().ready().is_ready()
                    {
                        log!(b"2\n");

                        self.periph
                            .usbpullup
                            .write_with(|v| v.set_connect_with(|v| v.set_enabled()));

                        // Clear by writing 1's.
                        self.periph
                            .eventcause
                            .write_with(|v| v.set_ready_with(|v| v.set_ready()));

                        // Errata #187: Part 2
                        // https://infocenter.nordicsemi.com/topic/errata_nRF52840_Rev3/ERR/nRF52840/Rev3/latest/anomaly_840_187.html
                        unsafe {
                            core::ptr::write_volatile(0x4006EC00 as *mut u32, 0x00009375);
                            core::ptr::write_volatile(0x4006ED14 as *mut u32, 0x00000000);
                            core::ptr::write_volatile(0x4006EC00 as *mut u32, 0x00009375);
                        }

                        self.state = State::PendingReset;
                    }
                }
                State::PendingReset => {
                    if self.power.usbregstatus.read().vbusdetect().is_novbus() {
                        log!(b"R\n");
                        self.state = State::Disconnected;
                        self.periph.enable.write_disabled();
                        continue;
                    }

                    if let Event::USBReset = event {
                        log!(b"3\n");

                        self.configure_endpoints();
                        self.state = State::Active;
                    }
                }
                State::Active => {
                    if self.power.usbregstatus.read().vbusdetect().is_novbus() {
                        log!(b"R\n");
                        self.state = State::Disconnected;
                        self.periph.enable.write_disabled();
                        continue;
                    }

                    if let Event::EP0Setup = event {
                        // log!(b"4\n");

                        let pkt = self.get_setup_packet();
                        self.handle_setup_packet(pkt, &mut handler).await;
                    }
                }
            }
        }
    }

    async fn wait_for_event(&mut self) -> Event {
        loop {
            if let Some(event) = self.pending_event().await {
                return event;
            }

            futures::race2(
                wait_for_irq(Interrupt::USBD),
                wait_for_irq(Interrupt::POWER_CLOCK),
            )
            .await;
        }
    }

    async fn pending_event(&mut self) -> Option<Event> {
        if Self::take_event(&mut self.power.events_usbdetected) {
            return Some(Event::PowerDetected);
        }
        if Self::take_event(&mut self.power.events_usbpwrrdy) {
            return Some(Event::PowerReady);
        }
        if Self::take_event(&mut self.power.events_usbremoved) {
            return Some(Event::PowerRemoved);
        }
        if Self::take_event(&mut self.periph.events_usbevent) {
            return Some(Event::USBEvent);
        }
        if Self::take_event(&mut self.periph.events_ep0setup) {
            return Some(Event::EP0Setup);
        }
        if Self::take_event(&mut self.periph.events_usbreset) {
            return Some(Event::USBReset);
        }
        if Self::take_event(&mut self.periph.events_endepin[0]) {
            return Some(Event::EndEpIN0);
        }
        if Self::take_event(&mut self.periph.events_endepout[0]) {
            return Some(Event::EndEpOUT0);
        }
        if Self::take_event(&mut self.periph.events_ep0datadone) {
            return Some(Event::EP0DataDone);
        }

        None
    }

    fn take_event<R: RegisterRead<Value = EventState> + RegisterWrite<Value = EventState>>(
        register: &mut R,
    ) -> bool {
        let v = register.read() == EventState::Generated;
        register.write(EventState::NotGenerated);
        v
    }

    fn configure_endpoints(&mut self) {
        self.periph
            .epinen
            .write_with(|v| v.set_in0_with(|v| v.set_enable()));
        self.periph
            .epouten
            .write_with(|v| v.set_out0_with(|v| v.set_enable()));
    }

    fn get_setup_packet(&self) -> SetupPacket {
        SetupPacket {
            bmRequestType: self.periph.bmrequesttype.read().to_raw() as u8,
            bRequest: self.periph.brequest.read().to_value() as u8,
            wValue: (self.periph.wvaluel.read() as u16)
                | ((self.periph.wvalueh.read() as u16) << 8),
            wIndex: (self.periph.windexl.read() as u16)
                | ((self.periph.windexh.read() as u16) << 8),
            wLength: (self.periph.wlengthl.read() as u16)
                | ((self.periph.wlengthh.read() as u16) << 8),
        }
    }

    async fn handle_setup_packet<H: USBDeviceHandler>(
        &mut self,
        pkt: SetupPacket,
        handler: &mut H,
    ) {
        // log!(b"==\n");

        if pkt.bmRequestType & (1 << 7) != 0 {
            // Device -> Host
            let res = USBDeviceControlResponse {
                controller: self,
                host_remaining: (pkt.wLength as usize),
            };
            handler.handle_control_response(pkt, res).await;
        } else {
            // Host -> Device
            let req = USBDeviceControlRequest {
                controller: self,
                host_remaining: (pkt.wLength as usize),
            };
            handler.handle_control_request(pkt, req).await;
        }
    }

    fn stale(&mut self) {
        self.periph.tasks_ep0stall.write_trigger();
    }

    /*
    TODO: Bulk/interrupt transactions must be up to 64 bytes
    - Also 32-bit aligned and a multiple of 4 bytes
    */

    async fn control_respond(&mut self, pkt: &SetupPacket, mut data: &[u8]) {
        // TODO: Assert that the top bit of kPacketType is set.

        // Remaining number of bytes the host will accept.
        let mut host_remaining = pkt.wLength as usize;

        let mut res = USBDeviceControlResponse {
            controller: self,
            host_remaining,
        };
        res.write(data).await;
    }
}

pub struct USBDeviceControlRequest<'a> {
    controller: &'a mut USBDeviceController,
    host_remaining: usize,
}

impl<'a> USBDeviceControlRequest<'a> {
    // TODO: This must support partially reading.
    // TODO: Verify that the host doesn't send more than host_remaining.
    pub async fn read(&mut self, mut output: &mut [u8]) -> usize {
        /*
        If no data, just call EP0STATUS immediately after receiving the SETUP packet.

        Otherwise,
        - For just the first packet:
            1. Configure the EasyDMA buffer
            2. Trigger EP0RCVOUT to allow acknowleding
            3. Wait for EP0DATADONE
            - At this point, process data for first packet
        - For future packets:
            1. Configure the EasyDMA buffer
            2. Trigger STARTEPOUT to allow receiving into the buffer.
            3. Wait for ENDEPOUT[0]
            - At this point process data in N'th data packet.
            4. Trigger EP0RCVOUT to allow ACK'ing it
            5. Wait for EP0DATADONE
            6. Either go back to #1 or trigger EP0STATUS
        */

        let mut total_read = 0;

        if self.host_remaining == 0 {
            self.controller.periph.tasks_ep0status.write_trigger();
            return 0;
        }

        let mut packet_buffer = [0u8; 64];

        self.controller.periph.epout[0]
            .ptr
            .write(unsafe { core::mem::transmute(packet_buffer.as_ptr()) });
        self.controller.periph.epout[0]
            .maxcnt
            .write(packet_buffer.len() as u32);

        self.controller.periph.tasks_ep0rcvout.write_trigger();

        while self.controller.wait_for_event().await != Event::EP0DataDone {}

        let packet_len = self.controller.periph.epout[0].amount.read() as usize;
        if packet_len > output.len() {
            // Overflow.
        }

        output[0..packet_len].copy_from_slice(&packet_buffer[0..packet_len]);
        output = &mut output[packet_len..];
        total_read += packet_len;
        self.host_remaining -= packet_len;

        if packet_len < packet_buffer.len() || self.host_remaining == 0 {
            self.controller.periph.tasks_ep0status.write_trigger();
            return total_read;
        }

        loop {
            self.controller.periph.tasks_startepout[0].write_trigger();

            while self.controller.wait_for_event().await != Event::EndEpOUT0 {}
            self.controller.periph.tasks_ep0rcvout.write_trigger();

            let packet_len = self.controller.periph.epout[0].amount.read() as usize;
            if packet_len > output.len() {
                // Overflow. Panic!
            }

            output[0..packet_len].copy_from_slice(&packet_buffer[0..packet_len]);
            output = &mut output[packet_len..];
            total_read += packet_len;
            self.host_remaining -= packet_len;

            while self.controller.wait_for_event().await != Event::EP0DataDone {}

            if packet_len < packet_buffer.len() || self.host_remaining == 0 {
                break;
            }
        }

        self.controller.periph.tasks_ep0status.write_trigger();

        total_read
    }

    pub fn stale(mut self) {
        self.controller.stale();
    }
}

pub struct USBDeviceControlResponse<'a> {
    controller: &'a mut USBDeviceController,
    host_remaining: usize,
}

impl<'a> USBDeviceControlResponse<'a> {
    // TODO: This must support partially writing.
    pub async fn write(&mut self, mut data: &[u8]) {
        // log!(crate::num_to_slice(host_remaining as u32).as_ref());
        // log!(b"\n");

        let mut done = false;

        // TODO: Move to the USBDeviceController instance?
        let mut packet_buffer = [0u8; 64];

        while self.host_remaining > 0 && !done {
            log!(b">\n");

            let mut packet_len = core::cmp::min(
                core::cmp::min(self.host_remaining, data.len()),
                packet_buffer.len(),
            );
            let mut packet = &mut packet_buffer[0..packet_len];
            // Maybe copy flash to RAM.
            packet.copy_from_slice(&data[0..packet_len]);
            data = &data[packet_len..];

            self.host_remaining -= packet_len;

            // log!(crate::num_to_slice(packet_len as u32).as_ref());
            // log!(b"\n");

            if packet_len < 64 {
                // In this case, we will end up sending the current packet as either incomplete
                // or as a ZLP.
                done = true;
            }

            // log!(b">1\n");

            // Send the packet.
            {
                // TODO: Berify that this is 32-bit aligned and always a
                self.controller.periph.epin[0]
                    .ptr
                    .write(unsafe { core::mem::transmute(packet.as_ptr()) });
                self.controller.periph.epin[0]
                    .maxcnt
                    .write(packet_len as u32);

                // log!(crate::num_to_slice(self.periph.epin[0].ptr.read() as u32).as_ref());
                // log!(b"\n");

                // Needed to avoid interactions with previous packets and to gurantee that the
                // send ordering is consistent.
                self.controller
                    .periph
                    .events_ep0datadone
                    .write_notgenerated();
                self.controller.periph.events_endepin[0].write_notgenerated();

                // if done || host_remaining == 0 {
                //     self.periph
                //         .shorts
                //         .write_with(|v| v.set_ep0datadone_ep0status_with(|v|
                // v.set_enabled())); } else {
                //     self.periph
                //         .shorts
                //         .write_with(|v| v.set_ep0datadone_ep0status_with(|v|
                // v.set_disabled())); }

                self.controller.periph.tasks_startepin[0].write_trigger();

                // log!(b">2\n");

                // TODO: Record any USBRESET or PowerRemoved events we receive and act on them
                // once the buffer is free'd
                // while self.wait_for_event().await != Event::EndEpIN0 {}

                // TODO: handle USBReset and PowerRemoved
                // loop {
                //     let e =

                // }

                // TODO: Start preparing the next packet while this one is beign sent.
                while self.controller.wait_for_event().await != Event::EP0DataDone {}
            }
        }

        log!(b"-\n");

        // Status stage
        self.controller.periph.tasks_ep0status.write_trigger();
    }

    pub fn stale(mut self) {
        self.controller.stale();
    }
}
