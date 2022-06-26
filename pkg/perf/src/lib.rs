#![feature(slice_take)]

use core::mem::transmute;
use std::ffi::{CStr, CString};
use core::arch::asm;

extern crate sys;
#[macro_use]
extern crate parsing;

use common::errors::*;
use sys::bindings::*;
use sys::VirtualMemoryMap;
use parsing::binary::*;

struct ConcatSlicePair<'a> {
    a: &'a [u8],
    b: &'a [u8]
}

impl<'a> ConcatSlicePair<'a> {
    fn read(&mut self, mut out: &mut [u8]) -> usize {
        let mut total = 0;
        total += Self::read_from_slice(&mut self.a, &mut out);
        total += Self::read_from_slice(&mut self.b, &mut out);
        total
    }

    fn read_from_slice(input: &mut &'a [u8], output: &mut &mut [u8]) -> usize {
        let n = input.len().min(output.len());
        (*output)[0..n].copy_from_slice(&(*input)[0..n]);
        *input = &(*input)[n..];

        output.take_mut(..n);

        // *output = &mut (*output)[n..];
        n
    }

    fn len(&self) -> usize {
        self.a.len() + self.b.len()
    } 

}

pub struct MemorySymbol {
    start_address: u64,
    end_address: u64,
}


/// Generates a CPU performance profile of the current process.
pub fn profile_self() {

    /*
    Read memory map.

    - for each range, if it's a file,
        - Read the file and 

    */

}


fn main() -> Result<()> {
    let map = VirtualMemoryMap::read_current()?;
    println!("Map: {:#x?}", map);

    let mut attr = perf_event_attr::default();
    attr.type_ = perf_type_id::PERF_TYPE_HARDWARE as u32;
    attr.size = core::mem::size_of::<perf_event_attr>() as u32;
    attr.config = perf_hw_id::PERF_COUNT_HW_CPU_CYCLES as u64;
    attr.sample_max_stack = 100;

    // attr.set_freq(1);
    attr.__bindgen_anon_1.sample_period = 1000;

    attr.sample_type = (perf_event_sample_format::PERF_SAMPLE_IP as u64)
        | (perf_event_sample_format::PERF_SAMPLE_CALLCHAIN as u64) | (perf_event_sample_format::PERF_SAMPLE_TID as u64);


    // attr.read_format = 0;

    // TODO: Instead disable it and set it via ioctl once the mmap is ready.
    attr.set_disabled(0); // Start event counter right away.

    attr.set_exclude_kernel(0);
    attr.set_exclude_user(0);

    // TODO: This doesn't seem to be supported?
    // attr.set_inherit(1);

    // TODO: Consider ignoring idle?

    let fd = unsafe { sys::perf_event_open(&attr, 0, -1, -1, (PERF_FLAG_FD_CLOEXEC).into())? };
    println!("Open {}", fd);

    const PAGE_SIZE: usize = 1024;

    let addr = unsafe {
        sys::mmap(
            core::ptr::null_mut(),
            (1+9) * PAGE_SIZE,
            (PROT_READ | PROT_WRITE) as i32,
            MAP_SHARED as i32,
            fd,
            0,
        )?
    };

    println!("Head Addr: {:?}", addr);

    let head_page: &'static mut perf_event_mmap_page = unsafe { transmute(addr) };

    println!("Version: {}", head_page.version);

    let mut header_buf = [0u8; core::mem::size_of::<perf_event_header>()];
    let mut record_buf = vec![];

    loop {
        println!("Size: {}", head_page.data_size);
        println!("Head: {}", head_page.data_head);
        println!("Offset: {}", head_page.data_offset);
    
        // TODO: read volatile
        let current_head = head_page.data_head as usize;
        let current_tail = head_page.data_tail as usize;

        let mut data: &[u8] = unsafe { core::slice::from_raw_parts(addr, (head_page.data_size + head_page.data_offset) as usize) };
        data = &data[(head_page.data_offset as usize)..];

        let start_i = current_tail % data.len();
        let end_i = current_head % data.len();

        let (a, b) = if end_i > start_i || current_head == current_tail {
            (&data[start_i..end_i], &data[0..0])
        } else {
            (&data[start_i..], &data[0..end_i])
        };

        let mut slice = ConcatSlicePair { a, b };

        // TODO: Support DWARF based callbacks to avoid saving frame pointers.

        while slice.len() > 0 {

            assert_eq!(slice.read(&mut header_buf), header_buf.len());

            let header: &perf_event_header = unsafe { transmute(header_buf.as_ptr()) };

            println!("{:x?}", header);

            record_buf.resize((header.size as usize) - header_buf.len(), 0);

            assert_eq!(slice.read(&mut record_buf), record_buf.len());

            if header.type_ == (perf_event_type::PERF_RECORD_SAMPLE as u32) {
                let mut input = &record_buf[..];

                // PERF_SAMPLE_IP
                {
                    let ip = parse_next!(input, le_u64);
                    println!("IP: {:x}", ip);
                }

                // PERF_SAMPLE_TID
                {
                    let pid = parse_next!(input, le_u32);
                    let tid = parse_next!(input, le_u32);
                    println!("PID: {}   TID: {}", pid, tid);
                }

                // PERF_SAMPLE_CALLCHAIN
                {
                    let nr = parse_next!(input, le_u64);
                    let ips: &[u64] = unsafe { core::slice::from_raw_parts(transmute(input.as_ptr()), nr as usize) };
                    input = &input[(nr as usize) * 8..];

                    println!("Stack:");
                    for ip in ips.iter().cloned() {

                        let mut file = "";
                        let mut offset = 0;

                        for area in &map.areas {
                            if ip >= area.start_address && ip < area.end_address {
                                file = &area.path;
                                offset = area.offset + (ip - area.start_address);
                                break;
                            }
                        }

                        println!("=> {:x} {} @ {:x}", ip, file, offset);
                    }
                }

                assert_eq!(input.len(), 0);
            }

        }

        head_page.data_tail = current_head as u64;

        busy_loop();

        // std::thread::sleep(std::time::Duration::from_secs(1));
    }

    

    Ok(())
}

#[inline(never)]
#[no_mangle]
fn busy_loop() {
    busy_loop_inner();
}

#[inline(never)]
#[no_mangle]
fn busy_loop_inner() {
    let now = std::time::Instant::now();

    loop {

        let dur = (std::time::Instant::now() - now);
        if dur >= std::time::Duration::from_secs(1) {
            break;
        }

        for i in 0..100000 {
            unsafe {
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");
            }

        }

    }

}
