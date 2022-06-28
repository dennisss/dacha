use core::arch::asm;


pub async fn task1() {
    unsafe {
        let mut cpu = 0;
        let mut node = 0;
        sys::getcpu(&mut cpu, &mut node).unwrap();

        println!("T2: Pid: {}, Tid: {}, CPU: {}, Node: {}", sys::getpid(), sys::gettid(), cpu, node);
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

        println!("T3: Pid: {}, Tid: {}, CPU: {}, Node: {}", sys::getpid(), sys::gettid(), cpu, node);
    };

    let mut i: u64 = 0;
    loop {
        i += 1;
    }
}


#[inline(never)]
#[no_mangle]
pub fn busy_loop() {
    busy_loop_inner();
}

#[inline(never)]
#[no_mangle]
pub fn busy_loop_inner() {
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