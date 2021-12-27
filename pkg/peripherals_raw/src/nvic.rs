/*

Cortex M4 SCS is documentd at:
- https://developer.arm.com/documentation/100166/0001/System-Control/System-control-registers

Cortex M4 NVIC is documented at:
- https://developer.arm.com/documentation/100166/0001/Nested-Vectored-Interrupt-Controller/NVIC-programmers-model/Table-of-NVIC-registers

*/

pub const NVIC_ICTR: *mut u32 = 0xE000E004 as *mut u32;

// System Control Space (SCS)
pub const NVIC_ICSR: *mut u32 = 0xE000ED04 as *mut u32;

// NVIC Proper
pub const NVIC_ISER0: u32 = 0xE000E100;
pub const NVIC_ICER0: u32 = 0xE000E180;
