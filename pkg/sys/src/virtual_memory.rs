use std::fs;

use common::errors::*;

// TODO: Switch to expecting exactly 2 numbers fo the device_bus and device_num.
regexp!(LINE => "^([a-f0-9]+)-([a-f0-9]+) +(r|-)(w|-)(x|-)(p|-) +([0-9a-f]+) +([0-9a-f]+):([0-9a-f]+) +([0-9]+) +([^ ]*)$");

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
    pub p: bool,
}

impl VirtualMemoryMap {
    pub fn read_current() -> Result<Self> {
        let data = fs::read_to_string("/proc/self/maps")?;

        let mut areas = vec![];

        for line in data.lines() {
            let m = LINE.exec(line).ok_or_else(|| err_msg("Invalid line"))?;

            let start_address = u64::from_str_radix(m.group_str(1).unwrap()?, 16)?;
            let end_address = u64::from_str_radix(m.group_str(2).unwrap()?, 16)?;
            let read = m.group_str(3).unwrap()? != "-";
            let write = m.group_str(4).unwrap()? != "-";
            let execute = m.group_str(5).unwrap()? != "-";
            let p = m.group_str(6).unwrap()? != "-";

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
                    p,
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

/*
dennis@dennis-pc:~/workspace/dacha$ cat /proc/1703614/maps
562c5143a000-562c51446000 r--p 00000000 00:1b 14556217                   /home/dennis/workspace/dacha/target/debug/sys
562c51446000-562c514bc000 r-xp 0000c000 00:1b 14556217                   /home/dennis/workspace/dacha/target/debug/sys
562c514bc000-562c514da000 r--p 00082000 00:1b 14556217                   /home/dennis/workspace/dacha/target/debug/sys
562c514da000-562c514e1000 r--p 0009f000 00:1b 14556217                   /home/dennis/workspace/dacha/target/debug/sys
562c514e1000-562c514e2000 rw-p 000a6000 00:1b 14556217                   /home/dennis/workspace/dacha/target/debug/sys
562c53171000-562c53192000 rw-p 00000000 00:00 0                          [heap]
7fa998711000-7fa998713000 rw-p 00000000 00:00 0
7fa998713000-7fa998735000 r--p 00000000 00:1b 3230247                    /usr/lib/x86_64-linux-gnu/libc-2.31.so
7fa998735000-7fa9988ad000 r-xp 00022000 00:1b 3230247                    /usr/lib/x86_64-linux-gnu/libc-2.31.so
7fa9988ad000-7fa9988fb000 r--p 0019a000 00:1b 3230247                    /usr/lib/x86_64-linux-gnu/libc-2.31.so
7fa9988fb000-7fa9988ff000 r--p 001e7000 00:1b 3230247                    /usr/lib/x86_64-linux-gnu/libc-2.31.so
7fa9988ff000-7fa998901000 rw-p 001eb000 00:1b 3230247                    /usr/lib/x86_64-linux-gnu/libc-2.31.so
7fa998901000-7fa998905000 rw-p 00000000 00:00 0
7fa998905000-7fa998906000 r--p 00000000 00:1b 3230248                    /usr/lib/x86_64-linux-gnu/libdl-2.31.so
7fa998906000-7fa998908000 r-xp 00001000 00:1b 3230248                    /usr/lib/x86_64-linux-gnu/libdl-2.31.so
7fa998908000-7fa998909000 r--p 00003000 00:1b 3230248                    /usr/lib/x86_64-linux-gnu/libdl-2.31.so
7fa998909000-7fa99890a000 r--p 00003000 00:1b 3230248                    /usr/lib/x86_64-linux-gnu/libdl-2.31.so
7fa99890a000-7fa99890b000 rw-p 00004000 00:1b 3230248                    /usr/lib/x86_64-linux-gnu/libdl-2.31.so
7fa99890b000-7fa998911000 r--p 00000000 00:1b 3230260                    /usr/lib/x86_64-linux-gnu/libpthread-2.31.so
7fa998911000-7fa998922000 r-xp 00006000 00:1b 3230260                    /usr/lib/x86_64-linux-gnu/libpthread-2.31.so
7fa998922000-7fa998928000 r--p 00017000 00:1b 3230260                    /usr/lib/x86_64-linux-gnu/libpthread-2.31.so
7fa998928000-7fa998929000 r--p 0001c000 00:1b 3230260                    /usr/lib/x86_64-linux-gnu/libpthread-2.31.so
7fa998929000-7fa99892a000 rw-p 0001d000 00:1b 3230260                    /usr/lib/x86_64-linux-gnu/libpthread-2.31.so
7fa99892a000-7fa99892e000 rw-p 00000000 00:00 0
7fa99892e000-7fa998930000 r--p 00000000 00:1b 3230262                    /usr/lib/x86_64-linux-gnu/librt-2.31.so
7fa998930000-7fa998934000 r-xp 00002000 00:1b 3230262                    /usr/lib/x86_64-linux-gnu/librt-2.31.so
7fa998934000-7fa998936000 r--p 00006000 00:1b 3230262                    /usr/lib/x86_64-linux-gnu/librt-2.31.so
7fa998936000-7fa998937000 r--p 00007000 00:1b 3230262                    /usr/lib/x86_64-linux-gnu/librt-2.31.so
7fa998937000-7fa998938000 rw-p 00008000 00:1b 3230262                    /usr/lib/x86_64-linux-gnu/librt-2.31.so
7fa998938000-7fa99893b000 r--p 00000000 00:1b 875725                     /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7fa99893b000-7fa99894d000 r-xp 00003000 00:1b 875725                     /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7fa99894d000-7fa998951000 r--p 00015000 00:1b 875725                     /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7fa998951000-7fa998952000 r--p 00018000 00:1b 875725                     /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7fa998952000-7fa998953000 rw-p 00019000 00:1b 875725                     /usr/lib/x86_64-linux-gnu/libgcc_s.so.1
7fa998953000-7fa998955000 rw-p 00000000 00:00 0
7fa998972000-7fa998975000 rw-s 00000000 00:0e 12688                      anon_inode:[perf_event]
7fa998975000-7fa998976000 ---p 00000000 00:00 0
7fa998976000-7fa998978000 rw-p 00000000 00:00 0
7fa998978000-7fa998979000 r--p 00000000 00:1b 3230243                    /usr/lib/x86_64-linux-gnu/ld-2.31.so
7fa998979000-7fa99899c000 r-xp 00001000 00:1b 3230243                    /usr/lib/x86_64-linux-gnu/ld-2.31.so
7fa99899c000-7fa9989a4000 r--p 00024000 00:1b 3230243                    /usr/lib/x86_64-linux-gnu/ld-2.31.so
7fa9989a5000-7fa9989a6000 r--p 0002c000 00:1b 3230243                    /usr/lib/x86_64-linux-gnu/ld-2.31.so
7fa9989a6000-7fa9989a7000 rw-p 0002d000 00:1b 3230243                    /usr/lib/x86_64-linux-gnu/ld-2.31.so
7fa9989a7000-7fa9989a8000 rw-p 00000000 00:00 0
7ffcfa480000-7ffcfa4a1000 rw-p 00000000 00:00 0                          [stack]
7ffcfa4a5000-7ffcfa4a9000 r--p 00000000 00:00 0                          [vvar]
7ffcfa4a9000-7ffcfa4ab000 r-xp 00000000 00:00 0                          [vdso]
ffffffffff600000-ffffffffff601000 --xp 00000000 00:00 0                  [vsyscall]

*/
