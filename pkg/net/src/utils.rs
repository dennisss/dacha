use common::errors::*;
use sys::OpenFileDescriptor;

pub(crate) unsafe fn set_reuse_port(fd: &OpenFileDescriptor, on: bool) -> Result<()> {
    let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::SOL_SOCKET,
        sys::SocketOption::SO_REUSEPORT,
        &value,
    )?;

    Ok(())
}

pub(crate) unsafe fn set_reuse_addr(fd: &OpenFileDescriptor, on: bool) -> Result<()> {
    let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::SOL_SOCKET,
        sys::SocketOption::SO_REUSEADDR,
        &value,
    )?;

    Ok(())
}

pub unsafe fn set_tcp_nodelay(fd: &OpenFileDescriptor, on: bool) -> Result<()> {
    let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::IPPROTO_TCP,
        sys::SocketOption::TCP_NODELAY,
        &value,
    )?;

    Ok(())
}

pub(crate) unsafe fn set_broadcast(fd: &OpenFileDescriptor, on: bool) -> Result<()> {
    let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::SOL_SOCKET,
        sys::SocketOption::SO_BROADCAST,
        &value,
    )?;

    Ok(())
}

pub unsafe fn set_bind_to_device(fd: &OpenFileDescriptor, dev_name: &str) -> Result<()> {
    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::SOL_SOCKET,
        sys::SocketOption::SO_BINDTODEVICE,
        dev_name.as_bytes()
    )
    .map_err(|e| format_err!("While running SO_BINDTODEVICE to {}: {}", dev_name, e))?;

    Ok(())
}

pub unsafe fn enable_hardware_timestamping(fd: &OpenFileDescriptor, dev_name: &str) -> Result<()> {
    // TODO: Set custom ids for SOF_TIMESTAMPING_OPT_ID 
    let value = ((
        (sys::bindings::SOF_TIMESTAMPING_TX_HARDWARE as u32) |
        (sys::bindings::SOF_TIMESTAMPING_RX_HARDWARE as u32) |
        (sys::bindings::SOF_TIMESTAMPING_RAW_HARDWARE as u32) |
        (sys::bindings::SOF_TIMESTAMPING_OPT_TSONLY as u32) |
        (sys::bindings::SOF_TIMESTAMPING_OPT_ID as u32)
    ) as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::SOL_SOCKET,
        sys::SocketOption::SO_TIMESTAMPING_NEW,
        &value
    )
    .map_err(|e| format_err!("While setting SO_TIMESTAMPING_NEW: {}", e))?;

    let mut req = sys::bindings::ifreq::default();
    req.ifr_ifrn.ifrn_name[0..dev_name.as_bytes().len()].copy_from_slice(core::mem::transmute(dev_name.as_bytes()));

    // TODO: Pin this.
    let mut config = alloc::boxed::Box::new(sys::bindings::hwtstamp_config::default());
    req.ifr_ifru.ifru_data = core::mem::transmute::<&sys::bindings::hwtstamp_config, _>(&config);

    // TODO: Figure out why this breaks on some of my Pis.
    /*
    sys::ioctl(**fd, sys::bindings::SIOCGHWTSTAMP, core::mem::transmute(&mut req))
    .map_err(|e| format_err!("While running SIOCGHWTSTAMP: {}", e))?;

    if config.tx_type == (sys::bindings::hwtstamp_tx_types::HWTSTAMP_TX_OFF as i32) ||
        config.rx_filter == (sys::bindings::hwtstamp_rx_filters::HWTSTAMP_FILTER_NONE as i32) {
        return Err(err_msg("Timestamping not configured on the ethernet interface"));
    }
    */

    Ok(())
}

/// Configures a network interface to have packet timestamping filters enabled.
/// Note that this is global and requires CAP_NET_ADMIN in the global namespace.
pub fn enable_hardware_timestamp_filters(dev_name: &str) -> Result<()> {
    unsafe { 
        // Open an arbitrary socket.
        let fd = sys::socket(
            sys::AddressFamily::AF_INET,
            sys::SocketType::SOCK_DGRAM,
            sys::SocketFlags::SOCK_CLOEXEC,
            sys::SocketProtocol::UDP,
        )?;

        let mut req = alloc::boxed::Box::new(sys::bindings::ifreq::default());
        req.ifr_ifrn.ifrn_name[0..dev_name.as_bytes().len()].copy_from_slice(core::mem::transmute(dev_name.as_bytes()));

        // TODO: Pin this.
        let mut config = alloc::boxed::Box::new(sys::bindings::hwtstamp_config::default());
        config.tx_type = sys::bindings::hwtstamp_tx_types::HWTSTAMP_TX_ON as i32;
        req.ifr_ifru.ifru_data = core::mem::transmute::<&sys::bindings::hwtstamp_config, _>(&config);

        let mut last_error = Ok(0);

        for filter in [
            // Works on most computers and Pi 5 (using RP1 PTP clock)
            sys::bindings::hwtstamp_rx_filters::HWTSTAMP_FILTER_ALL as i32,

            // Works on the CM5 using the Broadcom PHY
            sys::bindings::hwtstamp_rx_filters::HWTSTAMP_FILTER_PTP_V2_EVENT as i32,
        ] {
            config.rx_filter = filter;

            last_error = sys::ioctl(*fd, sys::bindings::SIOCSHWTSTAMP, core::mem::transmute::<&sys::bindings::ifreq, _>(&req));

            if last_error.is_ok() {
                break;
            }
        }

        last_error?;

        Ok(())
    }
}

