use crate::avr::interrupts::*;
use crate::avr::registers::*;
use crate::usb::SetupPacket;

// From the INTERNET:
// "Thank you for the reply. The USB interface is now receiving the setup
// packets ok! You are right: I must to capture the "end of reset" interrupt
// and, when fired, configure the endpoint 0 to receive the initial setup
// packets." Despite what the datasheet says, the control endpoint is
// deconfigured after a USB bus reset, which occurs when the device is plugged
// in.

/*
Endpoint Interrupt Registers
- UEINTX
    - NAK IN Received Interrupt Flag
    - NAK OUT Received Interrupt Flag

Interrupts enabled by:
- UEIENX
    - Flow error
    - NAK IN
    - NAK OUT
    - RXSTP
    - RXOUT
    - STALLED
    - TXIN
*/
// TODO: It would be nice if we could check for whether or not the total size of
// all endpoint memories are > 832
pub struct USBEndpoint {
    num: u8,
}

// NOTE: We assume that these are the same bits in the UEINTX and UEIENX
// registers.
const RXSTPI_SET: u8 = 1 << 3;
const RXOUTI_SET: u8 = 1 << 2;
const STALLEDI_SET: u8 = 1 << 1;
const TXINI_SET: u8 = 1 << 0;

/*
Control Write (receiving data):
- Get RXOUTI interrupt whenever we have data to receive ()
- Wait for NAKINI


Control Read
- First Unset TXINI after getting setup packet
- Wait for TCINI to go high in order to write data


*/

pub fn init_endpoints() {
    assert!(USB_EP0.configure(
        USBEndpointType::Control,
        USBEndpointDirection::OutOrControl,
        USBEndpointSize::B64,
        USBEndpointBanks::One,
    ));

    assert!(USB_EP1.configure(
        USBEndpointType::Interrupt,
        USBEndpointDirection::In,
        USBEndpointSize::B128,
        USBEndpointBanks::Double,
    ));

    // assert!(USB_EP2.configure(
    //     USBEndpointType::Interrupt,
    //     USBEndpointDirection::OutOrControl,
    //     USBEndpointSize::B128,
    //     USBEndpointBanks::Double,
    // ));
}

fn usb_control_thread() {
    loop {
        // Issues:
        // - If endpoints are reset while waiting for the RXSTPI intterupt or
        //   another, we won't have the interrupt enabled anymore so will never
        //   get ti

        // TODO: On any async delay, we need to remember to re-configure the
        // current endpoint

        // Wait for RXSTPI

        // Read SETUP packet

        // Clear RXSTPI and TXINI

        // Looping over data to send:
        // Wait for TXINI
        // Send response data

        // Wait for RXOUTI
        // Clear to finish?
    }
}

const EORSTE: u8 = 3;
const EORSTI: u8 = 3;

pub async fn wait_usb_end_of_reset() {
    // Enable 'End of Reset' interrupt.
    let ctx = InterruptEnabledContext::new(UDIEN, 1 << EORSTE);

    loop {
        let udint = unsafe { avr_read_volatile(UDINT) };
        if udint & (1 << EORSTI) != 0 {
            // Clear the bit so we don't keep getting the interrupt
            unsafe { avr_write_volatile(UDINT, udint & !(1 << EORSTI)) };
            break;
        }

        InterruptEvent::USBGeneral.to_future().await;
    }

    drop(ctx);
}

const EPEN: u8 = 0;
const CFGOK: u8 = 7;
const ALLOC: u8 = 1;

pub struct EndpointInterruptEnabledContext<'a> {
    ep: &'a USBEndpoint,
    inner: InterruptEnabledContext,
}

impl<'a> EndpointInterruptEnabledContext<'a> {
    pub fn new(ep: &'a USBEndpoint, register: *mut u8, mask: u8) -> Self {
        ep.select();
        let inner = InterruptEnabledContext::new(register, mask);
        Self { ep, inner }
    }
}

impl<'a> Drop for EndpointInterruptEnabledContext<'a> {
    fn drop(&mut self) {
        self.ep.select();
        // self.inner.drop();
    }
}

// TODO: One issue with USB interrupts is that only one interrupt per endpoint
// should be allowed at a time as separate futures will clear the interrupt bits
// of other pending futures.
impl USBEndpoint {
    /// NOTE: Must be called before ANY operation on the endpoint.
    #[inline(always)]
    fn select(&self) {
        unsafe { avr_write_volatile(UENUM, self.num) };
    }

    // TODO: Support reset with UERST?

    // TODO: Must use return value
    pub fn configure(
        &self,
        typ: USBEndpointType,
        dir: USBEndpointDirection,
        size: USBEndpointSize,
        banks: USBEndpointBanks,
    ) -> bool {
        self.select();

        unsafe {
            // Enable endpoint
            avr_write_volatile(UECONX, 1 << EPEN);

            // Configure CONTROL OUT endpoint.
            avr_write_volatile(UECFG0X, typ as u8 | dir as u8);

            // Configure size, banks, and ALLOCate the memory.
            avr_write_volatile(UECFG1X, size as u8 | banks as u8 | 1 << ALLOC);

            // Check CFGOK
            if (avr_read_volatile(UESTA0X) & (1 << CFGOK)) == 0 {
                // USB setup
                crate::USART1::send_blocking(b"USB Endpoint setup failed!\n");
                return false;
            }

            // Default to no interrupts.
            avr_write_volatile(UEIENX, 0);
            // Reset initial state of all interrupt flags.
            avr_write_volatile(UEINTX, 0);
        }

        return true;
    }

    pub fn request_stale(&self) {
        self.select();
        // Keep endpoint enabled and also enable STALLRQ
        unsafe { avr_write_volatile(UECONX, 1 | (1 << 5)) };
    }

    // UEINTX contains whether or not the interrupt has triggered.
    // UEIENX configures if interrupts are enabled

    // NOTE: This must be called after enabling an interrupt.
    async fn wait_for_event(&self) {
        let mask: u8 = 1 << self.num;
        loop {
            let ueint = unsafe { avr_read_volatile(UEINT) };

            // crate::USART1::send_blocking(b"UEINT ");
            // crate::avr::debug::num_to_slice(ueint, |s| {
            //     crate::USART1::send_blocking(s);
            // });
            // crate::USART1::send_blocking(b"\n");

            if ueint & mask != 0 {
                break;
            }

            // TODO: Consider keeping the waker in all queues until the future is dropped so
            // that we can optimize running the same future in a loop.
            InterruptEvent::USBEndpoint.to_future().await;
        }
    }

    fn check_flag(&self, mask: u8) -> bool {
        self.select();
        (unsafe { avr_read_volatile(UEINTX) }) & mask != 0
    }

    fn clear_flag(&self, mask: u8) {
        self.select();
        unsafe { avr_write_volatile(UEINTX, avr_read_volatile(UEINTX) & (!mask)) };
    }

    #[inline(never)]
    async fn wait_flag(&self, mask: u8) {
        // self.select();
        // TODO: When dropping, this must select the right endpoint.
        let ctx = EndpointInterruptEnabledContext::new(self, UEIENX, mask);

        loop {
            // NOTE: The await from the last iteration may have switched the endpoint
            // so we must ensure that the correct one is selected.
            // self.select();

            if self.check_flag(mask) {
                break;
            }

            // Enable interrupt for this flag (and disable others)
            // unsafe { avr_write_volatile(UEIENX, mask) };

            // Wait for next interesting event.
            // TODO: Disable the interrupt if this is dropped (but we must select the
            // thread)?
            self.wait_for_event().await;

            // Disable all interrupts on this endpoint.
            // self.select();
            // unsafe { avr_write_volatile(UEIENX, 0) };
        }

        // drop(ctx);
    }

    pub fn bytec(&self) -> u16 {
        self.select();

        let low = unsafe { avr_read_volatile(UEBCLX) } as u16;
        let high = unsafe { avr_read_volatile(UEBCHX) } as u16;
        (high << 8) | low
    }

    /// NOTE: This does not protect from overflowing the FIFO.
    pub fn read_bytes(&self, buf: &mut [u8]) {
        self.select();
        for i in 0..buf.len() {
            buf[i] = unsafe { avr_read_volatile(UEDATX) };
        }
    }

    pub fn write_bytes(&self, buf: &[u8]) {
        self.select();
        for i in 0..buf.len() {
            unsafe { avr_write_volatile(UEDATX, buf[i]) };
        }
    }

    /// Call after you are done reading or writing to the current FIFO bank.
    /// This will allow the controller to send/receive from/to it and switch to
    /// a different bank if available.
    pub fn release_bank(&self) {
        // Clear FIFOCON
        self.clear_flag(1 << 7);
    }

    // TODO: Handle FNCERR

    pub fn received_setup(&self) -> bool {
        self.check_flag(RXSTPI_SET)
    }
    pub fn clear_setup(&self) {
        self.clear_flag(RXSTPI_SET);
    }
    pub async fn wait_setup(&self) {
        self.wait_flag(RXSTPI_SET).await;
    }

    pub fn received_data(&self) -> bool {
        self.check_flag(RXOUTI_SET)
    }
    pub fn clear_received_data(&self) {
        self.clear_flag(RXOUTI_SET);
    }
    pub async fn wait_received_data(&self) {
        self.wait_flag(RXOUTI_SET).await;
    }

    pub fn transmitter_ready(&self) -> bool {
        self.check_flag(TXINI_SET)
    }
    // TODO: Consider automatically clearing the flags once an interrupt is
    // received?
    pub fn clear_transmitter_ready(&self) {
        self.clear_flag(TXINI_SET)
    }
    pub async fn wait_transmitter_ready(&self) {
        self.wait_flag(TXINI_SET).await
    }

    /// Responses to a control read request from the host with some data.
    /// NOTE: Only valid if called on the first endpoint.
    pub async fn control_respond<T: core::iter::Iterator<Item = u8>>(
        &self,
        pkt: &SetupPacket,
        mut data: T,
    ) {
        // Remaining number of bytes the host will accept.
        let mut host_remaining = pkt.wLength;

        // TODO: Check host_remaining > 0

        loop {
            self.wait_transmitter_ready().await;

            let mut done = false;
            let mut packet_bytes = USB_EP0.bytec();
            while packet_bytes > 0 && host_remaining > 0 {
                if let Some(byte) = data.next() {
                    // TODO: Write one byte.
                } else {
                    // In this case, we will end up sending the current packet as either incomplete
                    // or as a ZLP.
                    done = true;
                    break;
                }

                packet_bytes -= 1;
                host_remaining -= 1;
            }

            // Send the packet.
            self.clear_transmitter_ready();

            if done {
                break;
            }
        }

        // Status stage
        self.wait_received_data().await;
        self.clear_received_data();
    }
}

/// UECFG0X::EPDIR
pub enum USBEndpointDirection {
    In = 1,
    OutOrControl = 0,
}

/// UECFG0X::EPTYPE
pub enum USBEndpointType {
    Control = 0b00 << 6,
    Bulk = 0b10 << 6,
    Isochronous = 0b01 << 6,
    Interrupt = 0b11 << 6,
}

/// UECFG1X::EPBK
pub enum USBEndpointBanks {
    One = 0b00 << 2,
    Double = 0b01 << 2,
}

/// UECFG1X::EPSIZE
pub enum USBEndpointSize {
    B8 = 0b000 << 4,
    B16 = 0b001 << 4,
    B32 = 0b010 << 4,
    B64 = 0b011 << 4,
    B128 = 0b100 << 4,
    B256 = 0b101 << 4,
    B512 = 0b110 << 4,
}

pub const USB_EP0: USBEndpoint = USBEndpoint { num: 0 };
pub const USB_EP1: USBEndpoint = USBEndpoint { num: 1 };
pub const USB_EP2: USBEndpoint = USBEndpoint { num: 2 };
pub const USB_EP3: USBEndpoint = USBEndpoint { num: 3 };
pub const USB_EP4: USBEndpoint = USBEndpoint { num: 4 };
pub const USB_EP5: USBEndpoint = USBEndpoint { num: 5 };
pub const USB_EP6: USBEndpoint = USBEndpoint { num: 6 };
