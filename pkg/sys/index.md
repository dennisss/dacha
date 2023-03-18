
Note that syscalls like `chown()` don't exist on all platforms. So internally we prefer to implement most file systems in terms of their more broadly compatible `*at` variants (e.g. like `fchownat(AT_FDCWD, ..)`).

TODO: Split this into the parts bound from the kernel and the custom logic on top of that.