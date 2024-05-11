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

use core::arch::asm;

use common::register::{RegisterRead, RegisterWrite};
use common::struct_bytes::struct_bytes;
use executor::futures;
use executor::interrupts::wait_for_irq;
use peripherals::raw::usbd::epdatastatus::EPDATASTATUS_VALUE;
use peripherals::raw::usbd::epinen::EPINEN_VALUE;
use peripherals::raw::usbd::epouten::EPOUTEN_VALUE;
use peripherals::raw::usbd::size::epout::EPOUT_VALUE;
use peripherals::raw::EventState;
use peripherals::raw::Interrupt;
use usb::descriptors::*;

use crate::usb::aligned::Aligned;
use crate::usb::handler::{USBDeviceHandler, USBError};
use crate::usb::send_buffer::USBDeviceSendBuffer;

// TODO: Implement more errata like:
// https://infocenter.nordicsemi.com/topic/errata_nRF52840_Rev3/ERR/nRF52840/Rev3/latest/anomaly_840_199.html

pub const MAX_PACKET_SIZE: usize = 64;

pub struct USBDeviceController {
    periph: peripherals::raw::usbd::USBD,
    power: peripherals::raw::power::POWER,
    state: State,

    /// Direction and index of the endpoint which currently has an EasyDMA
    /// transfer running.
    ///
    /// NOTE: There can only be single EasyDMA transfer active on the USBD
    /// peripheral at a time, so we never have to deal with this tracking more
    /// than one transfer.
    pending_transfer: Option<(EndpointDirection, usize)>,

    send_buffer: Option<&'static USBDeviceSendBuffer>,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    /// Initial state. Waiting for USB power to be detected.
    Disconnected,

    Starting,

    PendingReset,

    Active,
}

#[derive(PartialEq, Clone, Copy)]
enum Event {
    PowerDetected = 1 << 0,
    PowerReady = 1 << 1,
    PowerRemoved = 1 << 2,

    USBEvent = 1 << 3,
    EP0Setup = 1 << 4,
    USBReset = 1 << 5,
    EndEpIN = 1 << 6,
    EndEpOUT = 1 << 7,
    EP0DataDone = 1 << 8,
    EPData = 1 << 9,

    /// Only emitted in USBDeviceController::run() when the
    SendBufferReadable = 1 << 10,
}

impl USBDeviceController {
    pub fn new(
        mut periph: peripherals::raw::usbd::USBD,
        mut power: peripherals::raw::power::POWER,
    ) -> Self {
        // NOTE: We assume that initially all the corresponding EVENT registers are not
        // set.

        // TODO: Clear these interrupts on Drop.

        // TODO: Clear EPDATASTATUS by writting all oens.

        periph.intenset.write_with(|v| {
            v.set_usbreset()
                .set_ep0setup()
                .set_usbevent()
                .set_endepin0()
                .set_endepin1()
                .set_endepin2()
                .set_endepin3()
                .set_endepin4()
                .set_endepin5()
                .set_endepin6()
                .set_endepin7()
                .set_endepout0()
                .set_endepout1()
                .set_endepout2()
                .set_endepout3()
                .set_endepout4()
                .set_endepout5()
                .set_endepout6()
                .set_endepout7()
                .set_ep0datadone()
                .set_epdata()
        });

        power
            .intenset
            .write_with(|v| v.set_usbdetected().set_usbpwrrdy().set_usbremoved());

        Self {
            periph,
            power,
            state: State::Disconnected,
            pending_transfer: None,
            send_buffer: None,
        }
    }

    pub fn set_send_buffer(&mut self, buffer: &'static USBDeviceSendBuffer) {
        self.send_buffer = Some(buffer);
    }

    pub async fn run<H: USBDeviceHandler>(&mut self, mut handler: H) {
        loop {
            // If we are in an active state, we may be able to send packets back to the
            // host.
            let send_buffer_future = {
                futures::optional(self.send_buffer.and_then(|buf| {
                    if self.state != State::Active {
                        return None;
                    }

                    Some(futures::map(buf.wait_until_readable(), |_| {
                        Event::SendBufferReadable
                    }))
                }))
            };

            let event = race!(self.wait_for_event(), send_buffer_future).await;

            // self.send_buffer.map(|buf| buf.)

            // futures::map(, mapper)

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
                        self.state = State::Disconnected;
                        self.periph.enable.write_disabled();
                        continue;
                    }

                    if let Event::USBReset = event {
                        self.configure_endpoints();
                        self.state = State::Active;
                    }
                }
                State::Active => {
                    if self.power.usbregstatus.read().vbusdetect().is_novbus() {
                        self.state = State::Disconnected;
                        self.periph.enable.write_disabled();
                        continue;
                    }

                    // TODO: Are we able to get a setup packet while a previous setup packet is
                    // being processed?

                    if let Event::EP0Setup = event {
                        // TODO: Improve the error handling by enqueuing pending events in the outer
                        // loop.
                        loop {
                            let pkt = self.get_setup_packet();
                            match self.handle_setup_packet(pkt, &mut handler).await {
                                Ok(()) => {}
                                Err(e) => {
                                    if e == USBError::Reset {
                                        log!("RESET");
                                        self.configure_endpoints();
                                    } else if e == USBError::NewSetupPacket {
                                        log!("RE-SETUP");
                                        continue;
                                    }
                                }
                            }

                            break;
                        }
                    } else if let Event::USBReset = event {
                        log!("RESET");
                        self.configure_endpoints();
                    } else if let Event::EPData = event {
                        let status = self.periph.epdatastatus.read();

                        // Clear by writing all 1's
                        self.periph
                            .epdatastatus
                            .write(EPDATASTATUS_VALUE::from_raw(0xffffffff));

                        let mut endpoint_index = None;
                        let mut input_done = false;

                        // TODO: What if while we are processing this, we get another EPData event
                        // (need to )

                        // TODO: It is possible that multiple could have data available, so we need
                        // to handle all of them (including input completions)
                        if status.epout1().is_started() {
                            endpoint_index = Some(1);
                        } else if status.epout2().is_started() {
                            endpoint_index = Some(2);
                        } else if status.epout3().is_started() {
                            endpoint_index = Some(3);
                        } else if status.epout4().is_started() {
                            endpoint_index = Some(4);
                        } else if status.epout5().is_started() {
                            endpoint_index = Some(5);
                        } else if status.epout6().is_started() {
                            endpoint_index = Some(6);
                        } else if status.epout7().is_started() {
                            endpoint_index = Some(7);
                        } else if status.epin1().is_datadone() {
                            input_done = true;
                        } else if status.epin2().is_datadone() {
                            input_done = true;
                        }
                        // TODO: Add other ones.

                        if input_done {
                            log!("IDONE");
                        }

                        /*

                        The NRF52 USBD peripheral has its own internal buffer for Bulk/Interrupt transfers.

                        - When we write data using STARTEPIN/ENDEPIN, the next IN packet after the ENDEPIN will receive that data and will be acknowledged.
                            - We get a notification of when the ACK actually occurs using the EPDATA event

                        - When the host writes things with OUT, and the internal buffer is empty (meaning that SIZE.EPOUT was edited)

                        NOTE: This means that all the DMA transfers are fast and not blocking for a long time.


                        */

                        if let Some(endpoint_index) = endpoint_index {
                            let mut request = USBDeviceNormalRequest {
                                controller: self,
                                endpoint_index,
                            };

                            let mut buf = [0u8; 64];
                            let nread = match request.read(&mut buf).await {
                                Ok(v) => v,
                                Err(e) => {
                                    log!("ERR");
                                    continue;
                                }
                            };

                            /*
                            let original = buf[0];
                            buf[0] = b'X';

                            log!("EPD: ", nread as u32);

                            for i in 0..1000 {
                                log!("...............................................................................");
                            }

                            buf[0] = original;
                            */

                            // TODO: There is a race condition here:
                            // - If the host sends an IN token, it must receive data from the last
                            //   time .write() was called rather than the data we are about to write
                            //   here.

                            let mut request2 = USBDeviceNormalResponse {
                                controller: self,
                                endpoint_index: 1,
                            };
                            request2.write(&buf[0..nread]).await;

                            log!("DONE!");
                        }
                    } else if let Event::SendBufferReadable = event {
                        let send_buffer = self.send_buffer.as_ref().unwrap();
                        if let Some(data) = send_buffer.try_read().await {
                            let mut res = USBDeviceNormalResponse {
                                controller: self,
                                endpoint_index: 1,
                            };

                            match res.write(&data).await {
                                Ok(()) => {}
                                Err(e) => {
                                    if e == USBError::Reset {
                                        log!("RESET");
                                        self.configure_endpoints();
                                    } else if e == USBError::NewSetupPacket {
                                        // TODO: Must re-enqueue this event in a bit map so that we
                                        // know to process it again.
                                        log!("TODO RE-SETUP");
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn wait_for_event(&mut self) -> Event {
        loop {
            if let Some(event) = self.pending_event() {
                return event;
            }

            race!(
                wait_for_irq(Interrupt::USBD),
                wait_for_irq(Interrupt::POWER_CLOCK),
            )
            .await;
        }
    }

    async fn wait_for_specific_event(
        &mut self,
        event: Event,
        defer_error: bool,
    ) -> Result<(), USBError> {
        let mut result = Ok(());
        loop {
            match self.wait_for_event().await {
                Event::PowerRemoved => {
                    result = Err(USBError::Disconnected);
                }
                Event::USBReset => {
                    result = Err(USBError::Reset);
                }
                Event::EP0Setup => {
                    result = Err(USBError::NewSetupPacket);
                }
                e => {
                    if e == event {
                        return result;
                    } else {
                        // TODO: THere may be events such as EPData which we may
                        // want to handle later.

                        // TODO:
                        // log!("EX", e as u32);
                    }
                }
            }

            if !defer_error && !result.is_ok() {
                return result;
            }
        }
    }

    fn pending_event(&mut self) -> Option<Event> {
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
        if Self::take_event(&mut self.periph.events_ep0datadone) {
            return Some(Event::EP0DataDone);
        }

        if let Some((dir, index)) = self.pending_transfer.clone() {
            match dir {
                EndpointDirection::In => {
                    if Self::take_event(&mut self.periph.events_endepin[index]) {
                        self.pending_transfer = None;
                        return Some(Event::EndEpIN);
                    }
                }
                EndpointDirection::Out => {
                    if Self::take_event(&mut self.periph.events_endepout[index]) {
                        self.pending_transfer = None;
                        return Some(Event::EndEpOUT);
                    }
                }
            }
        }

        // MUST be checked after the EndEP events are checked as we react to those
        // events first in the code.
        if Self::take_event(&mut self.periph.events_epdata) {
            return Some(Event::EPData);
        }

        crate::events::flush_events_clear();

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
        let mut epinen = EPINEN_VALUE::from_raw(0);
        let mut epouten = EPOUTEN_VALUE::from_raw(0);

        // Control endpoint.
        epinen.set_in0_with(|v| v.set_enable());
        epouten.set_out0_with(|v| v.set_enable());

        // TODO: Make this more configurable.
        epouten.set_out2_with(|v| v.set_enable());
        epinen.set_in1_with(|v| v.set_enable());

        self.periph.epinen.write(epinen);
        self.periph.epouten.write(epouten);

        // Write anything to SIZE.EPOUT[i]. This will ensure that the USB controller
        // knows that it is allowed to send us more EPDATA events.
        //
        // We need to do this because of the line in the product specification that says
        // "A NAK is returned until the software writes any value to register
        // SIZE.EPOUT[n], indicating that the content of the local buffer can be
        // overwritten.".
        //
        // NOTE: We also do this after every OUT bulk/interrupt transfer is done.
        for reg in &mut self.periph.size.epout {
            reg.write(EPOUT_VALUE::from_raw(0));
        }
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
    ) -> Result<(), USBError> {
        // log!("==");

        if pkt.bmRequestType & (1 << 7) != 0 {
            // Device -> Host
            let res = USBDeviceControlResponse {
                controller: self,
                host_remaining: (pkt.wLength as usize),
            };
            handler.handle_control_response(pkt, res).await
        } else {
            // Host -> Device
            let req = USBDeviceControlRequest {
                controller: self,
                host_remaining: (pkt.wLength as usize),
            };
            handler.handle_control_request(pkt, req).await
        }
    }

    fn stale(&mut self) {
        self.periph.tasks_ep0stall.write_trigger();
    }

    /*
    TODO: Bulk/interrupt transactions must be up to 64 bytes
    - Also 32-bit aligned and a multiple of 4 bytes
    */
}

pub struct USBDeviceControlRequest<'a> {
    controller: &'a mut USBDeviceController,
    host_remaining: usize,
}

impl<'a> USBDeviceControlRequest<'a> {
    /// TODO: This must support partially reading.
    /// TODO: Verify that the host doesn't send more than host_remaining.
    ///
    /// Notes:
    /// - EPOUT[0].AMOUNT seems to be useless.
    /// - STARTEPOUT[0] seems to be useless.
    /// - TASKS_EP0RCVOUT appears to be required BEFORE any DMA transfers will
    ///   occur.
    pub async fn read(&mut self, mut output: &mut [u8]) -> Result<usize, USBError> {
        let mut total_read = 0;

        // TODO: Re-use a more global buffer.
        let mut packet_buffer = [0u8; 64];

        self.controller.periph.epout[0]
            .ptr
            .write(unsafe { core::mem::transmute(packet_buffer.as_ptr()) });
        self.controller.periph.epout[0]
            .maxcnt
            .write(packet_buffer.len() as u32);

        while self.host_remaining > 0 {
            self.controller.periph.tasks_ep0rcvout.write_trigger();
            self.controller
                .wait_for_specific_event(Event::EP0DataDone, false)
                .await?;

            // XXX: Critical DMA section
            self.controller.pending_transfer = Some((EndpointDirection::Out, 0));
            self.controller.periph.tasks_startepout[0].write_trigger();
            self.controller
                .wait_for_specific_event(Event::EndEpOUT, true)
                .await?;

            let packet_len = self.controller.periph.epout[0].amount.read() as usize;
            // let packet_len = self.controller.periph.size.epout[0].read().size() as usize;
            if packet_len > output.len() {
                // Overflow. Panic!
            }

            output[0..packet_len].copy_from_slice(&packet_buffer[0..packet_len]);
            output = &mut output[packet_len..];
            total_read += packet_len;
            self.host_remaining -= packet_len;

            if packet_len < packet_buffer.len() {
                break;
            }
        }

        self.controller.periph.tasks_ep0status.write_trigger();

        Ok(total_read)
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
    pub async fn write(&mut self, mut data: &[u8]) -> Result<(), USBError> {
        // log!(">");

        let mut done = false;

        // TODO: Move to the USBDeviceController instance?
        let mut packet_buffer = [0u8; MAX_PACKET_SIZE];

        while self.host_remaining > 0 && !done {
            let mut packet_len = core::cmp::min(
                core::cmp::min(self.host_remaining, data.len()),
                packet_buffer.len(),
            );
            let mut packet = &mut packet_buffer[0..packet_len];
            // Maybe copy flash to RAM (if already in ram, no copying should be needed.)
            packet.copy_from_slice(&data[0..packet_len]);
            data = &data[packet_len..];

            self.host_remaining -= packet_len;

            if packet_len < MAX_PACKET_SIZE {
                // In this case, we will end up sending the current packet as either incomplete
                // or as a ZLP.
                done = true;
            }

            // Send the packet.
            {
                // TODO: Berify that this is 32-bit aligned and always a
                self.controller.periph.epin[0]
                    .ptr
                    .write(unsafe { core::mem::transmute(packet.as_ptr()) });
                self.controller.periph.epin[0]
                    .maxcnt
                    .write(packet_len as u32);

                // log!(crate::log::num_to_slice(self.periph.epin[0].ptr.read() as
                // u32).as_ref()); log!(b"\n");

                // Needed to avoid interactions with previous packets and to gurantee that the
                // send ordering is consistent.
                self.controller
                    .periph
                    .events_ep0datadone
                    .write_notgenerated();
                self.controller.periph.events_endepin[0].write_notgenerated();

                // NOTE: The clearing of the events on the previous lines may take up to 4
                // cycles to take effect. This means that if TASKS_STARTEPIN finishes too
                // quickly (e.g. with a zero length payload), the end events won't actually be
                // generated and we'll be stuck.
                unsafe {
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                }

                self.controller.pending_transfer = Some((EndpointDirection::In, 0));
                self.controller.periph.tasks_startepin[0].write_trigger();

                // TODO: handle USBReset and PowerRemoved
                // loop {
                //     let e =

                // }

                // while self.controller.wait_for_event().await != Event::EndEpIN {}

                // TODO: Must not return any errors until we get to the EndEpIN0

                // self.controller
                //     .wait_for_specific_event(Event::EndEpIN, true)
                //     .await?;

                // We MUST always wait for EndEpIN0 to happen first to ensure that the DMA
                // transfer is done. Then we should wait for EP0DataDone but we
                // may exist early on a reset/disconnect event.
                {
                    let mut result = Ok(());
                    let mut dma_done = false;

                    loop {
                        match self.controller.wait_for_event().await {
                            Event::EP0DataDone => break,
                            Event::PowerRemoved => {
                                result = Err(USBError::Disconnected);
                                if dma_done {
                                    break;
                                }
                            }
                            Event::USBReset => {
                                result = Err(USBError::Reset);
                                if dma_done {
                                    break;
                                }
                            }
                            Event::EP0Setup => {
                                result = Err(USBError::NewSetupPacket);
                                if dma_done {
                                    break;
                                }
                            }
                            // TODO: Must not return errors until the DMA is done.
                            Event::EndEpIN => {
                                dma_done = true;
                                if !result.is_ok() {
                                    break;
                                }
                            }
                            e => {
                                log!("E", e as u32);
                            }
                        }
                    }

                    result?;
                }

                // TODO: Start preparing the next packet while this one is beign
                // sent. self.controller
                //     .wait_for_specific_event(Event::EP0DataDone, false)
                //     .await?;
            }
        }

        unsafe {
            asm!("nop");
            asm!("nop");
            asm!("nop");
            asm!("nop");
        }

        // log!("<");

        // Status stage
        self.controller.periph.tasks_ep0status.write_trigger();

        Ok(())
    }

    pub fn stale(mut self) {
        self.controller.stale();
    }
}

/*
TODO: THere are some undocumented registers for aborting a transfer:
- https://github.com/NordicSemiconductor/nrfx/blob/master/drivers/src/nrfx_usbd.c#L774
*/

pub struct USBDeviceNormalRequest<'a> {
    controller: &'a mut USBDeviceController,
    endpoint_index: usize,
}

impl<'a> USBDeviceNormalRequest<'a> {
    // TODO: ONly allow calling this once.
    pub async fn read(&mut self, mut output: &mut [u8]) -> Result<usize, USBError> {
        // TODO: Re-use a global buffer
        let mut packet_buffer = Aligned::<_, u32>::new([0u8; MAX_PACKET_SIZE]);

        self.controller.periph.epout[self.endpoint_index]
            .ptr
            .write(unsafe { core::mem::transmute(packet_buffer.as_ptr()) });
        self.controller.periph.epout[self.endpoint_index]
            .maxcnt
            .write(packet_buffer.len() as u32);

        self.controller.pending_transfer = Some((EndpointDirection::Out, self.endpoint_index));
        self.controller.periph.tasks_startepout[self.endpoint_index].write_trigger();
        self.controller
            .wait_for_specific_event(Event::EndEpOUT, true)
            .await?;

        // NOTE: 'epout.amount' seems to always contain 64 (buffer size) while
        // SIZE.EPOUT seems to have the current value.
        //
        // let packet_len = self.controller.periph.epout[self.endpoint_index]
        //     .amount
        //     .read() as usize;
        let packet_len = self.controller.periph.size.epout[self.endpoint_index]
            .read()
            .size() as usize;

        // Clear it to allow receiving additional requests.
        self.controller.periph.size.epout[self.endpoint_index].write_with(|v| v.set_size(0));

        if packet_len > output.len() {
            // Overflow. Panic!
        }

        output[0..packet_len].copy_from_slice(&packet_buffer[0..packet_len]);
        Ok(packet_len)
    }
}

pub struct USBDeviceNormalResponse<'a> {
    controller: &'a mut USBDeviceController,
    endpoint_index: usize,
}

impl<'a> USBDeviceNormalResponse<'a> {
    /// Sends
    pub async fn write(&mut self, data: &[u8]) -> Result<(), USBError> {
        // TODO: Re-use a global buffer
        let mut packet_buffer = Aligned::<_, u32>::new([0u8; MAX_PACKET_SIZE]);

        packet_buffer[0..data.len()].copy_from_slice(data);

        self.controller.periph.epin[self.endpoint_index]
            .ptr
            .write(unsafe { core::mem::transmute(packet_buffer.as_ptr()) });
        self.controller.periph.epin[self.endpoint_index]
            .maxcnt
            .write(data.len() as u32);

        self.controller.pending_transfer = Some((EndpointDirection::In, self.endpoint_index));
        self.controller.periph.tasks_startepin[self.endpoint_index].write_trigger();

        self.controller
            .wait_for_specific_event(Event::EndEpIN, true)
            .await?;

        /*
        self.controller
            .wait_for_specific_event(Event::EPData, false)
            .await?;

        // TODO: Read the EPDATASTATUS and verify it is actually setting the correct
        // value.

        // Clear by writing all 1's
        self.controller
            .periph
            .epdatastatus
            .write(EPDATASTATUS_VALUE::from_raw(0xffffffff));
        */

        /*

        */

        // TODO: Wait for EPDATA and acknowledge the bit.

        /*
        (EPIN[1].PTR=0xnnnnnnnn
        EPIN[1].MAXCNT = <MaxPacketSize
        STARTEPIN[1]=1

        Invoke STARTEPIN[1]

        Will get events:
        - STARTED
        - ENDEPIN[1]
        - EPDATA

        */

        Ok(())
    }
}
