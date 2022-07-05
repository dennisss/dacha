use core::arch::asm;
use core::mem::transmute;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use common::concat_slice::ConcatSlicePair;
use common::errors::*;
use elf::*;
use google::proto::profile::*;
use parsing::binary::*;
use sys::bindings::*;
use sys::MappedMemory;

use crate::busy::*;
use crate::memory::MemoryMap;

/*
Things we need to test:
- If we have a thread started before the profiling starts, we are able to collect its usage.
- If we have a thread start after the profiling starts, we are able to collect its usage
- We are able to show time being blocked in a syscall.

- Have 3 threads:
    - One doing nothing but sleeping
    - One doing 100% work
    - One doing work 50% of the time.
    - Verify that percentages are reasonable for time.

*/

/// Generates a CPU performance profile of the current process.
///
/// TODO: Must verify that this is able to profile all new and existing threads.
pub async fn profile_self(duration: Duration) -> Result<Profile> {
    let memory_map = MemoryMap::read_self().await?;

    let mut profile = Profile::default();
    profile.add_string_table("".into());
    profile.add_string_table("samples".into());
    profile.add_string_table("count".into());

    let mut sample_type = ValueType::default();
    sample_type.set_typ(1);
    sample_type.set_unit(2);
    profile.add_sample_type(sample_type.clone());

    // TODO: Add one sample type with name "samples" and type "count"
    // TODO: Set "time_nanos", "duration_nanos"

    // TODO: Verify that mapping[0] is the main binary.
    for (i, area) in memory_map.regions().areas.iter().enumerate() {
        // TODO: Deduplicate this check with the MemoryMap code given that we depend on
        // that containing symbols only referencing the filtered areas.
        if !area.permissions.execute {
            continue;
        }

        let mut mapping = Mapping::default();
        mapping.set_id((i + 1) as u64);

        mapping.set_memory_start(area.start_address);
        mapping.set_memory_limit(area.end_address);
        mapping.set_file_offset(area.offset);

        mapping.set_filename(profile.string_table_len() as i64);
        profile.add_string_table(area.path.clone());

        if let Some(build_id) = memory_map.build_id(&area.path) {
            let build_id = common::hex::encode(build_id);
            mapping.set_build_id(profile.string_table_len() as i64);
            profile.add_string_table(build_id);
        }

        /*
        if area.path == "[vdso]" {
            println!("HARD CODE VDSO ID");
            mapping.set_build_id(profile.string_table_len() as i64);
            profile.add_string_table("ebac629cad1060bc413e445f445303e06a737228".into());
        }
        */

        profile.add_mapping(mapping);
    }

    //////// Phase 2: Actually doing the profiling.

    let mut attr = perf_event_attr::default();
    attr.type_ = perf_type_id::PERF_TYPE_HARDWARE as u32;
    attr.size = core::mem::size_of::<perf_event_attr>() as u32;
    attr.config = perf_hw_id::PERF_COUNT_HW_CPU_CYCLES as u64;
    attr.sample_max_stack = 100;

    attr.set_freq(1);
    attr.__bindgen_anon_1.sample_period = 1000;

    // TODO: Disable ip as not very useful with callchain.
    attr.sample_type = (perf_event_sample_format::PERF_SAMPLE_IP as u64)
        | (perf_event_sample_format::PERF_SAMPLE_CALLCHAIN as u64)
        | (perf_event_sample_format::PERF_SAMPLE_TID as u64);

    // TODO: Instead disable it and set it via ioctl once the mmap is ready.
    attr.set_disabled(0); // Start event counter right away.
    attr.set_exclude_idle(1);
    attr.set_exclude_user(0);

    // TODO: Setup wakeup_watermark (and watermark) and properly poll the state of
    // the memory buffer. Basically set it to half the size and then use epoll to
    // wait for POLL_IN.

    attr.set_inherit(1);

    // let child_task = common::async_std::task::spawn(task1());

    let mut cpu = 0;
    unsafe {
        let mut node = 0;
        sys::getcpu(&mut cpu, &mut node)?;

        println!(
            "T1: Pid: {}, Tid: {}, CPU: {}, Node: {}",
            sys::getpid(),
            sys::gettid(),
            cpu,
            node
        );
    };

    let num_cpus = sys::num_cpus()?;
    if num_cpus == 0 {
        return Err(err_msg("Expected at least one CPU"));
    }

    // TODO: If we are using cgroups, can that help us avoid enumerating all of
    // these? TODO: Figure out which CPUs each thread can go on (if restricted
    // then we don't need to make as many events).
    let mut task_ids = vec![];
    for entry in std::fs::read_dir("/proc/self/task")? {
        let entry = entry?;
        let id = entry
            .path()
            .file_name()
            .and_then(|s| s.to_str())
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
    // - We can't use the 'inherit' attribute unless the events are pinned to a
    //   single CPU.
    // - 'inherit' doesn't register existing children so we must register all
    //   existing threads by themselves.
    // - We can't use PERF_FLAG_FD_OUTPUT across all events because the kernel will
    //   refuse to map buffers
    // across CPUs.
    //
    // TODO: It's possibly that some threads may be created while we are loading
    // these events and thus wouldn't be included in our metrics.
    for cpu_i in 0..num_cpus {
        // println!("Open CPU {}", cpu_i);

        let mut group_fd = -1;

        for tid in task_ids.iter().cloned() {
            // println!("Open {} {} {}", cpu_i, tid, group_fd);

            let file = unsafe {
                File::from_raw_fd(sys::perf_event_open(
                    &attr,
                    tid,
                    cpu_i as i32,
                    group_fd,
                    (PERF_FLAG_FD_CLOEXEC | PERF_FLAG_FD_NO_GROUP | PERF_FLAG_FD_OUTPUT).into(),
                )?)
            };
            // PERF_FLAG_FD_NO_GROUP

            if group_fd == -1 {
                // NOTE: If this returns EPERM, then we may need to increase
                // /proc/sys/kernel/perf_event_mlock_kb
                const PAGE_SIZE: usize = 1024;
                event_buffers.push(unsafe {
                    MappedMemory::create(
                        core::ptr::null_mut(),
                        (1 + 256) * PAGE_SIZE,
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

    let mut start_time = Instant::now();

    let child_thread2 = std::thread::spawn(task2);

    let mut profiler = ProcessProfiler {
        header_buf: [0u8; core::mem::size_of::<perf_event_header>()],
        record_buf: vec![],
    };

    let mut seen_locations = HashMap::new();

    loop {
        let mut processed = 0;

        for addr in &event_buffers {
            processed += profiler.read_perf_ring_buffer(addr.addr(), |sample| {
                /*
                // println!("Stack:");
                for ip in sample.ips.iter().cloned() {
                    let mut path = "[unknown]";
                    let mut func = "[unknown]";

                    if let Some(symbol) = memory_map.lookup_symbol(ip) {
                        path = memory_map.regions().areas[symbol.area_index].path.as_str();
                        if let Some(f) = &symbol.function_name {
                            func = f.as_str();
                        }
                    }

                    // println!("=> {:x} {} @ {}", ip, path, func);
                }
                */

                // TODO: Fast skip any memory where we are in a syscall.

                // Skip to the current instruction in the call stack.
                // This should always skip to index 1 as index 0 contains an instruction in
                // '[vsyscall]' corresponding to the perf event recording code.
                let mut first_ip_index = 0;
                while first_ip_index < sample.ips.len() && sample.ips[first_ip_index] != sample.ip {
                    first_ip_index += 1;
                }

                let mut sample_proto = Sample::default();
                for ip in (&sample.ips[first_ip_index..]).iter().cloned() {
                    let location_id = match seen_locations.get(&ip) {
                        Some(v) => *v,
                        None => {
                            let id = (profile.location_len() + 1) as u64;
                            seen_locations.insert(ip, id);

                            let mut loc = Location::default();
                            loc.set_address(ip);
                            loc.set_id(id);

                            if let Some(symbol) = memory_map.lookup_symbol(ip) {
                                loc.set_mapping_id((symbol.area_index + 1) as u64);
                            };

                            profile.add_location(loc);

                            id
                        }
                    };

                    sample_proto.add_location_id(location_id);
                }

                assert!(sample_proto.location_id_len() != 0);

                sample_proto.add_value(1);

                profile.add_sample(sample_proto);
            })?;
        }

        println!("{}", processed);

        // busy_loop();

        let now = Instant::now();
        if now >= start_time + duration {
            break;
        }

        common::async_std::task::sleep(Duration::from_millis(100)).await;
    }

    Ok(profile)
}

struct ProfileBuilder {
    data: Profile,

    /// Map from
    seen_locations: HashSet<u64>,
}

#[derive(Default)]
struct PerfSampleRecord<'a> {
    ip: u64,
    pid: u32,
    tid: u32,
    ips: &'a [u64],
}

struct ProcessProfiler {
    header_buf: [u8; core::mem::size_of::<perf_event_header>()],
    record_buf: Vec<u8>,
}

impl ProcessProfiler {
    fn read_perf_ring_buffer<'a, F: FnMut(PerfSampleRecord<'a>)>(
        &mut self,
        addr: *mut u8,
        mut sample_handler: F,
    ) -> Result<usize> {
        // TODO: Handle PERF_RECORD_LOST / overflow events.

        let head_page: &'static mut perf_event_mmap_page = unsafe { transmute(addr) };

        let data_head = unsafe { core::mem::transmute::<&u64, &AtomicU64>(&head_page.data_head) };

        let data_tail = unsafe { core::mem::transmute::<&u64, &AtomicU64>(&head_page.data_tail) };

        // TODO: Use atomic u64s
        let current_head = data_head.load(Ordering::Relaxed);
        let current_tail = data_tail.load(Ordering::Relaxed);

        let mut data: &[u8] = unsafe {
            core::slice::from_raw_parts(
                addr,
                (head_page.data_size + head_page.data_offset) as usize,
            )
        };
        data = &data[(head_page.data_offset as usize)..];

        let start_i = (current_tail % (data.len() as u64)) as usize;
        let end_i = (current_head % (data.len() as u64)) as usize;

        let (a, b) = if end_i > start_i || current_head == current_tail {
            (&data[start_i..end_i], &data[0..0])
        } else {
            (&data[start_i..], &data[0..end_i])
        };

        let mut slice = ConcatSlicePair::new(a, b);

        let total_size = slice.len();

        // TODO: Support DWARF based callbacks to avoid saving frame pointers.

        while slice.len() > 0 {
            assert_eq!(slice.read(&mut self.header_buf), self.header_buf.len());

            let header: &perf_event_header = unsafe { transmute(self.header_buf.as_ptr()) };

            // println!("{:x?}", header);

            self.record_buf
                .resize((header.size as usize) - self.header_buf.len(), 0);

            assert_eq!(slice.read(&mut self.record_buf), self.record_buf.len());

            if header.type_ == (perf_event_type::PERF_RECORD_SAMPLE as u32) {
                let mut input = &self.record_buf[..];

                let mut sample = PerfSampleRecord::default();

                // PERF_SAMPLE_IP
                {
                    sample.ip = parse_next!(input, le_u64);
                    // println!("IP: {:x}", sample.ip);
                }

                // PERF_SAMPLE_TID
                {
                    sample.pid = parse_next!(input, le_u32);
                    sample.tid = parse_next!(input, le_u32);
                    // println!("PID: {}   TID: {}", pid, tid);
                }

                // PERF_SAMPLE_CALLCHAIN
                {
                    let nr = parse_next!(input, le_u64);
                    sample.ips = unsafe {
                        core::slice::from_raw_parts(transmute(input.as_ptr()), nr as usize)
                    };
                    input = &input[(nr as usize) * 8..];
                }

                sample_handler(sample);

                assert_eq!(input.len(), 0);
            } else if header.type_ == (perf_event_type::PERF_RECORD_LOST as u32) {
                // If we are here, then we didn't read events fast enough.
                println!("Lost records!");
            }
        }

        data_tail.store(current_head, Ordering::Relaxed);

        Ok(total_size)
    }
}
