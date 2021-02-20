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
    Internal = 0,

    // These all correspond to one hardware interrupt.
    ADCComplete = 1,
    EepromReady = 2,
    Int0 = 3,
    Int1 = 4,
    Int2 = 5,
    Int3 = 6,
    Int6 = 7,
    USBGeneral = 8,
    USBEndpoint = 9,
    OutputCompareOA = 10,
    PCInt0 = 11,
    USART1DataRegisterEmpty = 12,
    USART1RxComplete = 13,
}
const NUM_INTERRUPT_EVENTS: usize = 14;

impl InterruptEvent {
    #[inline(always)]
    fn waker_list(&self) -> &'static mut WakerList {
        unsafe { &mut INTERRUPT_WAKER_LISTS.as_mut().unwrap()[*self as usize] }
    }

    pub fn to_future(self) -> WakerFuture {
        self.waker_list().add()
    }
}

///
/// TODO: Fine a cleaner solution to this problem.
pub fn wake_up_thread_by_id(id: crate::avr::thread::ThreadId) {
    let f = InterruptEvent::Internal.waker_list().add_for_thread(id);

    // NOTE: THis should only be safe as wake_up_thread_by_id is only used when
    // starting a new thread so there is no way for the thread to be stopped prior
    // to it running at least once?
    // TODO: Actually the thread could get stopped manually right after it is
    // started?
    unsafe { f.leak_waker() };

    fire_internal_interrupt();
}

/// Context which keeps an interrupt enabled as long as the object is in scope.
/// The interrupt is disabled when this is dropped.
pub struct InterruptEnabledContext {
    register: *mut u8,
    mask: u8,
}

impl InterruptEnabledContext {
    pub fn new(register: *mut u8, mask: u8) -> Self {
        unsafe { avr_write_volatile(register, avr_read_volatile(register) | mask) };
        Self { register, mask }
    }
}

impl Drop for InterruptEnabledContext {
    fn drop(&mut self) {
        unsafe {
            avr_write_volatile(
                self.register,
                avr_read_volatile(self.register) & (!self.mask),
            )
        };
    }
}

pub unsafe fn init() {
    let mut lists: [WakerList; NUM_INTERRUPT_EVENTS] = core::mem::uninitialized();
    for i in 0..NUM_INTERRUPT_EVENTS {
        lists[i] = WakerList::new();
    }
    INTERRUPT_WAKER_LISTS = Some(lists)
}

pub unsafe fn wake_all_internal() {
    while INTERNAL_INTERRUPT_PENDING {
        INTERNAL_INTERRUPT_PENDING = false;
        InterruptEvent::Internal.waker_list().wake_all();
    }
}

pub fn fire_internal_interrupt() {
    unsafe { INTERNAL_INTERRUPT_PENDING = true };
}

// The challenge with USB interrupts is that we need to
// disable them ASAP, otherwise they will just keep
// triggering and the handler in the main thread may not run

// To interrupt on those, you must have something you want
// to send, or a location you want to receive to?

#[no_mangle]
#[inline(never)]
unsafe fn event_handler(e: InterruptEvent) {
    e.waker_list().wake_all();
    wake_all_internal();
    // crate::avr::usart::USART1::send_blocking(b"<\n");
}

// Fast skipping of an interrupt if we don't care about it.
macro_rules! ignore_interrupt {
    ($name:ident) => {
        #[cfg(target_arch = "avr")]
        #[no_mangle]
        unsafe extern "C" fn $name() {
            unsafe { llvm_asm!("reti") };
        }
    };
}

// Timer 0 used for delays:
// - 0.001 / (1/(16000000 / 64)) = 250
// So set output compare to 250 to 1ms precision.
#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_1() {
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

ignore_interrupt!(__vector_5);
ignore_interrupt!(__vector_6);

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
    crate::avr::usart::USART1::send_blocking(b"USBG\n");

    // NOTE: Users of this event are responsible for clearing the appropriate bit in
    // UDINT.
    event_handler(InterruptEvent::USBGeneral);

    // crate::avr::usart::USART1::send_blocking(b"]\n");
}

// USB Endpoint/Pipe Interrupt Communication Request
#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_11() {
    crate::avr::usart::USART1::send_blocking(b"USBE\n");

    // NOTE: UEINT is automatically cleared after executing the interrupt.
    event_handler(InterruptEvent::USBEndpoint);

    // crate::avr::usart::USART1::send_blocking(b"]\n");
}

ignore_interrupt!(__vector_12);
ignore_interrupt!(__vector_13);
ignore_interrupt!(__vector_14);
ignore_interrupt!(__vector_15);
ignore_interrupt!(__vector_16);
ignore_interrupt!(__vector_17);
ignore_interrupt!(__vector_18);
ignore_interrupt!(__vector_19);
ignore_interrupt!(__vector_20);

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
unsafe extern "avr-interrupt" fn __vector_25() {
    event_handler(InterruptEvent::USART1RxComplete);
}

#[cfg(target_arch = "avr")]
#[no_mangle]
unsafe extern "avr-interrupt" fn __vector_26() {
    event_handler(InterruptEvent::USART1DataRegisterEmpty);
}

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
        unsafe { init() };

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
