use peripherals::raw::ficr::FICR;
use peripherals::raw::nvmc::NVMC;
use peripherals::raw::register::{RegisterRead, RegisterWrite};

/// Start offset in flash of the bootloader code.
/// The bootloader code goes up to the BOOTLOADER_PARAMS.
pub const BOOTLOADER_OFFSET: u32 = 0;

/// Start offset in flash of the bootloader params page.
pub const BOOTLOADER_PARAMS_OFFSET: u32 = 28 * 1024;

/// Start offset in flash of application code.
/// The application code goes until the end of flash space.
pub const APPLICATION_CODE_OFFSET: u32 = 32 * 1024;

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

/// Writes word aligned bytes to flash. If the write starts mid way into a flash
/// page, we assume that it has already been erased.
pub fn write_to_flash(mut addr: u32, data: &[u8], nvmc: &mut NVMC) {
    const WORD_SIZE: usize = core::mem::size_of::<u32>();

    let page_size = flash_page_size();

    assert!(addr % (WORD_SIZE as u32) == 0);
    assert!(data.len() % WORD_SIZE == 0);

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

            while nvmc.readynext.read().is_busy() {
                continue;
            }
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
