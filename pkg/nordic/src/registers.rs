// System Control Space (SCS)
pub const NVIC_ICSR: *mut u32 = 0xE000ED04 as *mut u32;
pub const NVIC_ICTR: *mut u32 = 0xE000E004 as *mut u32;
pub const NVIC_ISER0: u32 = 0xE000E100;
pub const NVIC_ICER0: u32 = 0xE000E180;
