const GPIO_P0: u32 = 0x50000000;

pub const GPIO_P0_OUT: *mut u32 = (GPIO_P0 + 0x504) as *mut u32;
pub const GPIO_P0_OUTSET: *mut u32 = (GPIO_P0 + 0x508) as *mut u32;
pub const GPIO_P0_OUTCLR: *mut u32 = (GPIO_P0 + 0x50C) as *mut u32;

pub const GPIO_P0_DIR: *mut u32 = (GPIO_P0 + 0x514) as *mut u32;

const CLOCK: u32 = 0x40000000;
pub const TASKS_LFCLKSTART: *mut u32 = (CLOCK + 0x008) as *mut u32;
pub const TASKS_LFCLKSTOP: *mut u32 = (CLOCK + 0x00C) as *mut u32;

pub const LFCLKRUN: *mut u32 = (CLOCK + 0x414) as *mut u32;
pub const LFCLKSTAT: *mut u32 = (CLOCK + 0x418) as *mut u32;
pub const LFCLKSRC: *mut u32 = (CLOCK + 0x518) as *mut u32;

// NOTE: The RTC is initially stopped.
// Peripheral id = 11
const RTC0: u32 = 0x4000B000;
pub const RTC0_TASKS_START: *mut u32 = (RTC0 + 0x000) as *mut u32;

pub const RTC0_EVENTS_TICK: *mut u32 = (RTC0 + 0x100) as *mut u32;
pub const RTC0_EVENTS_COMPARE0: *mut u32 = (RTC0 + 0x140) as *mut u32;

pub const RTC0_COUNTER: *mut u32 = (RTC0 + 0x504) as *mut u32;

pub const RTC0_PRESCALER: *mut u32 = (RTC0 + 0x508) as *mut u32;

pub const RTC0_INTENSET: *mut u32 = (RTC0 + 0x304) as *mut u32;
pub const RTC0_INTENCLR: *mut u32 = (RTC0 + 0x308) as *mut u32;

pub const RTC0_EVTEN: *mut u32 = (RTC0 + 0x340) as *mut u32;
pub const RTC0_EVTENSET: *mut u32 = (RTC0 + 0x344) as *mut u32;

pub const RTC0_CC0: *mut u32 = (RTC0 + 0x540) as *mut u32;

// System Control Space (SCS)
pub const NVIC_ICSR: *mut u32 = 0xE000ED04 as *mut u32;
pub const NVIC_ICTR: *mut u32 = 0xE000E004 as *mut u32;
pub const NVIC_ISER0: u32 = 0xE000E100;

const RADIO: u32 = 0x40001000;
pub const RADIO_TASKS_TXEN: *mut u32 = (RADIO + 0x000) as *mut u32;
pub const RADIO_TASKS_RXEN: *mut u32 = (RADIO + 0x004) as *mut u32;
pub const RADIO_TASKS_START: *mut u32 = (RADIO + 0x008) as *mut u32;
pub const RADIO_TASKS_STOP: *mut u32 = (RADIO + 0x00C) as *mut u32;
pub const RADIO_TASKS_DISABLE: *mut u32 = (RADIO + 0x010) as *mut u32;
pub const RADIO_TASKS_RSSISTART: *mut u32 = (RADIO + 0x014) as *mut u32;
pub const RADIO_TASKS_RSSISTOP: *mut u32 = (RADIO + 0x018) as *mut u32;
pub const RADIO_TASKS_BCSTART: *mut u32 = (RADIO + 0x01C) as *mut u32;
pub const RADIO_TASKS_BCSTOP: *mut u32 = (RADIO + 0x020) as *mut u32;
pub const RADIO_TASKS_EDSTART: *mut u32 = (RADIO + 0x024) as *mut u32;
pub const RADIO_TASKS_EDSTOP: *mut u32 = (RADIO + 0x028) as *mut u32;
pub const RADIO_TASKS_CCASTART: *mut u32 = (RADIO + 0x02C) as *mut u32;
pub const RADIO_TASKS_CCASTOP: *mut u32 = (RADIO + 0x030) as *mut u32;
pub const RADIO_EVENTS_READY: *mut u32 = (RADIO + 0x100) as *mut u32;
pub const RADIO_EVENTS_ADDRESS: *mut u32 = (RADIO + 0x104) as *mut u32;
pub const RADIO_EVENTS_PAYLOAD: *mut u32 = (RADIO + 0x108) as *mut u32;
pub const RADIO_EVENTS_END: *mut u32 = (RADIO + 0x10C) as *mut u32;
pub const RADIO_EVENTS_DISABLED: *mut u32 = (RADIO + 0x110) as *mut u32;
pub const RADIO_EVENTS_DEVMATCH: *mut u32 = (RADIO + 0x114) as *mut u32;
pub const RADIO_EVENTS_DEVMISS: *mut u32 = (RADIO + 0x118) as *mut u32;
pub const RADIO_EVENTS_RSSIEND: *mut u32 = (RADIO + 0x11C) as *mut u32;
pub const RADIO_EVENTS_BCMATCH: *mut u32 = (RADIO + 0x128) as *mut u32;
pub const RADIO_EVENTS_CRCOK: *mut u32 = (RADIO + 0x130) as *mut u32;
pub const RADIO_EVENTS_CRCERROR: *mut u32 = (RADIO + 0x134) as *mut u32;
pub const RADIO_EVENTS_FRAMESTART: *mut u32 = (RADIO + 0x138) as *mut u32;
pub const RADIO_EVENTS_EDEND: *mut u32 = (RADIO + 0x13C) as *mut u32;
pub const RADIO_EVENTS_EDSTOPPED: *mut u32 = (RADIO + 0x140) as *mut u32;
pub const RADIO_EVENTS_CCAIDLE: *mut u32 = (RADIO + 0x144) as *mut u32;
pub const RADIO_EVENTS_CCABUSY: *mut u32 = (RADIO + 0x148) as *mut u32;
pub const RADIO_EVENTS_CCASTOPPED: *mut u32 = (RADIO + 0x14C) as *mut u32;
pub const RADIO_EVENTS_RATEBOOST: *mut u32 = (RADIO + 0x150) as *mut u32;
pub const RADIO_EVENTS_TXREADY: *mut u32 = (RADIO + 0x154) as *mut u32;
pub const RADIO_EVENTS_RXREADY: *mut u32 = (RADIO + 0x158) as *mut u32;
pub const RADIO_EVENTS_MHRMATCH: *mut u32 = (RADIO + 0x15C) as *mut u32;
pub const RADIO_EVENTS_PHYEND: *mut u32 = (RADIO + 0x16C) as *mut u32;
pub const RADIO_SHORTS: *mut u32 = (RADIO + 0x200) as *mut u32;
pub const RADIO_INTENSET: *mut u32 = (RADIO + 0x304) as *mut u32;
pub const RADIO_INTENCLR: *mut u32 = (RADIO + 0x308) as *mut u32;
pub const RADIO_CRCSTATUS: *mut u32 = (RADIO + 0x400) as *mut u32;
pub const RADIO_RXMATCH: *mut u32 = (RADIO + 0x408) as *mut u32;
pub const RADIO_RXCRC: *mut u32 = (RADIO + 0x40C) as *mut u32;
pub const RADIO_DAI: *mut u32 = (RADIO + 0x410) as *mut u32;
pub const RADIO_PDUSTAT: *mut u32 = (RADIO + 0x414) as *mut u32;
pub const RADIO_PACKETPTR: *mut u32 = (RADIO + 0x504) as *mut u32;
pub const RADIO_FREQUENCY: *mut u32 = (RADIO + 0x508) as *mut u32;
pub const RADIO_TXPOWER: *mut u32 = (RADIO + 0x50C) as *mut u32;
pub const RADIO_MODE: *mut u32 = (RADIO + 0x510) as *mut u32;
pub const RADIO_PCNF0: *mut u32 = (RADIO + 0x514) as *mut u32;
pub const RADIO_PCNF1: *mut u32 = (RADIO + 0x518) as *mut u32;
pub const RADIO_BASE0: *mut u32 = (RADIO + 0x51C) as *mut u32;
pub const RADIO_BASE1: *mut u32 = (RADIO + 0x520) as *mut u32;
pub const RADIO_PREFIX0: *mut u32 = (RADIO + 0x524) as *mut u32;
pub const RADIO_PREFIX1: *mut u32 = (RADIO + 0x528) as *mut u32;
pub const RADIO_TXADDRESS: *mut u32 = (RADIO + 0x52C) as *mut u32;
pub const RADIO_RXADDRESSES: *mut u32 = (RADIO + 0x530) as *mut u32;
pub const RADIO_CRCCNF: *mut u32 = (RADIO + 0x534) as *mut u32;
pub const RADIO_CRCPOLY: *mut u32 = (RADIO + 0x538) as *mut u32;
pub const RADIO_CRCINIT: *mut u32 = (RADIO + 0x53C) as *mut u32;
pub const RADIO_TIFS: *mut u32 = (RADIO + 0x544) as *mut u32;
pub const RADIO_RSSISAMPLE: *mut u32 = (RADIO + 0x548) as *mut u32;
pub const RADIO_STATE: *mut u32 = (RADIO + 0x550) as *mut u32;
pub const RADIO_DATAWHITEIV: *mut u32 = (RADIO + 0x554) as *mut u32;
pub const RADIO_BCC: *mut u32 = (RADIO + 0x560) as *mut u32;
pub const RADIO_DAB_0: *mut u32 = (RADIO + 0x600) as *mut u32;
pub const RADIO_DAB_1: *mut u32 = (RADIO + 0x604) as *mut u32;
pub const RADIO_DAB_2: *mut u32 = (RADIO + 0x608) as *mut u32;
pub const RADIO_DAB_3: *mut u32 = (RADIO + 0x60C) as *mut u32;
pub const RADIO_DAB_4: *mut u32 = (RADIO + 0x610) as *mut u32;
pub const RADIO_DAB_5: *mut u32 = (RADIO + 0x614) as *mut u32;
pub const RADIO_DAB_6: *mut u32 = (RADIO + 0x618) as *mut u32;
pub const RADIO_DAB_7: *mut u32 = (RADIO + 0x61C) as *mut u32;
pub const RADIO_DAP_0: *mut u32 = (RADIO + 0x620) as *mut u32;
pub const RADIO_DAP_1: *mut u32 = (RADIO + 0x624) as *mut u32;
pub const RADIO_DAP_2: *mut u32 = (RADIO + 0x628) as *mut u32;
pub const RADIO_DAP_3: *mut u32 = (RADIO + 0x62C) as *mut u32;
pub const RADIO_DAP_4: *mut u32 = (RADIO + 0x630) as *mut u32;
pub const RADIO_DAP_5: *mut u32 = (RADIO + 0x634) as *mut u32;
pub const RADIO_DAP_6: *mut u32 = (RADIO + 0x638) as *mut u32;
pub const RADIO_DAP_7: *mut u32 = (RADIO + 0x63C) as *mut u32;
pub const RADIO_DACNF: *mut u32 = (RADIO + 0x640) as *mut u32;
pub const RADIO_MHRMATCHCONF: *mut u32 = (RADIO + 0x644) as *mut u32;
pub const RADIO_MHRMATCHMAS: *mut u32 = (RADIO + 0x648) as *mut u32;
pub const RADIO_MODECNF0: *mut u32 = (RADIO + 0x650) as *mut u32;
pub const RADIO_SFD: *mut u32 = (RADIO + 0x660) as *mut u32;
pub const RADIO_EDCNT: *mut u32 = (RADIO + 0x664) as *mut u32;
pub const RADIO_EDSAMPLE: *mut u32 = (RADIO + 0x668) as *mut u32;
pub const RADIO_CCACTRL: *mut u32 = (RADIO + 0x66C) as *mut u32;
pub const RADIO_POWER: *mut u32 = (RADIO + 0xFFC) as *mut u32;
