use base_error::*;

use crate::file::blocking_read_to_string;

// TODO: Switch to expecting exactly 2 numbers fo the device_bus and device_num.
regexp!(LINE => "^([a-f0-9]+)-([a-f0-9]+) +(r|-)(w|-)(x|-)(s|p) +([0-9a-f]+) +([0-9a-f]+):([0-9a-f]+) +([0-9]+) +([^ ]*)$");

#[derive(Clone, Debug)]
pub struct VirtualMemoryMap {
    pub areas: Vec<VirtualMemoryArea>,
}

#[derive(Clone, Debug)]
pub struct VirtualMemoryArea {
    pub start_address: u64,
    pub end_address: u64,
    pub permissions: VirtualMemoryPermissions,
    pub offset: u64,

    // TODO: Check these.
    pub device_bus: u8,
    pub device_num: u8,

    pub inode: u64,

    pub path: String,
}

#[derive(Clone, Debug)]
pub struct VirtualMemoryPermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub private: bool,
}

impl VirtualMemoryMap {
    pub fn read_current() -> Result<Self> {
        let data = blocking_read_to_string("/proc/self/maps")?;
        Self::read_data(&data)
    }

    fn read_data(data: &str) -> Result<Self> {
        let mut areas = vec![];

        for line in data.lines() {
            let m = LINE
                .exec(line)
                .ok_or_else(|| format_err!("Invalid line: \"{}\"", line))?;

            let start_address = u64::from_str_radix(m.group_str(1).unwrap()?, 16)?;
            let end_address = u64::from_str_radix(m.group_str(2).unwrap()?, 16)?;
            let read = m.group_str(3).unwrap()? != "-";
            let write = m.group_str(4).unwrap()? != "-";
            let execute = m.group_str(5).unwrap()? != "-";
            let private = m.group_str(6).unwrap()? != "p";

            let offset = u64::from_str_radix(m.group_str(7).unwrap()?, 16)?;
            let device_bus = u8::from_str_radix(m.group_str(8).unwrap()?, 16)?;
            let device_num = u8::from_str_radix(m.group_str(9).unwrap()?, 16)?;
            let inode = u64::from_str_radix(m.group_str(10).unwrap()?, 10)?;

            let path = m.group_str(11).unwrap()?;

            areas.push(VirtualMemoryArea {
                start_address,
                end_address,
                permissions: VirtualMemoryPermissions {
                    read,
                    write,
                    execute,
                    private,
                },
                offset,
                device_bus,
                device_num,
                inode,
                path: path.to_string(),
            });
        }

        Ok(Self { areas })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_current_map() {
        let map = VirtualMemoryMap::read_current().unwrap();

        let mut has_heap = false;
        for area in &map.areas {
            if area.path == "[heap]" {
                has_heap = true;
            }
        }

        assert!(has_heap);
    }

    const TEST_MAP: &'static str = "56129f495000-5612a012c000 r-xp 00000000 00:1b 28912644                   /home/dennis/workspace/dacha/target/debug/metastore
5612a012c000-5612a01a9000 r--p 00c96000 00:1b 28912644                   /home/dennis/workspace/dacha/target/debug/metastore
5612a01a9000-5612a01aa000 rw-p 00d12000 00:1b 28912644                   /home/dennis/workspace/dacha/target/debug/metastore
5612a01aa000-5612a01ab000 rw-p 00000000 00:00 0 
5612a1d1a000-5612a435d000 rw-p 00000000 00:00 0                          [heap]
7f55d8000000-7f55d8042000 rw-p 00000000 00:00 0 
7f55d8042000-7f55dc000000 ---p 00000000 00:00 0 
7f55dc000000-7f55dc04a000 rw-p 00000000 00:00 0 
7f55dc04a000-7f55e0000000 ---p 00000000 00:00 0 
7f55e0000000-7f55e003f000 rw-p 00000000 00:00 0 
7f55e003f000-7f55e4000000 ---p 00000000 00:00 0 
7f55e4000000-7f55e4041000 rw-p 00000000 00:00 0 
7f55e4041000-7f55e8000000 ---p 00000000 00:00 0 
7f55e8000000-7f55e8056000 rw-p 00000000 00:00 0 
7f55e8056000-7f55ec000000 ---p 00000000 00:00 0 
7f55ec000000-7f55ec044000 rw-p 00000000 00:00 0 
7f55ec044000-7f55f0000000 ---p 00000000 00:00 0 
7f55f0000000-7f55f0036000 rw-p 00000000 00:00 0 
7f55f0036000-7f55f4000000 ---p 00000000 00:00 0 
7f55f4000000-7f55f404f000 rw-p 00000000 00:00 0 
7f55f404f000-7f55f8000000 ---p 00000000 00:00 0 
7f55f8000000-7f55f803e000 rw-p 00000000 00:00 0 
7f55f803e000-7f55fc000000 ---p 00000000 00:00 0 
7f55fc000000-7f55fc04a000 rw-p 00000000 00:00 0 
7f55fc04a000-7f5600000000 ---p 00000000 00:00 0 
7f5600000000-7f5600059000 rw-p 00000000 00:00 0 
7f5600059000-7f5604000000 ---p 00000000 00:00 0 
7f5604000000-7f5604057000 rw-p 00000000 00:00 0 
7f5604057000-7f5608000000 ---p 00000000 00:00 0 
7f5608000000-7f5608021000 rw-p 00000000 00:00 0 
7f5608021000-7f560c000000 ---p 00000000 00:00 0 
7f5610000000-7f561003b000 rw-p 00000000 00:00 0 
7f561003b000-7f5614000000 ---p 00000000 00:00 0 
7f5618000000-7f5618021000 rw-p 00000000 00:00 0 
7f5618021000-7f561c000000 ---p 00000000 00:00 0 
7f561f17a000-7f561f27b000 rw-p 00000000 00:00 0 
7f561f5fb000-7f561f5fc000 ---p 00000000 00:00 0 
7f561f5fc000-7f561f7fc000 rw-p 00000000 00:00 0 
7f561f7fc000-7f561f7fd000 ---p 00000000 00:00 0 
7f561f7fd000-7f561f9fd000 rw-p 00000000 00:00 0 
7f561f9fd000-7f561f9fe000 ---p 00000000 00:00 0 
7f561f9fe000-7f561fbfe000 rw-p 00000000 00:00 0 
7f561fbfe000-7f561fbff000 ---p 00000000 00:00 0 
7f561fbff000-7f561fdff000 rw-p 00000000 00:00 0 
7f561fdff000-7f561fe00000 ---p 00000000 00:00 0 
7f561fe00000-7f5620000000 rw-p 00000000 00:00 0 
7f5620000000-7f562004e000 rw-p 00000000 00:00 0 
7f562004e000-7f5624000000 ---p 00000000 00:00 0 
7f5624000000-7f5624055000 rw-p 00000000 00:00 0 
7f5624055000-7f5628000000 ---p 00000000 00:00 0 
7f5628000000-7f5628502000 rw-p 00000000 00:00 0 
7f5628502000-7f562c000000 ---p 00000000 00:00 0 
7f562c051000-7f562c052000 ---p 00000000 00:00 0 
7f562c052000-7f562c054000 rw-p 00000000 00:00 0 
7f562c054000-7f562c055000 ---p 00000000 00:00 0 
7f562c055000-7f562c057000 rw-p 00000000 00:00 0 
7f562c057000-7f562c058000 ---p 00000000 00:00 0 
7f562c058000-7f562c05a000 rw-p 00000000 00:00 0 
7f562c05a000-7f562c05b000 ---p 00000000 00:00 0 
7f562c05b000-7f562c05d000 rw-p 00000000 00:00 0 
7f562c05d000-7f562c05e000 ---p 00000000 00:00 0 
7f562c05e000-7f562c060000 rw-p 00000000 00:00 0 
7f562c060000-7f562c061000 ---p 00000000 00:00 0 
7f562c061000-7f562c063000 rw-p 00000000 00:00 0 
7f562c063000-7f562c064000 ---p 00000000 00:00 0 
7f562c064000-7f562c066000 rw-p 00000000 00:00 0 
7f562c066000-7f562c067000 ---p 00000000 00:00 0 
7f562c067000-7f562c267000 rw-p 00000000 00:00 0 
7f562c267000-7f562c268000 ---p 00000000 00:00 0 
7f562c268000-7f562c26a000 rw-p 00000000 00:00 0 
7f562c26a000-7f562c26b000 ---p 00000000 00:00 0 
7f562c26b000-7f562c46b000 rw-p 00000000 00:00 0 
7f562c46b000-7f562c46c000 ---p 00000000 00:00 0 
7f562c46c000-7f562c46e000 rw-p 00000000 00:00 0 
7f562c46e000-7f562c46f000 ---p 00000000 00:00 0 
7f562c46f000-7f562c66f000 rw-p 00000000 00:00 0 
7f562c66f000-7f562c670000 ---p 00000000 00:00 0 
7f562c670000-7f562c672000 rw-p 00000000 00:00 0 
7f562c672000-7f562c673000 ---p 00000000 00:00 0 
7f562c673000-7f562c873000 rw-p 00000000 00:00 0 
7f562c873000-7f562c874000 ---p 00000000 00:00 0 
7f562c874000-7f562c876000 rw-p 00000000 00:00 0 
7f562c876000-7f562c877000 ---p 00000000 00:00 0 
7f562c877000-7f562ca77000 rw-p 00000000 00:00 0 
7f562ca77000-7f562ca78000 ---p 00000000 00:00 0 
7f562ca78000-7f562cc78000 rw-p 00000000 00:00 0 
7f562cc78000-7f562cc79000 ---p 00000000 00:00 0 
7f562cc79000-7f562ce79000 rw-p 00000000 00:00 0 
7f562ce79000-7f562ce7a000 ---p 00000000 00:00 0 
7f562ce7a000-7f562d07a000 rw-p 00000000 00:00 0 
7f562d07a000-7f562d07b000 ---p 00000000 00:00 0 
7f562d07b000-7f562d27b000 rw-p 00000000 00:00 0 
7f562d27b000-7f562d27c000 ---p 00000000 00:00 0 
7f562d27c000-7f562d47c000 rw-p 00000000 00:00 0 
7f562d47c000-7f562d47d000 ---p 00000000 00:00 0 
7f562d47d000-7f562d67d000 rw-p 00000000 00:00 0 
7f562d67d000-7f562d67e000 ---p 00000000 00:00 0 
7f562d67e000-7f562d87e000 rw-p 00000000 00:00 0 
7f562d87e000-7f562d87f000 ---p 00000000 00:00 0 
7f562d87f000-7f562da82000 rw-p 00000000 00:00 0 
7f562da82000-7f562daaa000 r--p 00000000 00:1b 11485961                   /usr/lib/x86_64-linux-gnu/libc.so.6
7f562daaa000-7f562dc3f000 r-xp 00028000 00:1b 11485961                   /usr/lib/x86_64-linux-gnu/libc.so.6
7f562dc3f000-7f562dc97000 r--p 001bd000 00:1b 11485961                   /usr/lib/x86_64-linux-gnu/libc.so.6
7f562dc97000-7f562dc9b000 r--p 00214000 00:1b 11485961                   /usr/lib/x86_64-linux-gnu/libc.so.6
7f562dc9b000-7f562dc9d000 rw-p 00218000 00:1b 11485961                   /usr/lib/x86_64-linux-gnu/libc.so.6
7f562dc9d000-7f562dcaa000 rw-p 00000000 00:00 0 
7f562dcaa000-7f562dcad000 r--p 00000000 00:1b 10807975                   /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7f562dcad000-7f562dcc4000 r-xp 00003000 00:1b 10807975                   /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7f562dcc4000-7f562dcc8000 r--p 0001a000 00:1b 10807975                   /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7f562dcc8000-7f562dcc9000 r--p 0001d000 00:1b 10807975                   /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7f562dcc9000-7f562dcca000 rw-p 0001e000 00:1b 10807975                   /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7f562dccb000-7f562dccc000 ---p 00000000 00:00 0 
7f562dccc000-7f562dcce000 rw-p 00000000 00:00 0 
7f562dcce000-7f562dccf000 ---p 00000000 00:00 0 
7f562dccf000-7f562dcd1000 rw-p 00000000 00:00 0 
7f562dcd1000-7f562dcd2000 ---p 00000000 00:00 0 
7f562dcd2000-7f562dcd4000 rw-p 00000000 00:00 0 
7f562dcd4000-7f562dcd5000 ---p 00000000 00:00 0 
7f562dcd5000-7f562dcd7000 rw-p 00000000 00:00 0 
7f562dcd7000-7f562dcd8000 ---p 00000000 00:00 0 
7f562dcd8000-7f562dcda000 rw-p 00000000 00:00 0 
7f562dcda000-7f562dcdb000 ---p 00000000 00:00 0 
7f562dcdb000-7f562dcdd000 rw-p 00000000 00:00 0 
7f562dcdd000-7f562dcde000 ---p 00000000 00:00 0 
7f562dcde000-7f562dce0000 rw-p 00000000 00:00 0 
7f562dce0000-7f562dce2000 rw-s 08000000 00:0e 12756                      anon_inode:[io_uring]
7f562dce2000-7f562dce4000 rw-s 10000000 00:0e 12756                      anon_inode:[io_uring]
7f562dce4000-7f562dce6000 rw-s 00000000 00:0e 12756                      anon_inode:[io_uring]
7f562dce6000-7f562dce7000 ---p 00000000 00:00 0 
7f562dce7000-7f562dceb000 rw-p 00000000 00:00 0 
7f562dceb000-7f562dced000 r--p 00000000 00:1b 11485958                   /usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2
7f562dced000-7f562dd17000 r-xp 00002000 00:1b 11485958                   /usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2
7f562dd17000-7f562dd22000 r--p 0002c000 00:1b 11485958                   /usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2
7f562dd23000-7f562dd25000 r--p 00037000 00:1b 11485958                   /usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2
7f562dd25000-7f562dd27000 rw-p 00039000 00:1b 11485958                   /usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2
7ffeaed5c000-7ffeaed85000 rw-p 00000000 00:00 0                          [stack]
7ffeaedeb000-7ffeaedef000 r--p 00000000 00:00 0                          [vvar]
7ffeaedef000-7ffeaedf1000 r-xp 00000000 00:00 0                          [vdso]
ffffffffff600000-ffffffffff601000 --xp 00000000 00:00 0                  [vsyscall]";

    #[test]
    fn test_data_map() {
        VirtualMemoryMap::read_data(TEST_MAP).unwrap();
    }
}
