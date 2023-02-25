use alloc::string::{String, ToString};
use core::convert::From;
use core::fmt::Debug;

struct Unit(&'static str, usize, usize);

const POW2_UNITS: &'static [Unit] = &[
    Unit("B", 1, 1),
    Unit("KiB", 1, 1024),
    Unit("MiB", 1, 1024 * 1024),
    Unit("GiB", 1, 1024 * 1024 * 1024),
    Unit("TiB", 1, 1024 * 1024 * 1024 * 1024),
    Unit("PiB", 1, 1024 * 1024 * 1024 * 1024 * 1024),
];

#[derive(Clone, Copy)]
pub struct ByteCount {
    value: usize,
}

impl ByteCount {
    pub fn bytes(&self) -> usize {
        self.value
    }

    fn print(&self, units: &[Unit], approx: bool) -> String {
        if self.value == 0 {
            return format!("0 B");
        }

        if approx {
            for unit in units.iter().rev() {
                if self.value >= unit.2 {
                    let is_exact = self.value % unit.2 == 0;
                    let prefix = if is_exact { "" } else { "~" };

                    let num = (self.value as f64) / (unit.2 as f64);

                    return format!("{}{:.1} {}", prefix, num, unit.0);
                }
            }

            panic!()
        } else {
            for unit in units.iter().rev() {
                if self.value % unit.2 == 0 {
                    let num = self.value / unit.2;
                    return format!("{} {}", num, unit.0);
                }
            }

            panic!()
        }
    }
}

impl From<usize> for ByteCount {
    fn from(value: usize) -> Self {
        Self { value }
    }
}

impl Debug for ByteCount {
    fn fmt(
        &self,
        f: &mut ::core::fmt::Formatter<'_>,
    ) -> ::core::result::Result<(), ::core::fmt::Error> {
        write!(f, "{}", self.print(POW2_UNITS, true))
    }
}
