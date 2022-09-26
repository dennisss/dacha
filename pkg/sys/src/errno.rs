use std::fmt::Display;

#[derive(Fail, PartialEq, Eq)]
pub struct Errno(pub i64);

impl std::fmt::Display for Errno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = self.name() {
            if let Some(desc) = self.description() {
                return write!(f, "[{}] {}", name, desc);
            }
        }

        write!(f, "{:?}", self)
    }
}

impl core::fmt::Debug for Errno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

/*

#[derive(Debug, Fail)]
pub(crate) struct ProtocolErrorV1 {
    pub code: StatusCode,

    /// NOTE: This isn't sent back to the HTTP client as there is no good method
    /// of doing that without sending a body (which may be misinterpreted by
    /// a client).
    pub message: &'static str,
}

impl std::fmt::Display for ProtocolErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}: {}] {}",
            self.code.as_u16(),
            self.code.default_reason().unwrap_or(""),
            self.message
        )
    }
}
*/

macro_rules! errno_values {
    ($($name:ident, $desc:expr);* ;) => {
        impl Errno {
            $(
            pub const $name: Self = Self(crate::bindings::$name as i64);
            )*

            pub fn name(&self) -> Option<&'static str> {
                match *self {
                    $(
                    Self::$name => Some(stringify!($name)),
                    )*
                    _ => None
                }
            }

            pub fn description(&self) -> Option<&'static str> {
                match *self {
                    $(
                    Self::$name => Some($desc),
                    )*
                    _ => None
                }
            }
        }
    };
}

/// Data from:
/// /usr/include/asm-generic/errno-base.h
/// /usr/include/asm-generic/errno.h
/// (GPL-2.0 WITH Linux-syscall-note)
errno_values!(
    EPERM, "Operation not permitted";
    ENOENT, "No such file or directory";
    ESRCH, "No such process";
    EINTR, "Interrupted system call";
    EIO, "I/O error";
    ENXIO, "No such device or address";
    E2BIG, "Argument list too long";
    ENOEXEC, "Exec format error";
    EBADF, "Bad file number";
    ECHILD, "No child processes";
    EAGAIN, "Try again"; // Same as EWOULDBLOCK
    ENOMEM, "Out of memory";
    EACCES, "Permission denied";
    EFAULT, "Bad address";
    ENOTBLK, "Block device required";
    EBUSY, "Device or resource busy";
    EEXIST, "File exists";
    EXDEV, "Cross-device link";
    ENODEV, "No such device";
    ENOTDIR, "Not a directory";
    EISDIR, "Is a directory";
    EINVAL, "Invalid argument";
    ENFILE, "File table overflow";
    EMFILE, "Too many open files";
    ENOTTY, "Not a typewriter";
    ETXTBSY, "Text file busy";
    EFBIG, "File too large";
    ENOSPC, "No space left on device";
    ESPIPE, "Illegal seek";
    EROFS, "Read-only file system";
    EMLINK, "Too many links";
    EPIPE, "Broken pipe";
    EDOM, "Math argument out of domain of func";
    ERANGE, "Math result not representable";
    EDEADLOCK, "Resource deadlock would occur";
    ENAMETOOLONG, "File name too long";
    ENOLCK, "No record locks available";
    ENOSYS, "Invalid system call number";
    ENOTEMPTY, "Directory not empty";
    ELOOP, "Too many symbolic links encountered";
    ENOMSG, "No message of desired type";
    EIDRM, "Identifier removed";
    ECHRNG, "Channel number out of range";
    EL2NSYNC, "Level 2 not synchronized";
    EL3HLT, "Level 3 halted";
    EL3RST, "Level 3 reset";
    ELNRNG, "Link number out of range";
    EUNATCH, "Protocol driver not attached";
    ENOCSI, "No CSI structure available";
    EL2HLT, "Level 2 halted";
    EBADE, "Invalid exchange";
    EBADR, "Invalid request descriptor";
    EXFULL, "Exchange full";
    ENOANO, "No anode";
    EBADRQC, "Invalid request code";
    EBADSLT, "Invalid slot";
    EBFONT, "Bad font file format";
    ENOSTR, "Device not a stream";
    ENODATA, "No data available";
    ETIME, "Timer expired";
    ENOSR, "Out of streams resources";
    ENONET, "Machine is not on the network";
    ENOPKG, "Package not installed";
    EREMOTE, "Object is remote";
    ENOLINK, "Link has been severed";
    EADV, "Advertise error";
    ESRMNT, "Srmount error";
    ECOMM, "Communication error on send";
    EPROTO, "Protocol error";
    EMULTIHOP, "Multihop attempted";
    EDOTDOT, "RFS specific error";
    EBADMSG, "Not a data message";
    EOVERFLOW, "Value too large for defined data type";
    ENOTUNIQ, "Name not unique on network";
    EBADFD, "File descriptor in bad state";
    EREMCHG, "Remote address changed";
    ELIBACC, "Can not access a needed shared library";
    ELIBBAD, "Accessing a corrupted shared library";
    ELIBSCN, ".lib section in a.out corrupted";
    ELIBMAX, "Attempting to link in too many shared libraries";
    ELIBEXEC, "Cannot exec a shared library directly";
    EILSEQ, "Illegal byte sequence";
    ERESTART, "Interrupted system call should be restarted";
    ESTRPIPE, "Streams pipe error";
    EUSERS, "Too many users";
    ENOTSOCK, "Socket operation on non-socket";
    EDESTADDRREQ, "Destination address required";
    EMSGSIZE, "Message too long";
    EPROTOTYPE, "Protocol wrong type for socket";
    ENOPROTOOPT, "Protocol not available";
    EPROTONOSUPPORT, "Protocol not supported";
    ESOCKTNOSUPPORT, "Socket type not supported";
    EOPNOTSUPP, "Operation not supported on transport endpoint";
    EPFNOSUPPORT, "Protocol family not supported";
    EAFNOSUPPORT, "Address family not supported by protocol";
    EADDRINUSE, "Address already in use";
    EADDRNOTAVAIL, "Cannot assign requested address";
    ENETDOWN, "Network is down";
    ENETUNREACH, "Network is unreachable";
    ENETRESET, "Network dropped connection because of reset";
    ECONNABORTED, "Software caused connection abort";
    ECONNRESET, "Connection reset by peer";
    ENOBUFS, "No buffer space available";
    EISCONN, "Transport endpoint is already connected";
    ENOTCONN, "Transport endpoint is not connected";
    ESHUTDOWN, "Cannot send after transport endpoint shutdown";
    ETOOMANYREFS, "Too many references: cannot splice";
    ETIMEDOUT, "Connection timed out";
    ECONNREFUSED, "Connection refused";
    EHOSTDOWN, "Host is down";
    EHOSTUNREACH, "No route to host";
    EALREADY, "Operation already in progress";
    EINPROGRESS, "Operation now in progress";
    ESTALE, "Stale file handle";
    EUCLEAN, "Structure needs cleaning";
    ENOTNAM, "Not a XENIX named type file";
    ENAVAIL, "No XENIX semaphores available";
    EISNAM, "Is a named type file";
    EREMOTEIO, "Remote I/O error";
    EDQUOT, "Quota exceeded";
    ENOMEDIUM, "No medium found";
    EMEDIUMTYPE, "Wrong medium type";
    ECANCELED, "Operation Canceled";
    ENOKEY, "Required key not available";
    EKEYEXPIRED, "Key has expired";
    EKEYREVOKED, "Key has been revoked";
    EKEYREJECTED, "Key was rejected by service";
    EOWNERDEAD, "Owner died";
    ENOTRECOVERABLE, "State not recoverable";
    ERFKILL, "Operation not possible due to RF-kill";
    EHWPOISON, "Memory page has hardware error";
);
