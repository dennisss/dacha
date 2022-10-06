use core::arch::asm;
use std::time::Duration;

pub async fn task1() {
    unsafe {
        let mut cpu = 0;
        let mut node = 0;
        sys::getcpu(&mut cpu, &mut node).unwrap();

        println!(
            "T2: Pid: {}, Tid: {}, CPU: {}, Node: {}",
            sys::getpid(),
            sys::gettid(),
            cpu,
            node
        );
    };

    let mut i: u64 = 0;
    loop {
        i += 1;
    }
}

pub fn task2() {
    unsafe {
        let mut cpu = 0;
        let mut node = 0;
        sys::getcpu(&mut cpu, &mut node).unwrap();

        println!(
            "T3: Pid: {}, Tid: {}, CPU: {}, Node: {}",
            sys::getpid(),
            sys::gettid(),
            cpu,
            node
        );
    };

    let mut i: u64 = 0;
    loop {
        i += 1;
    }
}

/// Wastes CPU cycles on the current thread until the given 'duration' has
/// elapsed.
#[inline(never)]
#[no_mangle]
pub fn busy_loop(duration: Duration) {
    busy_loop_inner(duration);
}

/// Implementation of busy_loop(). This is mainly separate to
#[inline(never)]
#[no_mangle]
fn busy_loop_inner(duration: Duration) {
    let now = std::time::Instant::now();

    loop {
        let dur = (std::time::Instant::now() - now);
        if dur >= duration {
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
