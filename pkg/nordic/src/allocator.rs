use alloc::alloc::GlobalAlloc;
use alloc::alloc::Layout;
use core::cell::UnsafeCell;
use core::mem::transmute;

extern "C" {
    static mut _sheap: u8;
}

#[alloc_error_handler]
fn on_oom(_layout: Layout) -> ! {
    // asm::bkpt();

    loop {}
}

struct BumpAllocator {
    next: UnsafeCell<*mut u8>,
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut p = *self.next.get();
        p = p.add(p.align_offset(layout.align()));
        *self.next.get() = p.add(layout.size());
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let next = &mut *self.next.get();
        if ptr.add(layout.size()) == *next {
            *next = ptr;
        }

        // Never deallocs anything.
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let next = &mut *self.next.get();
        if ptr.add(layout.size()) == *next {
            *next = next.offset((new_size as isize) - (layout.size() as isize));
            return ptr;
        }

        GlobalAlloc::realloc(self, ptr, layout, new_size)
    }
}

unsafe impl Sync for BumpAllocator {}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator {
    next: UnsafeCell::new(unsafe { transmute(&_sheap) }),
};
