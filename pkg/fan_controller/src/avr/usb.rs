use crate::avr::interrupts::*;
use crate::avr::registers::*;
use crate::avr_assert;
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
    num_mask: u8,
}

// NOTE: We assume that these are the same bits in the UEINTX and UEIENX
// registers.
const RXSTPI_MASK: u8 = 1 << 3;
const RXOUTI_MASK: u8 = 1 << 2;
const STALLEDI_MASK: u8 = 1 << 1;
const TXINI_MASK: u8 = 1 << 0;

/*
Control Write (receiving data):
- Get RXOUTI interrupt whenever we have data to receive ()
- Wait for NAKINI


Control Read
- First Unset TXINI after getting setup packet
- Wait for TCINI to go high in order to write data


*/

pub fn init_endpoints() {
    avr_assert!(USB_EP0.configure(
        USBEndpointType::Control,
        USBEndpointDirection::OutOrControl,
        USBEndpointSize::B64,
        USBEndpointBanks::One,
    ));

    avr_assert!(USB_EP1.configure(
        USBEndpointType::Interrupt,
        USBEndpointDirection::In,
        USBEndpointSize::B64,
        USBEndpointBanks::Double,
    ));

    avr_assert!(USB_EP2.configure(
        USBEndpointType::Interrupt,
        USBEndpointDirection::OutOrControl,
        USBEndpointSize::B64,
        USBEndpointBanks::Double,
    ));
}

const EORSTE: u8 = 3;
const EORSTI: u8 = 3;

pub struct USB {}

impl USB {
    pub fn end_of_reset_seen() -> bool {
        let udint = unsafe { avr_read_volatile(UDINT) };
        udint & (1 << EORSTI) != 0
    }
    pub fn clear_end_of_reset() {
        unsafe { avr_write_volatile(UDINT, avr_read_volatile(UDINT) & !(1 << EORSTI)) };
    }
}

pub async fn wait_usb_end_of_reset() {
    // Enable 'End of Reset' interrupt.
    let ctx = InterruptEnabledContext::new(UDIEN, 1 << EORSTE);

    loop {
        if USB::end_of_reset_seen() {
            // Clear the bit so we don't keep getting the interrupt
            USB::clear_end_of_reset();
            break;
        }

        InterruptEvent::USBGeneral.to_future().await;
    }

    drop(ctx);
}

const EPEN: u8 = 0;
const CFGOK: u8 = 7;
const ALLOC: u8 = 1;

pub struct EndpointInterruptEnabledContext {
    ep: &'static USBEndpoint,
    inner: InterruptEnabledContext,
}

impl EndpointInterruptEnabledContext {
    #[inline(always)]
    pub fn new(ep: &'static USBEndpoint, register: *mut u8, mask: u8) -> Self {
        ep.select();
        let inner = InterruptEnabledContext::new(register, mask);
        Self { ep, inner }
    }
}

// TODO: Verify the order of events here.
impl Drop for EndpointInterruptEnabledContext {
    fn drop(&mut self) {
        self.ep.select();
        // self.inner.drop();
    }
}

// TODO: One issue with USB interrupts is that only one interrupt per endpoint
// should be allowed at a time as separate futures will clear the interrupt bits
// of other pending futures.
impl USBEndpoint {
    const fn new(num: u8) -> Self {
        Self {
            num,
            num_mask: 1 << num,
        }
    }

    /// NOTE: Must be called before ANY operation on the endpoint.
    #[inline(always)]
    fn select(&'static self) {
        unsafe { avr_write_volatile(UENUM, self.num) };
    }

    // TODO: Support reset with UERST?

    // TODO: Must use return value
    pub fn configure(
        &'static self,
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
            // avr_write_volatile(UEINTX, 0);
        }

        return true;
    }

    pub fn request_stale(&'static self) {
        // panic!();
        self.select();
        // Keep endpoint enabled and also enable STALLRQ
        unsafe { avr_write_volatile(UECONX, 1 | (1 << 5)) };
    }

    // UEINTX contains whether or not the interrupt has triggered.
    // UEIENX configures if interrupts are enabled

    // NOTE: This must be called after enabling an interrupt.
    #[inline(always)]
    async fn wait_for_event(&'static self) {
        loop {
            let ueint = unsafe { avr_read_volatile(UEINT) };
            if ueint & self.num_mask != 0 {
                break;
            }

            // TODO: Consider keeping the waker in all queues until the future is dropped so
            // that we can optimize running the same future in a loop.
            InterruptEvent::USBEndpoint.to_future().await;
        }
    }

    fn check_flag(&'static self, mask: u8) -> bool {
        self.select();
        (unsafe { avr_read_volatile(UEINTX) }) & mask != 0
    }

    fn clear_flag(&'static self, mask: u8) {
        self.select();
        unsafe { avr_write_volatile(UEINTX, avr_read_volatile(UEINTX) & (!mask)) };
    }

    async fn wait_flag(&'static self, mask: u8) {
        let ctx = EndpointInterruptEnabledContext::new(self, UEIENX, mask);

        loop {
            if self.check_flag(mask) {
                break;
            }

            // Wait for next interesting event.
            self.wait_for_event().await;
        }

        drop(ctx);
    }

    pub fn bytec(&'static self) -> u16 {
        self.select();

        let low = unsafe { avr_read_volatile(UEBCLX) } as u16;
        let high = unsafe { avr_read_volatile(UEBCHX) } as u16;
        (high << 8) | low
    }

    /// NOTE: This does not protect from overflowing the FIFO.
    pub fn read_bytes(&'static self, buf: &mut [u8]) {
        self.select();
        for i in 0..buf.len() {
            buf[i] = unsafe { avr_read_volatile(UEDATX) };
        }
    }

    pub fn write_bytes(&'static self, buf: &[u8]) {
        self.select();
        for i in 0..buf.len() {
            unsafe { avr_write_volatile(UEDATX, buf[i]) };
        }
    }

    /// Call after you are done reading or writing to the current FIFO bank.
    /// This will allow the controller to send/receive from/to it and switch to
    /// a different bank if available.
    pub fn release_bank(&'static self) {
        // Clear FIFOCON
        self.clear_flag(1 << 7);
    }

    // TODO: Handle FNCERR

    pub fn received_setup(&'static self) -> bool {
        self.check_flag(RXSTPI_MASK)
    }

    pub fn clear_setup(&'static self) {
        self.clear_flag(RXSTPI_MASK);
    }

    pub async fn wait_setup(&'static self) {
        self.wait_flag(RXSTPI_MASK).await
    }

    pub fn received_data(&'static self) -> bool {
        self.check_flag(RXOUTI_MASK)
    }
    pub fn clear_received_data(&'static self) {
        self.clear_flag(RXOUTI_MASK);
    }
    pub async fn wait_received_data(&'static self) {
        self.wait_flag(RXOUTI_MASK).await;
    }

    pub fn transmitter_ready(&'static self) -> bool {
        self.check_flag(TXINI_MASK)
    }

    pub fn clear_transmitter_ready(&'static self) {
        self.clear_flag(TXINI_MASK)
    }

    pub async fn wait_transmitter_ready(&'static self) {
        self.wait_flag(TXINI_MASK).await
    }

    /// Responses to a control read request from the host with some data.
    /// NOTE: Only valid if called on the first endpoint.
    pub async fn control_respond<T: core::iter::Iterator<Item = u8>>(
        &'static self,
        pkt: &SetupPacket,
        mut data: T,
    ) {
        // TODO: Assert that the top bit of kPacketType is set.

        // Remaining number of bytes the host will accept.
        let mut host_remaining = pkt.wLength;

        // TODO: Check host_remaining > 0

        loop {
            // Once this happens, we don't expect the host to send any more packets.
            if host_remaining == 0 {
                break;
            }

            self.wait_transmitter_ready().await;

            self.select();

            let mut done = false;
            // TODO: Check this. usually bytec is 0 for the control endpoints.
            // TODO: Make this dynamic depending on the endpoint config.
            let mut packet_bytes = 64; // self.bytec();

            while packet_bytes > 0 && host_remaining > 0 {
                if let Some(byte) = data.next() {
                    unsafe { avr_write_volatile(UEDATX, byte) };
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

    pub fn control_respond_sync<T: core::iter::Iterator<Item = u8>>(
        &'static self,
        pkt: &SetupPacket,
        mut data: T,
    ) -> bool {
        // Remaining number of bytes the host will accept.
        let mut host_remaining = pkt.wLength;

        // TODO: Check host_remaining > 0

        loop {
            // Wait for transmitter ready.
            loop {
                if USB::end_of_reset_seen() {
                    return false;
                }
                if self.transmitter_ready() {
                    break;
                }
            }

            self.select();

            let mut done = false;
            // TODO: Check this. usually bytec is 0
            let mut packet_bytes = 64; // self.bytec();

            while packet_bytes > 0 && host_remaining > 0 {
                if let Some(byte) = data.next() {
                    unsafe { avr_write_volatile(UEDATX, byte) };
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

            if done || host_remaining == 0 {
                break;
            }
        }

        // Status stage
        loop {
            if USB::end_of_reset_seen() {
                return false;
            }
            if self.received_data() {
                break;
            }
        }

        self.clear_received_data();

        true
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

pub const USB_EP0: USBEndpoint = USBEndpoint::new(0);
pub const USB_EP1: USBEndpoint = USBEndpoint::new(1);
pub const USB_EP2: USBEndpoint = USBEndpoint::new(2);
pub const USB_EP3: USBEndpoint = USBEndpoint::new(3);
pub const USB_EP4: USBEndpoint = USBEndpoint::new(4);
pub const USB_EP5: USBEndpoint = USBEndpoint::new(5);
pub const USB_EP6: USBEndpoint = USBEndpoint::new(6);
