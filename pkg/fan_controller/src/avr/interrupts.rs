use crate::avr::registers::*;
use core::future::Future;
use core::pin::Pin;
use core::ptr::{read_volatile, write_volatile};
use core::task::Poll;

// The value of the current event being handled.
static mut CURRENT_EVENT: InterruptEvent = InterruptEvent::None;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum InterruptEvent {
    None,
    ADCComplete,
    EepromReady,
    Int0,
    Int1,
    Int2,
    Int3,
    Int6,
    USBEndOfReset,
    Tick1ms,
    OutputCompareOA,
    PCInt0, // TODO: Implement me
    USBEP(u8),
}

impl Future for InterruptEvent {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut core::task::Context<'_>) -> Poll<()> {
        if unsafe { CURRENT_EVENT } == *self {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

// The challenge with USB interrupts is that we need to
// disable them ASAP, otherwise they will just keep
// triggering and the handler in the main thread may not run

// To interrupt on those, you must have something you want
// to send, or a location you want to receive to?

pub static mut PENDING_EVENTS: u8 = 0;

#[inline(always)]
unsafe fn event_handler(e: InterruptEvent) {
    CURRENT_EVENT = e;
    crate::thread::poll_all_threads();
    CURRENT_EVENT = InterruptEvent::None;
}

// Timer 0 used for delays:
// - 0.001 / (1/(16000000 / 64)) = 250
// So set output compare to 250 to 1ms precision.

#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_1() {
    event_handler(InterruptEvent::Int0);
}
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_2() {
    event_handler(InterruptEvent::Int1);
}
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_3() {
    event_handler(InterruptEvent::Int2);
}
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_4() {
    event_handler(InterruptEvent::Int3);
}
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_7() {
    event_handler(InterruptEvent::Int6);
}
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_9() {
    event_handler(InterruptEvent::PCInt0);
}
// USB General Interrupt Request
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_10() {
    let udint = read_volatile(UDINT);

    if udint & (1 << 3) != 0 {
        event_handler(InterruptEvent::USBEndOfReset);
    }

    // Clear interrupts
    write_volatile(UDINT, 0);
}
// USB Endpoint/Pipe Interrupt Communication Request
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_11() {
    // NOTE: UEINT is automatically cleared after executing the interrupt.
    let mut ueint = read_volatile(UEINT);
    for i in 0..=6 {
        if ueint & 1 != 0 {
            event_handler(InterruptEvent::USBEP(i));
        }
        ueint >>= 1;
    }
}

// Timer/Counter0 Compare Match A
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_21() {
    event_handler(InterruptEvent::OutputCompareOA);
}

// Timer/Counter0 Compare Match B
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_22() {}

#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_29() {
    // NOTE: ADIF is automatially cleared when executing the interrupt.
    event_handler(InterruptEvent::ADCComplete);
}
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_30() {
    // TODO: Do I need to clear a bit?
    event_handler(InterruptEvent::EepromReady);
}
