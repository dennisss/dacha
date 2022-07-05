use std::ffi::CStr;
use std::fmt::Debug;

use crate::bindings::new_utsname;
use crate::errno::Errno;

pub struct UtsName {
    raw: new_utsname,
}

impl UtsName {
    pub fn read() -> Result<Self, Errno> {
        let mut raw = new_utsname::default();
        unsafe { crate::uname(&mut raw) }?;
        Ok(Self { raw })
    }

    pub fn sysname(&self) -> &str {
        Self::get(&self.raw.sysname)
    }

    pub fn nodename(&self) -> &str {
        Self::get(&self.raw.nodename)
    }

    pub fn release(&self) -> &str {
        Self::get(&self.raw.release)
    }

    pub fn version(&self) -> &str {
        Self::get(&self.raw.version)
    }

    pub fn machine(&self) -> &str {
        Self::get(&self.raw.machine)
    }

    pub fn domainname(&self) -> &str {
        Self::get(&self.raw.domainname)
    }

    fn get(data: &[i8]) -> &str {
        CStr::from_bytes_until_nul(unsafe { core::mem::transmute::<&[i8], &[u8]>(data) })
            .unwrap()
            .to_str()
            .unwrap()
    }
}

impl Debug for UtsName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UtsName")
            .field("sysname", &self.sysname())
            .field("nodename", &self.nodename())
            .field("release", &self.release())
            .field("version", &self.version())
            .field("machine", &self.machine())
            .field("domainname", &self.domainname())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utsname_works() {
        let name = UtsName::read().unwrap();
        assert_eq!(name.sysname(), "Linux");
        assert_eq!(name.machine(), "x86_64");
        assert_eq!(name.domainname(), "(none)");
        println!("{:#?}", name);
    }
}
