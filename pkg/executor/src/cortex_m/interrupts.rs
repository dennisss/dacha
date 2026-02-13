use core::arch::asm;
use core::{
    pin::Pin,
    ptr::{read_volatile, write_volatile},
};

use peripherals_raw::nvic::*;
use peripherals_raw::Interrupt;

use crate::waker::WakerList;
use crate::CriticalSection;

/// Interrupt/exception number of the first external interrupt.
const EXTERNAL_INTERRUPT_OFFSET: usize = 16;

// TODO: Verify we use he right offset for this.
const NUM_EXTERNAL_INTERRUPTS: usize = 48; // TODO: Use Interrupt::MAX.

const NUM_INTERRUPTS: usize = EXTERNAL_INTERRUPT_OFFSET + NUM_EXTERNAL_INTERRUPTS; // TODO: Check this
static mut INTERRUPT_WAKER_LISTS: [WakerList; NUM_INTERRUPTS] = [WakerList::new(); NUM_INTERRUPTS];


#[derive(Clone, Copy)]
struct InterruptHandlerOverride {
    data: *const (),
    func: fn(*const ()),
}

static mut INTERRUPT_HANDLER_OVERRIDES: [Option<InterruptHandlerOverride>; NUM_INTERRUPTS] = [None; NUM_INTERRUPTS];

static mut DISABLED_INTERRUPTS_NESTING: usize = 0; 

const PENDSV_EXCEPTION_NUM: usize = 14;
const SYSTICK_EXCEPTION_NUM: usize = 15;

pub type InterruptHandler = unsafe extern "C" fn() -> ();

/// Prefer to use CriticalSection over this.
///
/// TODO: Make these unsafe and only use in a CriticalSection.
/// TODO: Biggest concern is interrupt disables inside of interrupt disables (e.g. using a mutex inside a mutex)
#[inline(always)]
pub unsafe fn disable_interrupts() {
    unsafe {
        asm!("cpsid i");
        DISABLED_INTERRUPTS_NESTING += 1;
    }
}

/// Prefer to use CriticalSection over this.
#[inline(always)]
pub unsafe fn enable_interrupts() {
    unsafe {
        DISABLED_INTERRUPTS_NESTING -= 1;
        if DISABLED_INTERRUPTS_NESTING == 0 {
            asm!("cpsie i");
        }
    };
}

/// Enables and overrides an interrupt to always immediately call the given function.
///
/// This has the benefit of having less overhead to invoke interrupts, but
/// after this is called, the interrupt can no longer be used in async calls
/// to 'wait_for_irq'.
///
/// NOTE: It is currently unsafe to call this if the given interrupt could interrupt
/// this function (only safe to call this during program initialization time). 
///
/// TODO: Ensure there are no wakers registered initially or after this is called. 
pub fn override_interrupt_handler(num: Interrupt, func: fn(*const ()), data: *const ()) {
    let num = num as usize;

    // Enable it and don't disable it.
    {
        let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[num + EXTERNAL_INTERRUPT_OFFSET] };
        let ctx = ExternalInterruptEnabledContext::create(num, waker_list);
        core::mem::forget(ctx);
    }

    unsafe {

        INTERRUPT_HANDLER_OVERRIDES[num + EXTERNAL_INTERRUPT_OFFSET] = Some(InterruptHandlerOverride {
            data,func
        });
    }
}

struct ExternalInterruptEnabledContext<'a> {
    nvic: NVIC,
    register_index: usize,
    register_mask: u32,
    waker_list: &'a mut WakerList,
}

impl<'a> ExternalInterruptEnabledContext<'a> {
    fn create(num: usize, waker_list: &'a mut WakerList) -> Self {
        let nvic = unsafe { NVIC::new() };
        let register_index = num / 32;
        let register_mask = (1 << (num % 32)) as u32;

        Self::new(nvic, register_index, register_mask, waker_list)
    }

    fn new(
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
        let cs = CriticalSection::new();

        // Disable the interrupt if no one else is waiting for it.
        if self.waker_list.is_empty() {
            self.nvic.icer[self.register_index].write(self.register_mask);
        }

        drop(cs);
    }
}

/// Marks a specific interrupt as pending in the NVIC so that its handler will get called soon (depending on interrupt priority and what is running now.
pub fn trigger_irq(num: Interrupt) {
    let mut nvic = unsafe { NVIC::new() };
    let num = num as usize;

    let i = num / 32;
    let bit = 1 << (num % 32);

    let v = nvic.ispr[i].read();

    nvic.ispr[i].write(v | bit);
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
    let mut cs = CriticalSection::new();

    let num = num as usize;

    let mut waker =
        crate::stack_pinned::stack_pinned(crate::thread::new_waker_for_current_thread(&mut cs));

    let waker = waker.into_pin();

    let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[num + EXTERNAL_INTERRUPT_OFFSET] };

    let waker = waker_list.insert(waker);

    let ctx = ExternalInterruptEnabledContext::create(num, waker_list);

    drop(cs);

    waker.await;

    drop(ctx);
}

// TODO: Find some way to verify this is working (see if threads are stacking)
pub fn make_high_priority_irq(num: Interrupt) {

    let mut nvic = unsafe { NVIC::new() };

    unsafe {
        // Default SysTick and PendSV to low priority.
        write_volatile(0xE000ED20 as *mut u32, 0xffffff);
    }

    for i in 0..(Interrupt::MAX + 1) {
        let register_index = i / 4;
        let register_shift = (i % 4) * 8;

        let mask = !(0xff << register_shift);

        // 0 is the highest priority.
        // Note that on most architectures, the lower 4 bits are ignored.
        // nRF52 only uses 3 bits (so only the top 3 bits of this value matter).
        let priority = if i == (num as usize) { 0 } else { 0xff };

        let v = (nvic.ipr[register_index].read() & mask) | (priority << register_shift);
        nvic.ipr[register_index].write(v); 
    }

}

pub fn trigger_pendsv() {
    let cs = CriticalSection::new();

    let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[PENDSV_EXCEPTION_NUM] };
    if waker_list.is_empty() {
        return;
    }

    // Set the PENDSVSET bit.
    unsafe { write_volatile(NVIC_ICSR, 1 << 28) };
}

pub fn trigger_systick() {
    let cs = CriticalSection::new();

    let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[SYSTICK_EXCEPTION_NUM] };
    if waker_list.is_empty() {
        return;
    }

    // Set the PENDSVSET bit.
    unsafe { write_volatile(NVIC_ICSR, 1 << 26) };
}

// TODO: Verify that this interrupt is at the same priority as all others.
pub async fn wait_for_pendsv(mut cs: CriticalSection) {
    let mut waker =
        crate::stack_pinned::stack_pinned(crate::thread::new_waker_for_current_thread(&mut cs));

    let waker = waker.into_pin();

    let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[PENDSV_EXCEPTION_NUM] };

    let waker = waker_list.insert(waker);

    drop(cs);

    waker.await;
}

// TODO: Verify interrupt priority of this.
pub async fn wait_for_systick(mut cs: CriticalSection) {
    let mut waker =
        crate::stack_pinned::stack_pinned(crate::thread::new_waker_for_current_thread(&mut cs));

    let waker = waker.into_pin();

    let waker_list = unsafe { &mut INTERRUPT_WAKER_LISTS[SYSTICK_EXCEPTION_NUM] };

    let waker = waker_list.insert(waker);

    drop(cs);

    waker.await;
}


pub async fn yield_now() {
    let cs = CriticalSection::new();
    trigger_pendsv();
    wait_for_pendsv(cs).await
}

extern "C" {
    fn entry() -> ();
}

/// NOTE: We subtract 1 from the size of this as the initial stack pointer entry
/// will be added by the linker script.
///
/// TODO: Add alignment constraints to this (for when not inserted at address
/// 0): https://developer.arm.com/documentation/dui0552/a/cortex-m3-peripherals/system-control-block/vector-table-offset-register
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

    if let Some(handler) = unsafe { &INTERRUPT_HANDLER_OVERRIDES[interrupt_num] } {
        (handler.func)(handler.data);
        return;
    }

    // TODO: Subtract 1 from this?
    let waker_list = &mut INTERRUPT_WAKER_LISTS[interrupt_num];
    waker_list.wake_all();

    let cs = CriticalSection::new();

    // Check if we need to disable the interrupt.
    if waker_list.is_empty() && interrupt_num >= EXTERNAL_INTERRUPT_OFFSET {
        let num = interrupt_num - EXTERNAL_INTERRUPT_OFFSET;

        let nvic = unsafe { NVIC::new() };
        let register_index = num / 32;
        let register_mask = (1 << (num % 32)) as u32;

        drop(ExternalInterruptEnabledContext::new(
            nvic,
            register_index,
            register_mask,
            waker_list,
        ));
    }

    // Enable interrupts.
    drop(cs);

    // Nordic requires 4 cycles to clear the interrupt.
    // The 'drop(cs)' will take at least 2 cycles.
    asm!("nop");
    asm!("nop");
}
