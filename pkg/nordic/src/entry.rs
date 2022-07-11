// Defines the entry() function which is the first thing that runs as part of
// every program.
//
// Until main() is called, entry() is not allowed to reference any symbols or
// functions outside of the '.entry' link section. Similarly nothing outside of
// the '.entry' link section should reference anything inside of the link
// section. The reason for this to allow main() to run from RAM. Because entry()
// initializes RAM, it is unsafe for entry() to depend on RAM functions.

use core::arch::asm;
use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use executor::interrupts::InterruptHandler;
use peripherals::raw::nvic::NVIC_VTOR;

extern "C" {
    static mut _sbss: u32;
    static mut _ebss: u32;

    static mut _sdata: u32;
    static mut _edata: u32;

    static _sidata: u32;

    static _vector_table: u32;
}

#[inline(never)]
#[link_section = ".entry"]
unsafe fn zero_bss() {
    let start = core::mem::transmute::<_, u32>(&_sbss);
    let end = core::mem::transmute::<_, u32>(&_ebss);

    let z: u32 = 0;
    for addr in start..end {
        asm!("strb {}, [{}]", in(reg) z, in(reg) addr);
    }
}

#[inline(never)]
#[link_section = ".entry"]
unsafe fn init_data() {
    let in_start = core::mem::transmute::<_, u32>(&_sidata);
    let out_start = core::mem::transmute::<_, u32>(&_sdata);
    let out_end = core::mem::transmute::<_, u32>(&_edata);

    for i in 0..(out_end - out_start) {
        let z = read_volatile((in_start + i) as *mut u8);
        let addr = out_start + i;

        asm!("strb {}, [{}]", in(reg) z, in(reg) addr);
    }
}

// TODO: Move to a different file.
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

// TODO: Move to a different file.
#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

#[inline(always)]
#[link_section = ".entry"]
pub unsafe fn entry_impl() {
    zero_bss();
    init_data();

    // When booting main() from RAM, the vector table may not be located at offset 0
    // in flash.
    core::ptr::write_volatile(NVIC_VTOR, core::mem::transmute(&_vector_table));
}

#[no_mangle]
#[link_section = ".entry"]
unsafe extern "C" fn entry_default_interrupt() -> () {
    loop {
        asm!("nop")
    }
}

extern "C" {
    fn entry() -> ();
}

/// Minimal interrupt vector table needed to run entry().
#[no_mangle]
#[link_section = ".entry_vector_table"]
static ENTRY_RESET_VECTOR: [::executor::interrupts::InterruptHandler; 15] = [
    entry,
    entry_default_interrupt, // NMI
    entry_default_interrupt, // Hard fault
    entry_default_interrupt, // Memory management fault
    entry_default_interrupt, // Bus fault
    entry_default_interrupt, // Usage fault
    entry_default_interrupt, // reserved 7
    entry_default_interrupt, // reserved 8
    entry_default_interrupt, // reserved 9
    entry_default_interrupt, // reserved 10
    entry_default_interrupt, // SVCall
    entry_default_interrupt, // Reserved for debug
    entry_default_interrupt, // Reserved
    entry_default_interrupt, // PendSV
    entry_default_interrupt, // Systick
];

#[macro_export]
macro_rules! entry {
    ($main:expr) => {
        #[no_mangle]
        #[link_section = ".entry"]
        pub extern "C" fn entry() -> () {
            unsafe {
                $crate::entry::entry_impl();
                $main()
            }
        }
    };
}
