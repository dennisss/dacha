#[macro_export]
macro_rules! define_bit_flags {
    ($struct:ident $t:ty { $($name:ident = $value:expr),* }) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        #[repr(transparent)]
        pub struct $struct {
            value: $t
        }

        impl $struct {
            $(
                pub const $name: Self = Self::from_raw($value);
            )*

            pub const fn empty() -> Self {
                Self::from_raw(0)
            }

            pub const fn from_raw(value: $t) -> Self {
                Self { value }
            }

            pub const fn to_raw(self) -> $t {
                self.value
            }

            pub const fn contains(&self, other: Self) -> bool {
                self.value & other.value == other.value
            }

            pub const fn or(self, rhs: Self) -> Self {
                Self::from_raw(self.value | rhs.value)
            }

            pub const fn remove(self, value: Self) -> Self {
                Self::from_raw(self.value & !value.value)
            }
        }

        impl ::core::convert::From<$t> for $struct {
            fn from(value: $t) -> Self {
                Self { value }
            }
        }

        impl ::core::ops::BitOr for $struct {
            type Output = Self;

            fn bitor(self, rhs: Self) -> Self::Output {
                self.or(rhs)
            }
        }

        impl ::core::fmt::Debug for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                use ::core::fmt::write;

                write!(f, stringify!($struct))?;
                write!(f, "(")?;

                let mut value = *self;
                let mut first = true;
                $(
                if value.contains(Self::$name) {
                    if first {
                        first = false;
                    } else {
                        write!(f, " | ")?;
                    }

                    write!(f, stringify!($name))?;
                    value = value.remove(Self::$name);
                }
                )*

                if value.value != 0 {
                    if !first {
                        write!(f, " | ")?;
                    }
                    write!(f, "{:b}", value.value)?;
                }

                write!(f, ")")
            }
        }
    };
}
