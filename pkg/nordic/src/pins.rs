macro_rules! define_pins {
    ($($name:ident = $port:ident $num:expr),*) => {
        pub struct Pins {
            $(pub $name: $name),*
        }

        impl Pins {
            pub unsafe fn new() -> Self {
                Self {
                    $($name: $name { hidden: () }),*
                }
            }
        }

        $(
            pub struct $name { hidden: () },

            impl Pin for $name {
                fn port() -> Port { $port }
                fn num() -> u8 { $num }
            }
        )*

    };
}

define_pins!(
    P0_00 = P0 0,
    P0_01 = P0 1,
    P0_02 = P0 2,
    P0_03 = P0 3,
    P0_04 = P0 4,
    P0_05 = P0 5,
    P0_06 = P0 6,
    P0_07 = P0 7,
    P0_08 = P0 8,
    P0_09 = P0 9,
    P0_10 = P0 10,
    P0_11 = P0 11,
    P0_12 = P0 12,
    P0_13 = P0 13,
    P0_14 = P0 14,
    P0_15 = P0 15,
    P0_16 = P0 16,
    P0_17 = P0 17,
    P0_18 = P0 18,
    P0_19 = P0 19,
    P0_20 = P0 20,
    P0_21 = P0 21,
    P0_22 = P0 22,
    P0_23 = P0 23,
    P0_24 = P0 24,
    P0_25 = P0 25,
    P0_26 = P0 26,
    P0_27 = P0 27,
    P0_28 = P0 28,
    P0_29 = P0 29,
    P0_30 = P0 30,
    P0_31 = P0 31,
    P1_00 = P1 0,
    P1_01 = P1 1,
    P1_02 = P1 2,
    P1_03 = P1 3,
    P1_04 = P1 4,
    P1_05 = P1 5,
    P1_06 = P1 6,
    P1_07 = P1 7,
    P1_08 = P1 8,
    P1_09 = P1 9,
    P1_10 = P1 10,
    P1_11 = P1 11,
    P1_12 = P1 12,
    P1_13 = P1 13,
    P1_14 = P1 14,
    P1_15 = P1 15
);

pub enum Port {
    P0 = 0,
    P1 = 1,
}

pub trait Pin {
    fn port() -> Port;
    fn pin() -> u8;
}
