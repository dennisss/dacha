/// Interrupt handlers which waker up threads.
///
/// Internal Implementation:
/// When an InterruptFuture is created, it adds an entry at the end of the
/// PENDING_EVENTS table. This entry stores:
/// - Thread Id
/// -
///
/// - Each distinct interrupt type has a distinct id/index as defined by the
///   InterruptEvent enum.
use crate::avr::registers::*;
use crate::avr::waker::*;

static mut INTERRUPT_WAKER_LISTS: Option<[WakerList; NUM_INTERRUPT_EVENTS]> = None;

static mut INTERNAL_INTERRUPT_PENDING: bool = false;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum InterruptEvent {
    // This is an internal event triggered by code.
    // Internal = 0,

    // These all correspond to one hardware interrupt.
    ADCComplete = 0,
    EepromReady = 1,
    Int0 = 2,
    Int1 = 3,
    Int2 = 4,
    Int3 = 5,
    Int6 = 6,
    USBGeneral = 7,
    USBEndpoint = 8,
    OutputCompareOA = 9,
    PCInt0 = 10,
}
const NUM_INTERRUPT_EVENTS: usize = 11;

impl InterruptEvent {
    #[inline(always)]
    fn waker_list(&self) -> &'static mut WakerList {
        // TODO: Move to a separate initialization function.
        unsafe {
            if INTERRUPT_WAKER_LISTS.is_none() {
                let mut lists: [WakerList; NUM_INTERRUPT_EVENTS] = core::mem::uninitialized();
                for i in 0..NUM_INTERRUPT_EVENTS {
                    lists[i] = WakerList::new();
                }
                INTERRUPT_WAKER_LISTS = Some(lists)
            }
        }

        unsafe { &mut INTERRUPT_WAKER_LISTS.as_mut().unwrap()[*self as usize] }
    }

    pub fn to_future(self) -> WakerFuture {
        self.waker_list().add()
    }
}

/*
pub unsafe fn wake_all_internal() {
    while INTERNAL_INTERRUPT_PENDING {
        INTERNAL_INTERRUPT_PENDING = false;
        InterruptEvent::Internal.waker_list().wake_all();
    }
}

pub fn fire_internal_interrupt() {
    unsafe { INTERNAL_INTERRUPT_PENDING = true };
}
*/

// The challenge with USB interrupts is that we need to
// disable them ASAP, otherwise they will just keep
// triggering and the handler in the main thread may not run

// To interrupt on those, you must have something you want
// to send, or a location you want to receive to?

#[no_mangle]
#[inline(never)]
unsafe fn event_handler(e: InterruptEvent) {
    e.waker_list().wake_all();
    // wake_all_internal();
}

// Timer 0 used for delays:
// - 0.001 / (1/(16000000 / 64)) = 250
// So set output compare to 250 to 1ms precision.
#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_1() {
    // InterruptEvent::Int0.waker_list().wake_all();
    event_handler(InterruptEvent::Int0);
}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_2() {
    event_handler(InterruptEvent::Int1);
}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_3() {
    event_handler(InterruptEvent::Int2);
}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_4() {
    event_handler(InterruptEvent::Int3);
}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_5() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_6() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_7() {
    event_handler(InterruptEvent::Int6);
}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_8() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_9() {
    event_handler(InterruptEvent::PCInt0);
}

// USB General Interrupt Request
#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_10() {
    event_handler(InterruptEvent::USBGeneral);

    // // Clear interrupts
    // // TODO: Check if this is done automatically.
    // avr_write_volatile(UDINT, 0);
}

// USB Endpoint/Pipe Interrupt Communication Request
#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_11() {
    // NOTE: UEINT is automatically cleared after executing the interrupt.
    event_handler(InterruptEvent::USBEndpoint);
}

// TODO: Make a cheaper single instruction interrupt that just calls RETI
#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_12() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_13() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_14() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_15() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_16() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_17() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_18() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_19() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_20() {}

// Timer/Counter0 Compare Match A
#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_21() {
    event_handler(InterruptEvent::OutputCompareOA);
}

// Timer/Counter0 Compare Match B
#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_22() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_23() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_24() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_25() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_26() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_27() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_28() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_29() {
    // NOTE: ADIF is automatially cleared when executing the interrupt.
    event_handler(InterruptEvent::ADCComplete);
}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_30() {
    // TODO: Do I need to clear a bit?
    event_handler(InterruptEvent::EepromReady);
}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_31() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_32() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_33() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_34() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_35() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_36() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_37() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_38() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_39() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_40() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_41() {}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_42() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avr::thread::poll_thread;
    use crate::define_thread;

    static mut COUNTER: usize = 0;

    define_thread!(TestThread, test_thread);
    async fn test_thread() {
        loop {
            unsafe {
                COUNTER += 1;
                InterruptEvent::Int0.to_future().await;
                COUNTER += 10;
                InterruptEvent::Int1.to_future().await;
            }
        }
    }

    fn counter() -> usize {
        unsafe { COUNTER }
    }

    fn wake_all0() {
        unsafe { event_handler(InterruptEvent::Int0) };
    }

    fn wake_all1() {
        unsafe { event_handler(InterruptEvent::Int1) };
    }

    // This is basically the same test as in the Waker class except we are using
    // interrupts.
    #[test]
    fn can_wake_a_thread() {
        TestThread::start();
        assert_eq!(counter(), 0);

        crate::avr::waker::init();

        // We haven't polled the thread yet, so no wakers are registered.
        wake_all0();
        wake_all1();
        assert_eq!(counter(), 0);

        unsafe { poll_thread(0) };
        assert_eq!(counter(), 1);

        // NOTE: Not waiting for this event
        wake_all1();
        assert_eq!(counter(), 1);

        wake_all0();
        assert_eq!(counter(), 11);

        for i in 0..10000 {
            unsafe { poll_thread(0) };
            assert_eq!(counter(), 11);
        }

        wake_all1();
        assert_eq!(counter(), 12);

        let initial_value = 12;
        for i in 0..100 {
            wake_all0();
            assert_eq!(counter(), initial_value + i * 11 + 10);
            wake_all1();
            assert_eq!(counter(), initial_value + i * 11 + 11);
        }

        TestThread::stop();
    }
}
