
Note that syscalls like `chown()` don't exist on all platforms. So internally we prefer to implement most file systems in terms of their more broadly compatible `*at` variants (e.g. like `fchownat(AT_FDCWD, ..)`).

A nice reference of all raw syscalls can be found at: https://www.chromium.org/chromium-os/developer-library/reference/linux-constants/syscalls/

TODO: Split this into the parts bound from the kernel and the custom logic on top of that.