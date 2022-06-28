use core::mem::transmute;
use std::ffi::{CStr, CString};
use core::arch::asm;
use std::collections::{HashMap, BTreeMap};
use std::ops::Bound::{Included, Excluded, Unbounded};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::fs::File;

use common::errors::*;
use common::concat_slice::ConcatSlicePair;
use sys::bindings::*;
use parsing::binary::*;
use sys::MappedMemory;
use elf::*;

use crate::busy::*;
use crate::memory::MemoryMap;

/*
Things we need to test:
- If we have a thread started before the profiling starts, we are able to collect its usage.
- If we have a thread start after the profiling starts, we are able to collect its usage
- We are able to show time being blocked in a syscall.

*/

/// Generates a CPU performance profile of the current process.
///
/// TODO: Must verify that this is able to profile all new and existing threads.
pub async fn profile_self() -> Result<()> {
    let memory_map = MemoryMap::read_self().await?;

    //////// Phase 2: Actually doing the profiling.

    let mut attr = perf_event_attr::default();
    attr.type_ = perf_type_id::PERF_TYPE_HARDWARE as u32;
    attr.size = core::mem::size_of::<perf_event_attr>() as u32;
    attr.config = perf_hw_id::PERF_COUNT_HW_CPU_CYCLES as u64;
    attr.sample_max_stack = 100;

    attr.set_freq(1);
    attr.__bindgen_anon_1.sample_period = 10;

    // TODO: Disable ip as not very useful with callchain.
    attr.sample_type = (perf_event_sample_format::PERF_SAMPLE_IP as u64)
        | (perf_event_sample_format::PERF_SAMPLE_CALLCHAIN as u64) | (perf_event_sample_format::PERF_SAMPLE_TID as u64);

    // TODO: Instead disable it and set it via ioctl once the mmap is ready.
    attr.set_disabled(0); // Start event counter right away.
    // attr.set_exclude_kernel(1);
    // attr.set_exclude_hv(1);
    attr.set_exclude_idle(1);
    // attr.set_exclude_callchain_kernel(1);
    attr.set_exclude_user(0);

    attr.set_inherit(1);

    let child_task = common::async_std::task::spawn(task1());

    let mut cpu = 0;
    unsafe {
        let mut node = 0;
        sys::getcpu(&mut cpu, &mut node)?;

        println!("T1: Pid: {}, Tid: {}, CPU: {}, Node: {}", sys::getpid(), sys::gettid(), cpu, node);
    };



    let num_cpus = sys::num_cpus()?;
    if num_cpus == 0 {
        return Err(err_msg("Expected at least one CPU"));
    }

    // TODO: If we are using cgroups, can that help us avoid enumerating all of these?
    // TODO: Figure out which CPUs each thread can go on (if restricted then we don't need to make as many events).
    let mut task_ids = vec![];
    for entry in std::fs::read_dir("/proc/self/task")? {
        let entry = entry?;
        let id = entry.path().file_name().and_then(|s| s.to_str())
            .ok_or_else(|| err_msg("Task has invalid directory name"))?
            .parse::<sys::pid_t>()?;
        task_ids.push(id);
    }

    // println!("Tasks: {:?}", task_ids);

    let mut event_files: Vec<File> = vec![];
    let mut event_buffers = vec![];

    event_files.reserve_exact(num_cpus * task_ids.len());
    event_buffers.reserve_exact(num_cpus);

    // Open enough events to monitor all current/future threads of the process.
    //
    // Notes:
    // - We can't use the 'inherit' attribute unless the events are pinned to a single CPU.
    // - 'inherit' doesn't register existing children so we must register all existing threads
    //   by themselves.
    // - We can't use PERF_FLAG_FD_OUTPUT across all events because the kernel will refuse to map buffers
    // across CPUs.
    //
    // TODO: It's possibly that some threads may be created while we are loading these events and thus wouldn't be included in our metrics. 
    for cpu_i in 0..num_cpus {
        // println!("Open CPU {}", cpu_i);

        let mut group_fd = -1;

        for tid in task_ids.iter().cloned() {
            // println!("Open {} {} {}", cpu_i, tid, group_fd);

            let file = unsafe { File::from_raw_fd(
                sys::perf_event_open(&attr, tid, cpu_i as i32, group_fd, (
                    PERF_FLAG_FD_CLOEXEC | PERF_FLAG_FD_NO_GROUP | PERF_FLAG_FD_OUTPUT).into())?)
            };
            // PERF_FLAG_FD_NO_GROUP

            if group_fd == -1 {
                // TODO: Need to munmap these.
                const PAGE_SIZE: usize = 1024;
                event_buffers.push(unsafe {
                    MappedMemory::create(
                        core::ptr::null_mut(),
                        (1+128) * PAGE_SIZE,
                        PROT_READ | PROT_WRITE,
                        MAP_SHARED,
                        file.as_raw_fd(),
                        0,
                    )?
                });

                group_fd = file.as_raw_fd();
            }

            event_files.push(file);
        }
    }

    println!("Opened {} events", event_files.len());

    let child_thread2 = std::thread::spawn(task2);

    let mut profiler = ProcessProfiler {
        header_buf: [0u8; core::mem::size_of::<perf_event_header>()],
        record_buf: vec![],
        memory_map
    };

    loop {
        let mut processed = 0;

        for addr in &event_buffers {
            processed += profiler.read_perf_ring_buffer(addr.addr())?;
        }

        println!("{}", processed);

        // busy_loop();

        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    Ok(())
}


struct ProcessProfiler {
    header_buf: [u8; core::mem::size_of::<perf_event_header>()],
    record_buf: Vec<u8>,
    memory_map: MemoryMap,
}

impl ProcessProfiler {

    fn read_perf_ring_buffer<'a>(
        &mut self, addr: *mut u8
    ) -> Result<usize> {
        let head_page: &'static mut perf_event_mmap_page = unsafe { transmute(addr) };

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

        let mut slice = ConcatSlicePair::new(a, b);

        // TODO: Implement a better way to detect overflows / missed bytes.
        if slice.len() >= (head_page.data_size - 1024) as usize {
            println!("Close to overflow");
        }

        let total_size = slice.len();

        // TODO: Support DWARF based callbacks to avoid saving frame pointers.

        while slice.len() > 0 {

            assert_eq!(slice.read(&mut self.header_buf), self.header_buf.len());

            let header: &perf_event_header = unsafe { transmute(self.header_buf.as_ptr()) };

            // println!("{:x?}", header);

            self.record_buf.resize((header.size as usize) - self.header_buf.len(), 0);

            assert_eq!(slice.read(&mut self.record_buf), self.record_buf.len());

            if header.type_ == (perf_event_type::PERF_RECORD_SAMPLE as u32) {
                let mut input = &self.record_buf[..];

                // PERF_SAMPLE_IP
                {
                    let ip = parse_next!(input, le_u64);
                    // println!("IP: {:x}", ip);
                }

                // PERF_SAMPLE_TID
                {
                    let pid = parse_next!(input, le_u32);
                    let tid = parse_next!(input, le_u32);
                    // println!("PID: {}   TID: {}", pid, tid);
                }

                // PERF_SAMPLE_CALLCHAIN
                {
                    let nr = parse_next!(input, le_u64);
                    let ips: &[u64] = unsafe { core::slice::from_raw_parts(transmute(input.as_ptr()), nr as usize) };
                    input = &input[(nr as usize) * 8..];

                    println!("Stack:");
                    for ip in ips.iter().cloned() {

                        let mut path = "[unknown]";
                        let mut func = "[unknown]";

                        if let Some(symbol) = self.memory_map.lookup_symbol(ip) {
                            path = symbol.file_path;
                            if let Some(f) = symbol.function_name {
                                func = f;
                            }
                        }

                        println!("=> {:x} {} @ {}", ip, path, func);
                    }
                }

                assert_eq!(input.len(), 0);
            }

        }

        head_page.data_tail = current_head as u64;

        Ok(total_size)
    }
}

