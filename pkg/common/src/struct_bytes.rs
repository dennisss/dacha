// This is only safe if everything in the struct recursively is Sync + Sync (no
// mutexes, arcs, or atomic variables). TODO: Make this unsafe?
pub const unsafe fn struct_bytes<'a, T>(v: &'a T) -> &'a [u8] {
    core::slice::from_raw_parts(
        core::mem::transmute::<&T, *const u8>(v),
        core::mem::size_of::<T>(),
    )
}

pub unsafe fn struct_bytes_mut<'a, T>(v: &'a mut T) -> &'a mut [u8] {
    core::slice::from_raw_parts_mut(
        core::mem::transmute::<_, *mut u8>(v),
        core::mem::size_of::<T>(),
    )
}
