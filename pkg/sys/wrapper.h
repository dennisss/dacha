// Note that we are note using glibc so ideally as many of these headers as possible
// are from the linux kernel headers.

// Get rid of the glibc definitions and use the linux ones.
#define __itimerspec_defined 1
#define __timeval_defined 1
#define _SYS_TIME_H 1

#include <asm/stat.h>
#include <linux/stat.h>
#include <linux/wait.h>
#include <linux/fanotify.h>
#include <linux/inotify.h>
#include <linux/fcntl.h>
#include <linux/errno.h>
#include <linux/socket.h>
#include <linux/timex.h>
#include <linux/fs.h>
#include <linux/fuse.h>
#include <linux/gpio.h>
#include <linux/io_uring.h>
#include <linux/loop.h>
#include <linux/mman.h>
#include <linux/perf_event.h>
#include <linux/poll.h>
#include <linux/sched.h>
#include <linux/serial.h>
#include <linux/tcp.h>
#include <linux/termios.h>
#include <linux/utsname.h>
#include <linux/prctl.h>
#include <linux/capability.h>
#include <linux/netlink.h>
#include <linux/i2c.h>
#include <linux/i2c-dev.h>
#include <linux/ptp_clock.h>
#include <linux/net_tstamp.h>
#include <linux/errqueue.h>
#include <linux/sockios.h>

#include <sys/epoll.h>
#include <dirent.h>
// #include <pthread.h>
#include <signal.h>
#include <scsi/sg.h>
#include <net/if.h>
#include <netinet/ip.h>
#include <sys/syscall.h>

