use sys::bindings::*;
use sys::{iow, ior, iowr, c_int};

const PTP_CLK_MAGIC: u8 = b'=';

ior!(ptp_clock_getcaps, PTP_CLK_MAGIC, 1, ptp_clock_caps);
iow!(ptp_extts_request, PTP_CLK_MAGIC, 2, ptp_extts_request);
iow!(ptp_perout_request, PTP_CLK_MAGIC, 3, ptp_perout_request);
iow!(ptp_enable_pps, PTP_CLK_MAGIC, 4, c_int);
iow!(ptp_sys_offset, PTP_CLK_MAGIC, 5, ptp_sys_offset);
iowr!(ptp_pin_getfunc, PTP_CLK_MAGIC, 6, ptp_pin_desc);
iow!(ptp_pin_setfunc, PTP_CLK_MAGIC, 7, ptp_pin_desc);
iowr!(ptp_sys_offset_precise, PTP_CLK_MAGIC, 8, ptp_sys_offset_precise);
iowr!(ptp_sys_offset_extended, PTP_CLK_MAGIC, 9, ptp_sys_offset_extended);

ior!(ptp_clock_getcaps2, PTP_CLK_MAGIC, 10, ptp_clock_caps);
iow!(ptp_extts_request2, PTP_CLK_MAGIC, 11, ptp_extts_request);
iow!(ptp_perout_request2, PTP_CLK_MAGIC, 12, ptp_perout_request);
iow!(ptp_enable_pps2, PTP_CLK_MAGIC, 13, c_int);
iow!(ptp_sys_offset2, PTP_CLK_MAGIC, 14, ptp_sys_offset);
iowr!(ptp_pin_getfunc2, PTP_CLK_MAGIC, 15, ptp_pin_desc);
iow!(ptp_pin_setfunc2, PTP_CLK_MAGIC, 16, ptp_pin_desc);
iowr!(ptp_sys_offset_precise2, PTP_CLK_MAGIC, 17, ptp_sys_offset_precise);
iowr!(ptp_sys_offset_extended2, PTP_CLK_MAGIC, 18, ptp_sys_offset_extended);
