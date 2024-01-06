/*
TODO: Guard against stack overflow.

e.g. Using guarded pages https://github.com/rust-lang/rust/blob/b70baa4f922a1809d79caeaeb902800c3be283b9/library/std/src/sys/unix/stack_overflow.rs#L60


x86-64 thread local stuff:
- https://www.kernel.org/doc/html/v5.17/x86/x86_64/fsgs.html
- https://wiki.osdev.org/Thread_Local_Storage

TLS_TCB_SIZE
__static_tls_size

pthread initialization:
- https://github.com/walac/glibc/blob/264aad1e6aca30efddfcfae05c44ad38bb5e137d/nptl/allocatestack.c#L409

- It uses mmap(MAP_PRIVATE | MAP_ANONYMOUS | MAP_STACK)

- Pthreads calls clone here:
    - https://github.com/walac/glibc/blob/264aad1e6aca30efddfcfae05c44ad38bb5e137d/nptl/sysdeps/pthread/createthread.c#L147


CLONE_PARENT_SETTID
CLONE_CHILD_CLEARTID using a futex


thread_data {
    tlb: u64,
    thread_id: u32,
    result: u32,
}


init_one_static_tls

dl_init_static_tls

_dl_try_allocate_static_tls


At the start of the stack, we'll place a tcbhead_t / 'pthread' struct
*/

use core::ptr::write_volatile;
use std::alloc::{alloc_zeroed, dealloc, Layout};

use base_error::*;

use crate::thread::entry::*;
use crate::thread::tls::TLSSegment;
use crate::wait::WaitStatus;
use crate::{pid_t, WaitOptions};
use crate::{CLONE_FILES, CLONE_FS, CLONE_IO, CLONE_SETTLS, CLONE_SIGHAND, CLONE_THREAD, CLONE_VM};

/// Creator of new process threads.
pub struct ThreadFactory {
    tls_segment: TLSSegment,
}

impl ThreadFactory {
    pub fn create() -> Result<Self> {
        let tls_segment = TLSSegment::find()?;
        Ok(Self { tls_segment })
    }

    pub fn spawn<F: (FnOnce() -> crate::c_int) + Send + 'static>(
        &self,
        f: F,
    ) -> Result<ChildThread> {
        let thread_func = Box::into_raw(Box::new(f) as Box<ThreadFunction>);
        self.spawn_impl(thread_func)
    }

    fn spawn_impl(&self, thread_func: *mut ThreadFunction) -> Result<ChildThread> {
        unsafe {
            let stack_size = 8 * 1024 * 1024;

            // TOOD: Instead use mmap with page aligned storage.
            let stack_layout = Layout::from_size_align(stack_size, 32).unwrap();

            let stack_start_ptr = alloc_zeroed(stack_layout);
            assert!(!stack_start_ptr.is_null());

            let mut stack_end_ptr = stack_start_ptr.add(stack_size);

            let tls_data = {
                // Add the 'pthread/tcbhead_t' struct. Just the first field should be needed for
                // making statically linked binaries work.
                //
                // In x86-64, this allows reading the value of 'fs' by reading '%fs:0'.
                stack_end_ptr = stack_end_ptr.sub(8);
                write_volatile(stack_end_ptr as *mut u64, stack_end_ptr as u64);

                let ptr = stack_end_ptr;

                stack_end_ptr = stack_end_ptr.sub(self.tls_segment.memory_size());

                let buf = unsafe {
                    core::slice::from_raw_parts_mut(stack_end_ptr, self.tls_segment.memory_size())
                };
                self.tls_segment.copy_to(buf);

                ptr
            };

            // Re-align to 16 bytes.
            {
                let mut addr = stack_end_ptr as u64;
                addr -= addr % 16;
                stack_end_ptr = addr as *mut u8;
            }

            // Align to 8 bytes
            // TODO: Check this.
            stack_end_ptr = stack_end_ptr.sub(8);

            // Frame pointer of the caller of thread_bootstrap_fn
            stack_end_ptr = stack_end_ptr.sub(8);
            write_volatile(stack_end_ptr as *mut u64, 0);

            let thread_bootstrap_fn_base_pointer = stack_end_ptr;

            // Allocations for the thread_bootstrap_fn stack. Keep changes to the stack
            // pointer 16-byte aligned.
            {
                // Pass the user function to the thread through the stack.
                stack_end_ptr = stack_end_ptr.sub(16);

                let thread_func_data = core::mem::transmute::<_, [u8; 16]>(thread_func);
                write_volatile(stack_end_ptr as *mut [u8; 16], thread_func_data);
            }

            // Note: Right here stack_end_ptr is equal to the stack pointer that will be
            // present in thread_bootstrap_fn

            // Return address used by the 'ret' in syscall_raw.
            stack_end_ptr = stack_end_ptr.sub(8);
            write_volatile(stack_end_ptr as *mut u64, thread_bootstrap_fn as u64);

            // Frame pointer for the caller of syscall_raw.
            // (this is pushed onto the stack in the main thread by syscall_raw)
            stack_end_ptr = stack_end_ptr.sub(8);
            write_volatile(
                stack_end_ptr as *mut u64,
                thread_bootstrap_fn_base_pointer as u64,
            );

            // TODO: Double check if the new thread will be send SIGCHLD signals for other
            // threads.
            //
            // TODO: If this fails, deallocate all the memory right
            // away.
            let result = crate::old::clone(
                CLONE_FILES
                    | CLONE_FS
                    | CLONE_IO
                    | CLONE_SIGHAND
                    | CLONE_THREAD // Note: No SIGCHLD will be sent with this. 
                    | CLONE_VM
                    | CLONE_SETTLS,
                stack_end_ptr as *mut crate::c_void,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                tls_data as u64,
            )?;

            Ok(ChildThread { pid: result })
        }
    }
}

pub struct ChildThread {
    pid: pid_t,
}

impl ChildThread {
    pub fn wait_blocking(self) -> Result<WaitStatus> {
        // NOTE: We assume the return value is equal to self.pid.
        Ok(unsafe { crate::waitpid(self.pid, WaitOptions::empty())? })
    }
}
