use core::{
    pin::Pin,
    ptr::{read_volatile, write_volatile},
};

use peripherals_raw::nvic::*;
use peripherals_raw::Interrupt;

use crate::waker::WakerList;

/// Interrupt/exception number of the first external interrupt.
const EXTERNAL_INTERRUPT_OFFSET: usize = 16;

// TODO: Verify we use he right offset for this.
const NUM_EXTERNAL_INTERRUPTS: usize = 48; // TODO: Use Interrupt::MAX.

const NUM_INTERRUPTS: usize = EXTERNAL_INTERRUPT_OFFSET + NUM_EXTERNAL_INTERRUPTS; // TODO: Check this
static mut INTERRUPT_WAKER_LISTS: [WakerList; NUM_INTERRUPTS] = [WakerList::new(); NUM_INTERRUPTS];

const PENDSV_EXCEPTION_NUM: usize = 14;

type InterruptHandler = unsafe extern "C" fn() -> ();

struct ExternalInterruptEnabledContext<'a> {
    nvic: NVIC,
    register_index: usize,
    register_mask: u32,
    waker_list: &'a mut WakerList,
}

impl<'a> ExternalInterruptEnabledContext<'a> {
    pub fn new(
        mut nvic: NVIC,
        register_index: usize,
        register_mask: u32,
        waker_list: &'a mut WakerList,
    ) -> Self {
        nvic.iser[register_index].write(register_mask);

        Self {
            nvic,
            register_index,
            register_mask,
            waker_list,
        }
    }
}

impl Drop for ExternalInterruptEnabledContext<'_> {
    fn drop(&mut self) {
        // Disable the interrupt if no one else is waiting for it.
        if self.waker_list.is_empty() {
            self.nvic.icer[self.register_index].write(self.register_mask);
        }
    }
}

/// Waits for the given external interrupt to be triggered.
///
/// When the interrupt is triggered, this function will return while still
/// running in the interrupt handler.
///
/// For NRF52 chips, the user MUST write 0 to the EVENT registers that were set
/// high by the interrupt to avoid marking the interrupt as pending immediately
/// after the interrupt handler returns.
pub async fn wait_for_irq(num: Interrupt) {
    let num = num as usize;

    let mut waker =
        crate::stack_pinned::stack_pinned(crate::thread::new_waker_for_current_thread());

    let waker = waker.into_pin();

    let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[num + EXTERNAL_INTERRUPT_OFFSET] };

    let waker = waker_list.insert(waker);

    let nvic = unsafe { NVIC::new() };
    let register_index = num / 32;
    let register_mask = (1 << (num % 32)) as u32;

    let ctx = ExternalInterruptEnabledContext::new(nvic, register_index, register_mask, waker_list);

    waker.await;

    drop(ctx);
}

pub fn trigger_pendsv() {
    let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[PENDSV_EXCEPTION_NUM] };
    if waker_list.is_empty() {
        return;
    }

    // Set the PENDSVSET bit.
    unsafe { write_volatile(NVIC_ICSR, 1 << 28) };
}

// TODO: Verify that this interrupt is at the same priority as all others.
pub async fn wait_for_pendsv() {
    let mut waker =
        crate::stack_pinned::stack_pinned(crate::thread::new_waker_for_current_thread());

    let waker = waker.into_pin();

    let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[PENDSV_EXCEPTION_NUM] };

    let waker = waker_list.insert(waker);
    waker.await;
}

extern "C" {
    fn entry() -> ();
}

/// NOTE: We subtract 1 from the size of this as the initial stack pointer entry
/// will be added by the linker script.
#[link_section = ".vector_table.reset_vector"]
#[no_mangle]
static RESET_VECTOR: [InterruptHandler; EXTERNAL_INTERRUPT_OFFSET - 1 + NUM_EXTERNAL_INTERRUPTS] = [
    entry,             // Reset
    default_interrupt, // NMI
    default_interrupt, // Hard fault
    default_interrupt, // Memory management fault
    default_interrupt, // Bus fault
    default_interrupt, // Usage fault
    default_interrupt, // reserved 7
    default_interrupt, // reserved 8
    default_interrupt, // reserved 9
    default_interrupt, // reserved 10
    default_interrupt, // SVCall
    default_interrupt, // Reserved for debug
    default_interrupt, // Reserved
    default_interrupt, // PendSV
    default_interrupt, // Systick
    default_interrupt, // IRQ0
    default_interrupt, // IRQ1
    default_interrupt, // IRQ2
    default_interrupt, // IRQ3
    default_interrupt, // IRQ4
    default_interrupt, // IRQ5
    default_interrupt, // IRQ6
    default_interrupt, // IRQ7
    default_interrupt, // IRQ8
    default_interrupt, // IRQ9
    default_interrupt, // IRQ10
    default_interrupt, // IRQ11
    default_interrupt, // IRQ12
    default_interrupt, // IRQ13
    default_interrupt, // IRQ14
    default_interrupt, // IRQ15
    default_interrupt, // IRQ16
    default_interrupt, // IRQ17
    default_interrupt, // IRQ18
    default_interrupt, // IRQ19
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
    default_interrupt,
];

#[no_mangle]
unsafe extern "C" fn default_interrupt() -> () {
    let interrupt_num = (read_volatile(NVIC_ICSR) & 0xff) as usize;

    if interrupt_num <= 8 {
        loop {
            asm!("nop");
        }
    }

    // unsafe { asm!("cpsid i") }

    // TODO: Subtract 1 from this?
    INTERRUPT_WAKER_LISTS[interrupt_num].wake_all();

    // Enable interrupts.
    // unsafe { asm!("cpsie i") };

    asm!("nop");
    asm!("nop");
    asm!("nop");
    asm!("nop");
}
