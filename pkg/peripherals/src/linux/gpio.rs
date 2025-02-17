use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::sync::Arc;

use base_util::null_terminated::read_null_terminated_string;
use common::errors::*;
use file::LocalPath;
pub use sys::bindings::gpio_v2_line_flag;
use sys::bindings::{
    gpio_v2_line_config, gpio_v2_line_info, gpio_v2_line_request, gpio_v2_line_values,
    gpiochip_info,
};
use sys::OpenFileDescriptor;

ior!(gpio_get_chipinfo, 0xB4, 0x01, gpiochip_info);
iowr!(gpio_get_lineinfo_unwatch, 0xB4, 0x0C, u32);
iowr!(gpio_v2_get_lineinfo, 0xB4, 0x05, gpio_v2_line_info);
iowr!(gpio_v2_get_lineinfo_watch, 0xB4, 0x06, gpio_v2_line_info);
iowr!(gpio_v2_get_line, 0xB4, 0x07, gpio_v2_line_request);
iowr!(gpio_v2_line_set_config, 0xB4, 0x0D, gpio_v2_line_config);
iowr!(gpio_v2_line_get_values, 0xB4, 0x0E, gpio_v2_line_values);
iowr!(gpio_v2_line_set_values, 0xB4, 0x0F, gpio_v2_line_values);

const DEFAULT_CHIP_LABELS: &'static [&'static str] = &[
    // Raspberry Pi 1-3
    "pinctrl-bcm2835",
    // Raspberry Pi 4
    "pinctrl-bcm2711",
    // Raspberry Pi 5
    "pinctrl-rp1",
];

pub struct GPIOChip {
    /// Descritor for the '/dev/gpiochip*' file
    chip_file: Arc<file::LocalFile>,
}

impl GPIOChip {
    pub fn list() -> Result<Vec<Self>> {
        let mut out = vec![];

        for entry in file::read_dir("/dev/")? {
            if !entry.name().starts_with("gpiochip") {
                continue;
            }

            let path = LocalPath::new("/dev").join(entry.name());

            out.push(Self::open(&path)?);
        }

        Ok(out)
    }

    pub fn default_chip() -> Result<Self> {
        for chip in GPIOChip::list()? {
            if DEFAULT_CHIP_LABELS.contains(&chip.info()?.label.as_str()) {
                return Ok(chip);
            }
        }

        Err(err_msg("No suitable default GPIO chip found"))
    }

    /// Path should be of the form '/dev/gpiochip*'
    pub fn open(path: &LocalPath) -> Result<Self> {
        let chip_file = file::LocalFile::open_with_options(
            path,
            file::LocalFileOpenOptions::new().write(true).read(true),
        )?;

        Ok(Self {
            chip_file: Arc::new(chip_file),
        })
    }

    pub fn info(&self) -> Result<GPIOChipInfo> {
        let mut raw = gpiochip_info::default();
        unsafe {
            gpio_get_chipinfo(self.chip_file.as_raw_fd(), &mut raw)?;
        }

        Ok(GPIOChipInfo {
            name: read_null_terminated_string(&raw.name[..])?,
            label: read_null_terminated_string(&raw.label[..])?,
            lines: raw.lines,
        })
    }

    pub fn line_info(&self, index: u32) -> Result<GPIOLineInfo> {
        let mut raw = gpio_v2_line_info::default();
        raw.offset = index;
        unsafe {
            gpio_v2_get_lineinfo(self.chip_file.as_raw_fd(), &mut raw)?;
        }

        let mut attrs = vec![];
        for i in 0..raw.num_attrs {
            attrs.push(GPIOLineAttribute {});
        }

        Ok(GPIOLineInfo {
            name: read_null_terminated_string(&raw.name[..])?,
            consumer: read_null_terminated_string(&raw.consumer[..])?,
            flags: raw.flags,
            attrs,
        })
    }

    pub fn pin(&self, index: u32) -> Result<GPIOPin> {
        let mut req = gpio_v2_line_request::default();
        req.offsets[0] = index;
        req.num_lines = 1;
        req.event_buffer_size = 0;

        req.config.flags = gpio_v2_line_flag::GPIO_V2_LINE_FLAG_INPUT as u64;

        unsafe {
            gpio_v2_get_line(self.chip_file.as_raw_fd(), &mut req)?;
        }

        if req.fd <= 0 {
            return Err(err_msg("Failed to allocate line fd"));
        }

        // TODO: Set CLO_EXEC flag.
        let request_fd = OpenFileDescriptor::new(req.fd);

        Ok(GPIOPin {
            chip_file: self.chip_file.clone(),
            request_fd,
            config: req.config.clone(),
        })
    }
}

pub struct GPIOPin {
    chip_file: Arc<file::LocalFile>,
    request_fd: OpenFileDescriptor,
    config: gpio_v2_line_config,
}

impl GPIOPin {
    pub fn configure(&mut self, flags: GPIOLineFlags) -> Result<()> {
        self.config.flags = flags.to_raw();
        unsafe {
            gpio_v2_line_set_config(*self.request_fd, &mut self.config)?;
        }

        Ok(())
    }

    pub fn write(&mut self, high: bool) -> Result<()> {
        let mut raw = gpio_v2_line_values::default();
        raw.mask = 1;
        raw.bits = if high { 1 } else { 0 };

        unsafe {
            gpio_v2_line_set_values(*self.request_fd, &mut raw)?;
        }

        Ok(())
    }
}

define_bit_flags!(GPIOLineFlags u64 {
    INPUT = (gpio_v2_line_flag::GPIO_V2_LINE_FLAG_INPUT as u64),
    OUTPUT = (gpio_v2_line_flag::GPIO_V2_LINE_FLAG_OUTPUT as u64)
});

#[derive(Clone, Debug)]
pub struct GPIOChipInfo {
    pub name: String,
    pub label: String,
    pub lines: u32,
}

#[derive(Clone, Debug)]
pub struct GPIOLineInfo {
    pub name: String,
    pub consumer: String,
    pub flags: u64, // gpio_v2_line_flag,
    pub attrs: Vec<GPIOLineAttribute>,
}

#[derive(Clone, Debug)]
pub struct GPIOLineAttribute {}
