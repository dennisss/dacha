use core::arch::global_asm;

pub type ThreadFunction = dyn FnOnce() -> crate::c_int;

extern "C" {
    /// First segment of code executed in a child thread.
    /// - Arguments as passed to this function through a pre-initialized stack.
    /// - Its job is to extract values from the stack in fixed locations and
    ///   pass them to thread_entry_fn.
    pub fn thread_bootstrap_fn();
}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
global_asm!(
    r#"
.global thread_bootstrap_fn
thread_bootstrap_fn:
    mov rdi, [rsp]
    mov rsi, [rsp + 8]
    call thread_entry_fn
    "#
);

#[cfg(target_arch = "aarch64")]
#[no_mangle]
global_asm!(
    r#"
.global thread_bootstrap_fn
thread_bootstrap_fn:
    mov x0, [sp]
    mov x1, [sp + 8]
    call thread_entry_fn
    "#
);

// TODO: Verify that the backtrace for this looks correct.

#[no_mangle]
unsafe extern "C" fn thread_entry_fn(thread_func: *mut ThreadFunction) {
    let status = {
        let thread_func = Box::from_raw(thread_func);
        thread_func()
    };

    // TODO: dealloc the stack memory (assuming we can call exit() without any stack
    // pushes).

    crate::exit(status);
}
