#![feature(slice_take, thread_local, c_size_t)]

use core::arch::asm;
use core::mem::transmute;
use std::ffi::{CStr, CString};
use std::time::Duration;

extern crate sys;
#[macro_use]
extern crate parsing;

use base_error::*;
use common::array_ref;
use parsing::binary::*;
use sys::VirtualMemoryMap;
use sys::{bindings::*, Errno};
use sys::{RWFlags, UmountFlags};

#[thread_local]
static mut VAL: usize = 0xAABCDDEEFF;

#[thread_local]
static mut VAL2: usize = 13;

// extern "C" {
//     static _dl_tls_static_size: sys::c_size_t;
// }

/*
      mem = mmap (NULL, size, prot,
              MAP_PRIVATE | MAP_ANONYMOUS | MAP_STACK, -1, 0);
*/

/*
Tests to write:
- General thread functionality.
- Isolation of thread local variables.
*/

// TODO: Test that registering SIGINT only gets triggered once for the top level
// thread?

extern "C" fn handle_sigchld(signum: sys::c_int) {
    println!("<<>>>>>")

    // sys::write(0, b"HIHI\n" as *const u8, 5).unwrap();
}

fn register_int() {
    use common::nix::sys::signal::Signal;
    use common::nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet};

    let action = SigAction::new(
        SigHandler::Handler(handle_sigchld),
        SaFlags::empty(),
        SigSet::empty(),
    );
    let old = unsafe { sigaction(Signal::SIGCHLD, &action) }.unwrap();

    println!("{:?}", old);
}

fn test_uring_cancel() -> Result<()> {
    let mut ring = sys::IoUring::create()?;
    let (mut submit_queue, mut completion_queue) = ring.split();

    unsafe {
        submit_queue.submit(
            sys::IoUringOp::Timeout {
                duration: Duration::from_secs(10),
            },
            1,
        )?;

        submit_queue.submit(sys::IoUringOp::Cancel { user_data: 1 }, 2)?;
    }

    completion_queue.wait(None)?;

    // This will be the timeout result
    let completion = completion_queue.retrieve().unwrap();
    println!("{:?}", completion);
    assert_eq!(completion.user_data, 1);
    assert_eq!(completion.result.timeout_result(), Err(Errno::ECANCELED));

    completion_queue.wait(None)?;

    let completion = completion_queue.retrieve().unwrap();
    println!("{:?}", completion);
    assert_eq!(completion.user_data, 2);
    assert_eq!(completion.result.timeout_result(), Ok(()));

    Ok(())
}

fn test_uring() -> Result<()> {
    let path = CString::new("test").unwrap();

    let fd = unsafe { sys::open(path.as_ptr(), sys::O_RDONLY | sys::O_CLOEXEC, 0) }?;
    println!("{}", fd);

    let mut ring = sys::IoUring::create()?;

    let (mut submit_queue, mut completion_queue) = ring.split();

    let mut buffer = [0u8; 64];
    let mut vecs = [sys::IoSliceMut::new(&mut buffer)];

    println!("A");

    unsafe {
        submit_queue.submit(
            sys::IoUringOp::ReadV {
                fd,
                offset: 0,
                buffers: &vecs,
                flags: RWFlags::empty(),
            },
            123,
        )
    }?;

    println!("B");

    completion_queue.wait(None)?;

    println!("C");

    let mut completion = completion_queue.retrieve().unwrap();
    assert_eq!(completion.user_data, 123);

    let res = completion.result.readv_result()?;

    println!("{:?}", res);

    println!("{:?}", buffer);

    completion_queue.wait(Some(std::time::Duration::from_secs(4)))?;

    Ok(())
}

fn test_dirent() -> Result<()> {
    let path = CString::new("/").unwrap();
    let fd = unsafe { sys::open(path.as_ptr(), sys::O_RDONLY | sys::O_CLOEXEC, 0) }?;

    let mut buf = [0u8; 8192];

    for i in 0..2 {
        let mut rest = unsafe { sys::getdents64(fd, &mut buf)? };

        while !rest.is_empty() {
            let d_ino = u64::from_ne_bytes(*array_ref![rest, 0, 8]);
            let d_off = u64::from_ne_bytes(*array_ref![rest, 8, 8]);
            let d_reclen = u16::from_ne_bytes(*array_ref![rest, 16, 2]) as usize;
            let d_type = rest[18];

            println!("off: {}", d_off);

            let name = &rest[19..d_reclen];
            println!("{:?}", common::bytes::Bytes::from(name));

            rest = &rest[d_reclen..];
        }

        /*
        println!("{:?}", dir);

        println!("{}", dir.len());

        println!("{:?}", common::bytes::Bytes::from(&buf[..]));
        */
    }

    Ok(())
}

fn test_waitpid() -> Result<()> {
    unsafe {
        // TODO: Test that fork() triggers us to get a SIGCHLD signal.

        // register_int();

        // Should fail as we have no child processed yet.
        assert_eq!(
            sys::waitpid(-1, sys::WaitOptions::WNOHANG),
            Err(sys::Errno::ECHILD)
        );

        let child_pid = match sys::fork()? {
            Some(pid) => pid,
            None => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                sys::exit(120);
                loop {}
            }
        };

        assert_eq!(
            sys::waitpid(child_pid, sys::WaitOptions::WNOHANG)?,
            sys::WaitStatus::NoStatus
        );

        assert_eq!(
            sys::waitpid(child_pid, sys::WaitOptions::empty())?,
            sys::WaitStatus::Exited {
                pid: child_pid,
                status: 120
            }
        );

        // Test a process we must signal.

        let child_pid = match sys::fork()? {
            Some(pid) => pid,
            None => loop {
                std::thread::sleep(std::time::Duration::from_millis(1000));
            },
        };

        assert_eq!(
            sys::waitpid(child_pid, sys::WaitOptions::WNOHANG)?,
            sys::WaitStatus::NoStatus
        );

        sys::kill(child_pid, sys::Signal::SIGSTOP)?;

        assert_eq!(
            sys::waitpid(child_pid, sys::WaitOptions::WUNTRACED)?,
            sys::WaitStatus::Stopped {
                pid: child_pid,
                signal: sys::Signal::SIGSTOP
            }
        );

        sys::kill(child_pid, sys::Signal::SIGKILL)?;

        assert_eq!(
            sys::waitpid(child_pid, sys::WaitOptions::empty())?,
            sys::WaitStatus::Signaled {
                pid: child_pid,
                signal: sys::Signal::SIGKILL,
                core_dumped: false
            }
        );

        // Test dies by signal.

        let child_pid = match sys::fork()? {
            Some(pid) => pid,
            None => loop {
                sys::exit(130);
            },
        };

        println!("{:?}", sys::waitpid(child_pid, sys::WaitOptions::empty())?);

        // TODO: Test WCONTINUED
    }

    Ok(())
}

fn test_clone_args() -> Result<()> {
    register_int();

    let pid = sys::CloneArgs::new().sigchld().spawn_process(|| {
        std::thread::sleep(std::time::Duration::from_secs(1));
        0
    })?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1))
    }
}

/*
cross build --bin sys --target aarch64-unknown-linux-gnu --release
scp -i ~/.ssh/id_cluster target/aarch64-unknown-linux-gnu/release/sys pi@10.1.0.114:~/sys

ssh -i ~/.ssh/id_cluster pi@10.1.0.114
*/

/*
/sys/block/[block_name]
    /size : Size in 512 byte blocks
    /removable : 0 or 1

*/

fn test_general() -> Result<()> {
    println!("Hello world!");

    println!("Pid: {}", unsafe { sys::getpid() });
    println!("Current Exe: {}", sys::current_exe()?);
    println!("Num CPUs: {}", sys::num_cpus()?);

    Ok(())
}

fn test_mounts() -> Result<()> {
    let mounts = sys::mounts()?;

    println!("{:#?}", mounts);

    // sys::umount("/media/dennis/boot", UmountFlags::empty())?;

    Ok(())
}

fn main() -> Result<()> {
    test_mounts()?;
    return Ok(());

    test_general()?;
    return Ok(());

    // test_clone_args()?;

    test_waitpid()?;

    // test_uring_cancel()?;

    // test_dirent()?;

    // test_uring()?;

    return Ok(());

    // std::thread::spawn(|| {
    //     println!("Hello");
    // });

    // unsafe {
    //     println!("dl_tls_static_size: {}", _dl_tls_static_size);
    // }

    // register_int();

    /*
    sys::signal(
        sys::Signal::from_raw(2),
        sys::SigAction::new(sys::SigHandler::Handler(handle_sigchld)),
    )?;
     */

    // 0x7f2e940f4420

    // register_int();

    // register_int();

    println!("==");

    // let thread_factory = sys::thread::ThreadFactory::create()?;

    // println!("{}", );

    println!("Current Exe: {}", sys::current_exe()?);

    unsafe {
        VAL = 0x10;
        println!("[Main] VAL: {:x}", VAL);
    }

    /*
    let child_thread = thread_factory.spawn(|| {
        // for i in 0..20 {
        //     println!("THREAD INTERRUPTING");
        // }

        unsafe {
            println!("[Thread] VAL: {:x}", VAL);
            VAL = 0x15;
            println!("[Thread] VAL: {:x}", VAL);
        }

        /*
        let val = unsafe { &VAL };

        let val2 = unsafe { &VAL2 };

        println!("{:x}", unsafe { core::mem::transmute::<_, u64>(val) });

        println!("{:x}", unsafe { core::mem::transmute::<_, u64>(val2) });
         */

        /*
        let fs = unsafe {
            let mut v = 0;
            sys::arch_prctl_get(sys::ARCH_GET_FS, &mut v).unwrap();
            v
        };

        println!("FS: {}", fs);

        println!("Testing here: {}", *val);

        unsafe {
            VAL = 12;
        }
        */

        0
    })?;
     */

    // for i in 0..20 {
    //     println!("Hello world this is a really really long test");
    // }

    // println!("{:?}", child_thread.wait_blocking()?);

    std::thread::sleep(std::time::Duration::from_secs(100));

    unsafe {
        println!("[Main] VAL: {:x}", VAL);
    }

    // println!("Done!");

    // println!("{}", unsafe { core::mem::transmute::<_, u64>(&a[1000000]) });

    return Ok(());

    /*
    let path = CString::new("test").unwrap();

    let fd = unsafe { sys::open(path.as_ptr(), sys::O_RDONLY | sys::O_CLOEXEC, 0) }?;
    println!("{}", fd);

    let mut buf = [0u8; 8];

    let ret = unsafe { sys::read(fd, buf.as_mut_ptr(), 8) }?;

    println!("read: {}", ret);
    println!("{:?}", std::str::from_utf8(&buf[..]));

    let ret = unsafe { sys::read(fd, buf.as_mut_ptr(), 8) }?;
    println!("read: {}", ret);
     */

    // TODO: Add a test case to verify that the correct platform specific syscall
    // numbers are being usd.

    // 0x7f947e680f20
    //   7f947e680f20

    Ok(())
}
