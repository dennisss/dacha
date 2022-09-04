use peripherals::raw::ficr::FICR;
use peripherals::raw::nvmc::NVMC;
use peripherals::raw::register::{RegisterRead, RegisterWrite};
use peripherals::raw::uicr::{UICR, UICR_REGISTERS};

/// Start offset in flash of the bootloader code.
/// The bootloader code goes up to the BOOTLOADER_PARAMS.
pub const BOOTLOADER_OFFSET: u32 = 0;

/// Start offset in flash of the bootloader params page.
pub const BOOTLOADER_PARAMS_OFFSET: u32 = 28 * 1024;

/// Start offset in flash of application code.
/// The application code goes until the end of flash space.
pub const APPLICATION_CODE_OFFSET: u32 = 32 * 1024;

/// Expected value of a word of flash memory after an erase.
pub const FLASH_ERASED_WORD_VALUE: u32 = 0xFFFFFFFF;

/// Size of a single flash page. This is the smallest granularity which we can
/// erase.
///
/// This should normally return 4096.
pub fn flash_page_size() -> u32 {
    let ficr = unsafe { FICR::new() };
    ficr.codepagesize.read()
}

pub fn flash_size() -> u32 {
    let ficr = unsafe { FICR::new() };
    ficr.codepagesize.read() * ficr.codesize.read()
}

pub fn flash_page_start(addr: u32) -> u32 {
    let page_size = flash_page_size();
    (addr / page_size) * page_size
}

pub unsafe fn bootloader_params_data() -> &'static [u8] {
    core::slice::from_raw_parts(
        BOOTLOADER_PARAMS_OFFSET as *mut u8,
        flash_page_size() as usize,
    )
}

pub unsafe fn application_code_data() -> &'static [u8] {
    let len = flash_size() - APPLICATION_CODE_OFFSET;
    core::slice::from_raw_parts(APPLICATION_CODE_OFFSET as *mut u8, len as usize)
}

// TODO: Keep in sync with the linker script.
fn application_params_length() -> u32 {
    4 * flash_page_size()
}

pub unsafe fn application_params_data() -> &'static [u8] {
    let len = application_params_length();
    core::slice::from_raw_parts((flash_size() - len) as *mut u8, len as usize)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FlashSegment {
    BootloaderCode,
    BootloaderParams,
    ApplicationCode,
    ApplicationParams,
    UICR,
}

impl FlashSegment {
    /// Looks up the segment in which a given memory address is located. The
    /// address must be between the [start_address, end_address) of a segment to
    /// be considered inside of it.
    pub fn from_address(addr: u32) -> Option<Self> {
        let uicr_start_address =
            unsafe { core::mem::transmute::<&UICR_REGISTERS, u32>(&*UICR::new()) };
        let uicr_end_address = uicr_start_address + (core::mem::size_of::<UICR_REGISTERS>() as u32);

        let app_params = unsafe { application_params_data() };
        let app_params_start_address = unsafe { core::mem::transmute(app_params.as_ptr()) };

        if addr >= BOOTLOADER_OFFSET && addr < BOOTLOADER_PARAMS_OFFSET {
            Some(Self::BootloaderCode)
        } else if addr >= BOOTLOADER_PARAMS_OFFSET && addr < APPLICATION_CODE_OFFSET {
            Some(Self::BootloaderParams)
        } else if addr >= APPLICATION_CODE_OFFSET && addr < app_params_start_address {
            Some(Self::ApplicationCode)
        } else if addr >= app_params_start_address && addr < flash_size() {
            Some(Self::ApplicationParams)
        } else if addr >= uicr_start_address && addr < uicr_end_address {
            Some(Self::UICR)
        } else {
            None
        }
    }

    pub fn start_address(&self) -> u32 {
        match self {
            FlashSegment::BootloaderCode => BOOTLOADER_OFFSET,
            FlashSegment::BootloaderParams => BOOTLOADER_PARAMS_OFFSET,
            FlashSegment::ApplicationCode => APPLICATION_CODE_OFFSET,
            FlashSegment::ApplicationParams => flash_size() - application_params_length(),
            FlashSegment::UICR => unsafe {
                core::mem::transmute::<&UICR_REGISTERS, u32>(&*UICR::new())
            },
        }
    }
}

/// Writes word aligned bytes to flash. If the write starts mid way into a flash
/// page, we assume that it has already been erased.
///
/// TODO: We could make this asyncronous if we supported co-operatively sleeping
/// when we are in a busy loop.
pub fn write_to_flash(mut addr: u32, data: &[u8], nvmc: &mut NVMC) {
    const WORD_SIZE: usize = core::mem::size_of::<u32>();

    let page_size = flash_page_size();

    assert_no_debug!(addr % (WORD_SIZE as u32) == 0);
    assert_no_debug!(data.len() % WORD_SIZE == 0);

    let words = unsafe {
        core::slice::from_raw_parts::<u32>(
            core::mem::transmute(data.as_ptr()),
            data.len() / WORD_SIZE,
        )
    };

    for w in words {
        while nvmc.readynext.read().is_busy() {
            continue;
        }

        if addr % page_size == 0 {
            nvmc.config.write_with(|v| v.set_wen_with(|v| v.set_een()));
            nvmc.erasepage.write(addr);
            nvmc.config.write_with(|v| v.set_wen_with(|v| v.set_ren()));

            // NOTE: READYNEXT seems to sometimes not properly register when writes can
            // start occuring after an erase.
            while nvmc.ready.read().is_busy() {
                continue;
            }
        }

        // Double check that the page erase has completed. As accesses to flash while
        // flash is being modified will halt the CPU, this should block if we are still
        // erasing for some reason.
        assert_no_debug!(
            unsafe { core::ptr::read_volatile(addr as *mut u32) } == FLASH_ERASED_WORD_VALUE
        );

        nvmc.config.write_with(|v| v.set_wen_with(|v| v.set_wen()));
        unsafe { core::ptr::write_volatile(addr as *mut u32, *w) };
        nvmc.config.write_with(|v| v.set_wen_with(|v| v.set_ren()));

        assert_no_debug!(unsafe { core::ptr::read_volatile(addr as *mut u32) } == *w);

        addr += WORD_SIZE as u32;
    }

    // Wait for all writes to complete.
    while nvmc.ready.read().is_busy() {
        continue;
    }
}

/// Erases all data in the UICR registers.
/// NOTE: We do not wait for the erase to be complete. The assumption is that
/// the user will call write_to_uicr afterwards which will block if needed.
pub fn erase_uicr_async(nvmc: &mut NVMC) {
    while nvmc.readynext.read().is_busy() {
        continue;
    }

    nvmc.config.write_with(|v| v.set_wen_with(|v| v.set_een()));
    nvmc.eraseuicr.write_erase();
    nvmc.config.write_with(|v| v.set_wen_with(|v| v.set_ren()));
}

/// NOTE: This assumes that UICR has already been erased.
pub fn write_to_uicr(mut addr: u32, data: &[u8], nvmc: &mut NVMC) {
    const WORD_SIZE: usize = core::mem::size_of::<u32>();

    assert_no_debug!(addr % (WORD_SIZE as u32) == 0);
    assert_no_debug!(data.len() % WORD_SIZE == 0);

    let words = unsafe {
        core::slice::from_raw_parts::<u32>(
            core::mem::transmute(data.as_ptr()),
            data.len() / WORD_SIZE,
        )
    };

    for w in words {
        while nvmc.readynext.read().is_busy() {
            continue;
        }

        nvmc.config.write_with(|v| v.set_wen_with(|v| v.set_wen()));
        unsafe { core::ptr::write_volatile(addr as *mut u32, *w) };
        nvmc.config.write_with(|v| v.set_wen_with(|v| v.set_ren()));

        addr += WORD_SIZE as u32;
    }

    // Wait for all writes to complete.
    while nvmc.ready.read().is_busy() {
        continue;
    }
}
