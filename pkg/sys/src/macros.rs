// TODO: Deduplicate with define_transparent_enum
macro_rules! define_bindings_enum {
    ($struct:ident $t:ty => $($name:ident),*) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        #[repr(transparent)]
        pub struct $struct {
            value: $t,
        }

        impl $struct {
            $(
                pub const $name: Self = Self::from_raw($crate::bindings::$name);
            )*

            pub const fn from_raw(value: $t) -> Self {
                Self { value }
            }

            pub fn to_raw(&self) -> $t {
                self.value
            }

            pub fn as_str(&self) -> Option<&'static str> {
                match self.value {
                    $(
                        $crate::bindings::$name => Some(stringify!($name)),
                    )*
                    _ => None
                }
            }
        }

        impl ::core::convert::From<$t> for $struct {
            fn from(value: $t) -> Self {
                Self { value }
            }
        }

        impl ::core::fmt::Debug for $struct {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                if let Some(name) = self.as_str() {
                    write!(f, "{}", name)
                } else {
                    write!(f, "0x{:x}", self.value)
                }
            }
        }
    };
}
