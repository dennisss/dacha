# Synchronized I/O

When implementing data persistence to disk, we want to provide users with the ability to tell when their data has been fully flushed to disk. This is essential in order to provide strong consistency guarantees in databases by providing power loss / failure resilience.

This is largely complicated by several caching layers:

1. `write()` calls in Linux by default write to a kernel page cache and then return before being written to disk.
2. Hard drives may have read caches that return recently written but un-flushed data on reads.
3. Upon receiving a flush request, a hard drive may lie and report success before actually flushing to non-volatile storage cells (usually happens on enterprise SSDs which have capacitor backup power).

The remainder of this doc discusses our general approach to solving this across this project.

## Linux Write Path

Linux has `fsync()` / `fdatasync()` syscalls which we will periodically issue as write boundaries. We assume that once `fsync()` returns, all data written to a file since the last `fsync()` call (and associated file size / block list metadata) has been flushed. If we enqueued additional writes after we started an `fsync()` but before the last `fsync()` finished, we will issue another `fsync()`.

Generally for most writes, we will only use `fdatasync()` as we don't care about metadata like mtimes very much.

### Failure Handling

Based on the current references, calling `fsync()` only accounts for writes which were applied since the last `fsync()`. This means that if `fsync()` fails, we can't retry it. So, once `fsync()` errors out, we will consider a file handle to be poisoned (future writes will fail). We will require that the file is closed and then the program must either:

1. Completely restart to ensure that no internal state is dependent on the assumed state of the file.
2. Re-read the file from stratch with new file descriptors before attempting to continue writes.
3. Discard the file and write elsewhere.

Because we will be running most apps in self-restarting containers, option #1 is implemented by default when errors are not caught and cause a program to exit. But, for any program that needs to interact heavily with disks, we should implement one of the other options to ensure resilience to degraded disks (as constantly restarting on failures can be disruptive).

TODO: Implement poisoning of files. This doesn't prevent the application from re-opening the file and continueing to write though.

### New File/Directory Creation

As discussed in the `fsync()` documentation, calls to `fsync()` will NOT flush any changes to the parent directory file so a newly created file may disappear. So when creating a new file, we must `fsync()` the parent directory.

But, note that the ordering of `open`/`fsync` operations matters. In particular, we must open the directory before creating the file (otherwise the directory write may fail before the directory is opened). Specifically, we will normally perform something like:

```
dir_fd = open("dir", O_DIRECTORY | O_RDONLY)
file_fd = openat(dir_fd, "file", O_CREATE)
fsync(dir_fd)
fsync(file_fd)
```

Note: correctness of the above requires usage of `openat`.

### Write Barriers

For most I/O operations we are using io_uring to ensure writes. io_uring doesn't define the order in which operations will be executed. Additionally features like Linux I/O batching and hard drive NCQ (Native Command Queuing) may arbitrarily re-order writes. So in order to guarantee that one write 'B' happens after another write 'A' we must:

1. Enqueue write A
2. Enqueue and wait for an `fsync()`
3. Enqueue write B

## Linux Read Path

We must be able to guarantee that reads to Linux files only return durably flushed data. If this is not true, we may not notice that a previous write failed (e.g. if we restarted a program to recover from a write failure). This is not guaranteed by the 'write path' described above because failed writes may still appear in the Linux page cache.

To avoid these issues, we will open files that require reading with the `O_DIRECT` flag. We assume that Linux will then force reads to bypass the page cache and also request that any hard drive cache is also bypassed.

Note: This may matter for directories as well so when reading the contents of a directory, we should use `O_DIRECT` as well but its not documented if this will actually make any difference.

TODO: Verify the maximum sequential read performance of O_DIRECT on an HDD (not sure if there is any read ahead stuff we are missing).

## Extra Safeguards

fsync and O_DIRECT documentation are both super fuzzy and most likely some of what is mentioned above is probably wrong or could become wrong in the future. To further mitigate issues, we should do the following:

1. Use BTRFS. Based on this [study](https://www.usenix.org/system/files/atc20-rebello.pdf), BRTFS has the best behaviors in terms of reverting in-memory state after failures.
2. Have a UPS in order to try and avoid some of these issues.
3. Don't store data on local disk: it would be better to defer writes to a networked file system whichhas a single canonical 

## References

- https://wiki.postgresql.org/wiki/Fsync_Errors
- https://linux.die.net/man/2/fsync
- https://lwn.net/Articles/752063/
- https://calvin.loncaric.us/articles/CreateFile.html
- https://www.evanjones.ca/durability-filesystem.html
- https://www.usenix.org/system/files/atc20-rebello.pdf
- https://www.evanjones.ca/durability-filesystem.html
  - TODO: Pre-allocation performs much better in some environments?