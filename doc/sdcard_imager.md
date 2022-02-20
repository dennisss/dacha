

SDCard contents become identical to the image file.

- Write to /dev/sdc
- Is a block device.
    - Open with O_DIRECT and O_EXCL
- Need to queue multiple blocks to support good io

-  https://lwn.net/Articles/736534/

- "The logical block size can be determined using
       the ioctl(2) BLKSSZGET operation or from the shell using the
       command:"