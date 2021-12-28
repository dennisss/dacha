/*

Cortex M4 SCS is documentd at:
- https://developer.arm.com/documentation/100166/0001/System-Control/System-control-registers

Cortex M4 NVIC is documented at:
- https://developer.arm.com/documentation/100166/0001/Nested-Vectored-Interrupt-Controller/NVIC-programmers-model/Table-of-NVIC-registers

*/

use core::mem::transmute;
use core::ops::{Deref, DerefMut};

use crate::register::RawRegister;

pub const NVIC_ICTR: *mut u32 = 0xE000E004 as *mut u32;

// System Control Space (SCS)
pub const NVIC_ICSR: *mut u32 = 0xE000ED04 as *mut u32;

pub struct NVIC {
    hidden: (),

    /// Interrupt Set-Enable Registers
    pub iser: NVIC_ISER,
    pub icer: NVIC_ICER,
}

impl NVIC {
    pub unsafe fn new() -> Self {
        Self {
            hidden: (),
            iser: NVIC_ISER { hidden: () },
            icer: NVIC_ICER { hidden: () },
        }
    }
}

macro_rules! define_register_array {
    ($name:ident, $addr:expr, $len:expr) => {
        pub struct $name {
            hidden: (),
        }

        impl $name {
            const ADDR: u32 = $addr;
        }

        impl Deref for $name {
            type Target = [RawRegister<u32>; $len];

            fn deref(&self) -> &Self::Target {
                unsafe { transmute(Self::ADDR) }
            }
        }

        impl DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                unsafe { transmute(Self::ADDR) }
            }
        }
    };
}

define_register_array!(NVIC_ISER, 0xE000E100, 8);
define_register_array!(NVIC_ICER, 0xE000E180, 8);
