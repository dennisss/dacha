// Pin port configuration and digital I/O.

const DDR_OUTPUT: u8 = 1;
const DDR_INPUT: u8 = 0;

const PORT_PULLUP: u8 = 1;
const PORT_HIGHZ: u8 = 0;

const PORT_HIGH: u8 = 1;
const PORT_LOW: u8 = 0;

macro_rules! define_port {
    ($name:ident, $pin_addr:expr, $ddr_addr:expr, $port_addr:expr, $( $pin_name:ident : $pin_num:expr ),*) => {
        // Create one struct for the port and one per pin to ensure that a lot of
        // inlining happens.
        pub struct $name {}
        impl $name {
            #[inline(always)]
            fn pin_register() -> *mut u8 {
                $pin_addr as *mut u8
            }

            #[inline(always)]
            fn ddr_register() -> *mut u8 {
                $ddr_addr as *mut u8
            }
            #[inline(always)]
            fn port_register() -> *mut u8 {
                $port_addr as *mut u8
            }

            #[inline(always)]
            pub fn configure(cfg: &PortConfig) {
                unsafe {
                    $crate::avr::registers::avr_write_volatile(Self::ddr_register(), cfg.ddr);
                    $crate::avr::registers::avr_write_volatile(Self::port_register(), cfg.port);
                }
            }
        }

        $(define_pin!{$name, $pin_name, $pin_num})*
    };
}

macro_rules! define_pin {
    ($port: ident, $name:ident, $num:expr) => {
        pub struct $name {}
        impl $name {
            #[inline(always)]
            pub fn write(high: bool) {
                let bit = (if high { PORT_HIGH } else { PORT_LOW }) << $num;
                unsafe {
                    let value = $crate::avr::registers::avr_read_volatile($port::port_register());
                    $crate::avr::registers::avr_write_volatile(
                        $port::port_register(),
                        (value & (!(1 << $num))) | bit,
                    );
                }
            }

            #[inline(always)]
            pub fn read() -> bool {
                let value =
                    unsafe { $crate::avr::registers::avr_read_volatile($port::pin_register()) };
                value & (1 << $num) != 0
            }

            // For some pins, we want to be able to perform an analog read?
        }
    };
}

define_port!(
    PB, 0x23, 0x24, 0x25,
    PB0 : 0, PB1 : 1, PB2 : 2, PB3 : 3, PB4 : 4,
    PB5 : 5, PB6 : 6, PB7 : 7
);

define_port!(
    PC, 0x26, 0x27, 0x28,
    PC6 : 6, PC7 : 7
);

define_port!(
    PD, 0x29, 0x2A, 0x2B,
    PD0 : 0, PD1 : 1, PD2 : 2, PD3 : 3, PD4 : 4,
    PD5 : 5, PD6 : 6, PD7 : 7
);

define_port!(
    PE, 0x2C, 0x2D, 0x2E,
    PE2 : 2, PE6 : 6
);

define_port!(
    PF, 0x2F, 0x30, 0x31,
    PF0 : 0, PF1 : 1, PF4 : 4,
    PF5 : 5, PF6 : 6, PF7 : 7
);

pub struct PortConfig {
    ddr: u8,
    port: u8,
}

impl PortConfig {
    pub const fn new() -> Self {
        Self { ddr: 0, port: 0 }
    }
    pub const fn input(mut self, pin: u8) -> Self {
        self.ddr |= DDR_INPUT << pin;
        self.port |= PORT_HIGHZ << pin;
        self
    }
    pub const fn input_pullup(mut self, pin: u8) -> Self {
        self.ddr |= DDR_INPUT << pin;
        self.port |= PORT_PULLUP << pin;
        self
    }
    pub const fn output_low(mut self, pin: u8) -> Self {
        self.ddr |= DDR_OUTPUT << pin;
        self.port |= PORT_LOW << pin;
        self
    }
    pub const fn output_high(mut self, pin: u8) -> Self {
        self.ddr |= DDR_OUTPUT << pin;
        self.port |= PORT_HIGH << pin;
        self
    }
}
