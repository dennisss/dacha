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

    pub fn load_byte(&'static self, index: usize) -> Option<u8> {
        if index >= self.size_of() {
            return None;
        }

        // TODO: Verifythat this is the address in 8byte words and not in 16byte words.
        let addr = index + unsafe { core::mem::transmute::<_, usize>(self) };

        let z_high: u8 = ((addr as u16) >> 8) as u8;
        let z_low: u8 = addr as u8;

        // TODO: Can also use the Z+ instruction to efficiently increment over many
        // bytes.

        let value: u8;
        unsafe {
            llvm_asm!("ldi ZH, $0"
                :
                : "0"(z_high)
            );
            llvm_asm!("ldi ZL, $0"
                :
                : "0"(z_low)
            );
            llvm_asm!("lpm $0, Z"
                : "=r"(value));
        }
        Some(value)
    }

    pub fn iter(&'static self) -> ProgMemIter<T> {
        ProgMemIter {
            value: self,
            index: 0,
        }
    }
}

pub struct ProgMemIter<T: 'static> {
    value: &'static ProgMem<T>,
    index: usize,
}

impl<T: 'static> core::iter::Iterator for ProgMemIter<T> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        let v = self.value.load_byte(self.index);
        if v.is_some() {
            self.index += 1;
        }

        v
    }
}

#[macro_export]
macro_rules! progmem {
    ($name:ident : $typ:ident = $value:expr) => {
        #[cfg_attr(target_arch = "avr", link_section = ".progmem.data")]
        static $name: $crate::avr::progmem::ProgMem<$typ> =
            unsafe { $crate::avr::progmem::ProgMem::new($value) };
    };
}
