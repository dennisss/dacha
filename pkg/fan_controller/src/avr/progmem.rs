/// NOTE: Do not use me directly. Instead use the progmem macro
pub struct ProgMem<T> {
    value: T,
}

impl<T> ProgMem<T> {
    pub const unsafe fn new(value: T) -> Self {
        Self { value }
    }

    pub fn size_of(&'static self) -> usize {
        core::mem::size_of::<T>()
    }

    // pub fn load_byte(&'static self, index: usize) -> Option<u8> {
    //     if index >= self.size_of() {
    //         return None;
    //     }

    //     // TODO: Verifythat this is the address in 8byte words and not in 16byte words.
    //     let addr = index + unsafe { core::mem::transmute::<_, usize>(self) };

    //     let z_high: u8 = ((addr as u16) >> 8) as u8;
    //     let z_low: u8 = addr as u8;

    //     // TODO: Can also use the Z+ instruction to efficiently increment over many
    //     // bytes.

    //     let value: u8;
    //     unsafe {
    //         llvm_asm!("ldi ZH, $0"
    //             :
    //             : "0"(z_high)
    //         );
    //         llvm_asm!("ldi ZL, $0"
    //             :
    //             : "0"(z_low)
    //         );
    //         llvm_asm!("lpm $0, Z"
    //             : "=r"(value));
    //     }
    //     Some(value)
    // }

    // pub fn iter(&'static self) -> ProgMemIter<T> {
    //     ProgMemIter {
    //         value: self,
    //         index: 0,
    //     }
    // }

    pub fn iter_bytes(&'static self) -> ProgMemIterBytes {
        ProgMemIterBytes {
            addr: unsafe { core::mem::transmute::<_, u16>(self) },
            remaining: self.size_of()
        }
    }
}

#[cfg(target_arch = "avr")]
global_asm!(
    r#"
    .global avr_progmem_load_byte
avr_progmem_load_byte:
    ; Input: Address in (r24, r25)
    ; Return: r24

    ; We use R31:R30 for Z
    ; TODO: Preserve sreg?

    movw r30, r24
    lpm r24, Z
    ret
"#
);

#[cfg(target_arch = "avr")]
extern "C" {
    pub fn avr_progmem_load_byte(addr: u16) -> u8;
}


// pub fn progmem_load_byte(addr: usize) -> u8 {

//     // TODO: Verifythat this is the address in 8byte words and not in 16byte words.
//     // let addr = index + unsafe { core::mem::transmute::<_, usize>(self) };

//     let z_high: u8 = ((addr as u16) >> 8) as u8;
//     let z_low: u8 = addr as u8;

//     // TODO: Can also use the Z+ instruction to efficiently increment over many
//     // bytes.

//     let value: u8;
//     unsafe {
//         llvm_asm!("ldi ZH, $0"
//             :
//             : "0"(z_high)
//         );
//         llvm_asm!("ldi ZL, $0"
//             :
//             : "0"(z_low)
//         );
//         llvm_asm!("lpm $0, Z"
//             : "=r"(value));
//     }

//     value
// }

pub struct ProgMemIterBytes {
    addr: u16,
    remaining: usize
}

impl core::iter::Iterator for ProgMemIterBytes {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if self.remaining == 0 {
            return None;
        }

        // let v = unsafe { avr_progmem_load_byte(self.addr) };
        let v: u8 = unsafe { *core::mem::transmute::<_, *const u8>(self.addr) };
        self.remaining -= 1;

        self.addr += 1;

        Some(v)
    }
}


// pub struct ProgMemIter<T: 'static> {
//     value: &'static ProgMem<T>,
//     index: usize,
// }

// impl<T: 'static> core::iter::Iterator for ProgMemIter<T> {
//     type Item = u8;

//     fn next(&mut self) -> Option<u8> {
//         let v = self.value.load_byte(self.index);
//         if v.is_some() {
//             self.index += 1;
//         }

//         v
//     }
// }

#[macro_export]
macro_rules! progmem {
    ($name:ident : $typ:ident = $value:expr) => {
        // #[cfg_attr(target_arch = "avr", link_section = ".progmem.data")]
        static $name: $crate::avr::progmem::ProgMem<$typ> =
            unsafe { $crate::avr::progmem::ProgMem::new($value) };
    };
}
