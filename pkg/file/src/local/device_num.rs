
/// Wrapper around a 'dev_t' Linux type which stores a file's major/minor device
/// numbers encoded as one u64.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct DeviceNumber(u64);

impl DeviceNumber {
    pub fn new(major: u32, minor: u32) -> Self {
        let major = major as u64;
        let minor = minor as u64;
        let mut dev = 0;

        dev |= (major & 0x0000_0fff) << 8;
        dev |= (major & 0xffff_f000) << 32;

        dev |= minor & 0x0000_00ff;
        dev |= (minor & 0xffff_ff00) << 12;

        Self(dev)
    }

    pub fn from_raw(dev: u64) -> Self {
        Self(dev)
    }

    pub fn major(&self) -> u32 {
        let dev = self.0;
        let mut major = 0;
        major |= (dev >> 8) & 0xfff;
        major |= (dev >> 32) & 0xffff_f000;
        major as u32
    }

    pub fn minor(&self) -> u32 {
        let dev = self.0;
        let mut minor = 0;
        minor |= dev & 0xff;
        minor |= (dev >> 12) & 0xffff_ff00;
        minor as u32
    }
}

impl std::fmt::Debug for DeviceNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceNumber")
            .field("major", &self.major())
            .field("minor", &self.minor())
            // .field("raw", &self.0) 
            .finish()
    }
}