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

/*
pub trait USBDeviceHandler {
    // handle_control() -> Future

    // fn handle_control_read(&self, )

    type ReceiveFuture: Future<()>;

    fn on_receive(data: &[u8]) -> Self::ReceiveFuture;

    type SendFuture: Future<usize>;

    fn on_send(output: &mut [u8]) -> Self::SendFuture;
}

pub trait USBDeviceControlResponder {
    type ControlRespondFuture<'a>: Future<Output = Result<()>> + 'a
    where
        Self: 'a;

    fn control_respond(&mut self, data: &[u8], done: bool) -> Self::ControlRespondFuture;
}
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

    pub async fn run(&mut self) {
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
                        self.handle_setup_packet(pkt).await;
                    }
                }
            }
        }
    }

    async fn wait_for_event(&mut self) -> Event {
        loop {
            if Self::take_event(&mut self.power.events_usbdetected) {
                return Event::PowerDetected;
            }
            if Self::take_event(&mut self.power.events_usbpwrrdy) {
                return Event::PowerReady;
            }
            if Self::take_event(&mut self.power.events_usbremoved) {
                return Event::PowerRemoved;
            }
            if Self::take_event(&mut self.periph.events_usbevent) {
                return Event::USBEvent;
            }
            if Self::take_event(&mut self.periph.events_ep0setup) {
                return Event::EP0Setup;
            }
            if Self::take_event(&mut self.periph.events_usbreset) {
                return Event::USBReset;
            }
            if Self::take_event(&mut self.periph.events_endepin[0]) {
                return Event::EndEpIN0;
            }
            if Self::take_event(&mut self.periph.events_ep0datadone) {
                return Event::EP0DataDone;
            }

            futures::race2(
                wait_for_irq(Interrupt::USBD),
                wait_for_irq(Interrupt::POWER_CLOCK),
            )
            .await;
        }
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

    async fn handle_setup_packet(&mut self, pkt: SetupPacket) {
        // log!(b"==\n");

        if pkt.bRequest == StandardRequestType::SET_ADDRESS as u8 {
            // Don't need to do anything as this is implemented in hardware.
            log!(b"A\n");
            return;
        } else if pkt.bRequest == StandardRequestType::SET_CONFIGURATION as u8 {
            if pkt.bmRequestType != 0b00000000 {
                self.stale();
                return;
            }

            // TODO: upper byte of wValue is reserved.
            // TODO: Value of 0 puts device in address state.

            if pkt.wValue != 1 {
                self.stale();
                return;
            }

            // No data stage

            // Status stage
            // TODO: This is standard from any 'Host -> Device' request
            self.periph.tasks_ep0status.write_trigger();
        } else if pkt.bRequest == StandardRequestType::GET_CONFIGURATION as u8 {
            if pkt.bmRequestType != 0b10000000
                || pkt.wValue != 0
                || pkt.wIndex != 0
                || pkt.wLength != 1
            {
                self.stale();
                return;
            }

            self.control_respond(&pkt, &[1]).await;

            // EP.control_respond(&pkt, (&[1]).iter().cloned()).await;
        } else if pkt.bRequest == StandardRequestType::GET_DESCRIPTOR as u8 {
            if pkt.bmRequestType != 0b10000000 {
                self.stale();
                return;
            }

            let desc_type = (pkt.wValue >> 8) as u8;
            let desc_index = (pkt.wValue & 0xff) as u8; // NOTE: Starts at 0

            if desc_type == DescriptorType::DEVICE as u8 {
                if desc_index != 0 {
                    self.stale();
                    return;
                }
                // TODO: Assert language code.

                log!(b"DD\n");

                self.control_respond(&pkt, DESCRIPTORS.device_bytes()).await;
            } else if desc_type == DescriptorType::CONFIGURATION as u8 {
                // TODO: Validate that the configuration exists.
                // If it doesn't return an error.

                log!(b"DC\n");

                let data = DESCRIPTORS.config_bytes();

                self.control_respond(&pkt, data).await;
            } else if desc_type == DescriptorType::ENDPOINT as u8 {
                self.stale();
            } else if desc_type == DescriptorType::DEVICE_QUALIFIER as u8 {
                // According to the USB 2.0 spec, a full-speed only device should respond to
                // a DEVICE_QUALITY request with an error.
                //
                // TODO: Probably simpler to just us the USB V1 in the device descriptor?
                self.stale();
            } else if desc_type == DescriptorType::STRING as u8 {
                log!(b"DS\n");

                let data = if desc_index == 0 {
                    STRING_DESC0
                } else {
                    STRING_DESC1
                };

                self.control_respond(&pkt, data).await;
            } else {
                self.stale();
            }
        } else {
            self.stale();
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

        // log!(crate::num_to_slice(host_remaining as u32).as_ref());
        // log!(b"\n");

        // TODO: Check host_remaining > 0

        let mut done = false;

        // TODO: Move to the USBDevice instance?
        let mut packet_buffer = [0u8; 64];

        while host_remaining > 0 && !done {
            log!(b">\n");

            let mut packet_len = core::cmp::min(
                core::cmp::min(host_remaining, data.len()),
                packet_buffer.len(),
            );
            let mut packet = &mut packet_buffer[0..packet_len];
            // Maybe copy flash to RAM.
            packet.copy_from_slice(&data[0..packet_len]);
            data = &data[packet_len..];

            host_remaining -= packet_len;

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
                self.periph.epin[0]
                    .ptr
                    .write(unsafe { core::mem::transmute(&packet[0]) });
                self.periph.epin[0].maxcnt.write(packet_len as u32);

                // log!(crate::num_to_slice(self.periph.epin[0].ptr.read() as u32).as_ref());
                // log!(b"\n");

                // Needed to avoid interactions with previous packets and to gurantee that the
                // send ordering is consistent.
                self.periph.events_ep0datadone.write_notgenerated();
                self.periph.events_endepin[0].write_notgenerated();

                // if done || host_remaining == 0 {
                //     self.periph
                //         .shorts
                //         .write_with(|v| v.set_ep0datadone_ep0status_with(|v|
                // v.set_enabled())); } else {
                //     self.periph
                //         .shorts
                //         .write_with(|v| v.set_ep0datadone_ep0status_with(|v|
                // v.set_disabled())); }

                self.periph.tasks_startepin[0].write_trigger();

                // log!(b">2\n");

                // TODO: Record any USBRESET or PowerRemoved events we receive and act on them
                // once the buffer is free'd
                // while self.wait_for_event().await != Event::EndEpIN0 {}

                // TODO: handle USBReset and PowerRemoved
                // loop {
                //     let e =

                // }

                // TODO: Start preparing the next packet while this one is beign sent.
                while self.wait_for_event().await != Event::EP0DataDone {}
            }
        }

        log!(b"-\n");

        // Status stage
        self.periph.tasks_ep0status.write_trigger();
    }

    async fn control_receive(&mut self, pkt: &SetupPacket) {
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
    }
}
