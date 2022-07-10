/*
da build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840_bootloader

openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program built/pkg/nordic/nordic_bootloader verify" -c reset -c exit

- `~/apps/gcc-arm-none-eabi-10.3-2021.10/bin/arm-none-eabi-gdb /home/dennis/workspace/dacha/built-rust/bfd75a5982e33698/thumbv7em-none-eabihf/release/nordic_bootloader`
- `target extended-remote localhost:3333`
- `monitor reset halt`

Notes:
- The UF2 will present addresses in strictly increasing order and the data must align to 32 bits

TODOs:
- Support partially reading requests so that we can stale them.

We will implement USB DFU
- https://www.usb.org/sites/default/files/DFU_1.1.pdf


0b10101


What this needs to do:
- Check for whether or not the user button is pressed down
- If so, start USB thread and wait for commands

- When ready to execute binary
    - Protect flash memory.
    - Change interrupt table location.
    - Reset stack pointer (use the stack pointer in the table)
    - Jump to the first thing in the new vector table
-


When is the bootloader entered:
- Check the RESETREAS register to see if we were reset via a pin or software
    - Also clear this register as it is cumulative.

TODO:
- See https://devzone.nordicsemi.com/f/nordic-q-a/65099/nreset-on-nrf52840-shortened-to-gnd-ground-is-it-possible-to-map-the-nreset-to-another-pin-and-execute-any-pin-mapping-while-p0-18-is-stuck-low
- Normally the nRESET pin is not mapped but it is mapped usually the first time the board is programmed or user code runs
- It would be interesting to replicate this behavior.
- This also means that we could get another pin if we wanted one.

https://devzone.nordicsemi.com/f/nordic-q-a/50722/nrf52832-can-nreset-be-programmed-to-any-gpio-using-pselreset-0-pselreset-1-registers

*/

#![feature(
    lang_items,
    type_alias_impl_trait,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types
)]
#![no_std]
#![no_main]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate executor;
extern crate peripherals;
#[macro_use]
extern crate common;
#[macro_use]
extern crate nordic;
#[macro_use]
extern crate macros;
extern crate uf2;

use core::arch::asm;
use core::future::Future;
use core::ptr::read_volatile;

use common::fixed::vec::FixedVec;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
use nordic::bootloader::flash::*;
use nordic::bootloader::params::*;
use nordic::config_storage::NetworkConfigStorage;
use nordic::gpio::*;
use nordic::log;
use nordic::reset::*;
use nordic::timer::Timer;
use nordic::uarte::UARTE;
use nordic::usb::aligned::Aligned;
use nordic::usb::controller::USBDeviceControlRequest;
use nordic::usb::controller::{
    USBDeviceControlResponse, USBDeviceController, USBDeviceNormalRequest,
};
use nordic::usb::default_handler::USBDeviceDefaultHandler;
use nordic::usb::handler::{USBDeviceHandler, USBError};
use nordic_proto::proto::bootloader::*;
use nordic_proto::usb_descriptors::*;
use peripherals::raw::nvic::NVIC_VTOR;
use peripherals::raw::nvmc::NVMC;
use peripherals::raw::power::resetreas::RESETREAS_VALUE;
use peripherals::raw::register::RegisterRead;
use peripherals::raw::register::RegisterWrite;
use protobuf::Message;
use uf2::*;
use usb::descriptors::SetupPacket;
use usb::dfu::*;

pub struct BootloaderUSBHandler {
    nvmc: NVMC,

    timer: Timer,

    /// Current value of the BootloaderParams proto loaded from flash.
    params: BootloaderParams,

    /// Status of the last command performed.
    status_code: DFUStatusCode,

    state: State,

    /// NOTE: The size of this must match the wTransferSize in the descriptor
    /// Alignment of this is required because we cast it to a UF2Block.
    buffer: Aligned<[u8; UF2_BLOCK_SIZE], u32>,
}

enum State {
    Idle,
    Downloading(DownloadingState),

    /// We've just received a zero length DFU_DNLOAD and we are done writing to
    /// flash. Upon getting the next DFU_GETSTATUS, we will reset to the
    /// application.
    Manifesting,
}

impl State {
    fn to_dfu_state(&self) -> DFUState {
        match self {
            State::Idle => DFUState::dfuIDLE,
            State::Downloading(_) => DFUState::dfuDNLOAD_IDLE,
            State::Manifesting => DFUState::dfuMANIFEST,
        }
    }
}

#[derive(Clone)]
struct DownloadingState {
    /// Position immediately after the last flash position to which we have
    /// written.
    next_flash_offset: u32,

    /// Next UF2 block number expected. This is also the next expected wBlockNum
    /// DFU number when truncated to 16 bits.
    next_block_number: u32,

    application_hasher: CRC32CHasher,

    application_size: u32,

    total_written: u32,
}

// TODO: Have a macro to auto-generate this.
impl USBDeviceHandler for BootloaderUSBHandler {
    type HandleControlRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleControlResponseFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalResponseAcknowledgedFuture<'a> =
        impl Future<Output = Result<(), USBError>> + 'a;

    fn handle_control_request<'a>(
        &'a mut self,
        setup: SetupPacket,
        req: USBDeviceControlRequest<'a>,
    ) -> Self::HandleControlRequestFuture<'a> {
        self.handle_control_request_impl(setup, req)
    }

    fn handle_control_response<'a>(
        &'a mut self,
        setup: SetupPacket,
        res: USBDeviceControlResponse<'a>,
    ) -> Self::HandleControlResponseFuture<'a> {
        self.handle_control_response_impl(setup, res)
    }

    fn handle_normal_request<'a>(
        &'a mut self,
        endpoint_index: usize,
        req: USBDeviceNormalRequest,
    ) -> Self::HandleNormalRequestFuture<'a> {
        async move { Ok(()) }
    }

    fn handle_normal_response_acknowledged<'a>(
        &'a mut self,
        endpoint_index: usize,
    ) -> Self::HandleNormalResponseAcknowledgedFuture<'a> {
        async move { Ok(()) }
    }
}

impl BootloaderUSBHandler {
    pub fn new(params: BootloaderParams, nvmc: NVMC, timer: Timer) -> Self {
        Self {
            nvmc,
            timer,
            params,
            status_code: DFUStatusCode::OK,
            state: State::Idle,
            buffer: Aligned::new([0u8; UF2_BLOCK_SIZE]),
        }
    }

    async fn handle_control_request_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut req: USBDeviceControlRequest<'a>,
    ) -> Result<(), USBError> {
        if setup.bmRequestType == 0b00100001 /* Host-to-device | Class | Interface */
            && setup.wIndex == get_attr!(&BOOTLOADER_USB_DESCRIPTORS, usb::dfu::DFUInterfaceNumberTag) as u16
        {
            if setup.bRequest == DFURequestType::DFU_ABORT as u8 {
                self.status_code = DFUStatusCode::OK;
                self.state = State::Idle;
                req.read(&mut []).await?;
                return Ok(());
            } else if setup.bRequest == DFURequestType::DFU_DETACH as u8 {
                self.status_code = DFUStatusCode::OK;
                req.read(&mut []).await?;

                // TODO: Debug this with the Manifestation code.

                // Give the application enough time to notice the response.
                self.timer.wait_ms(10).await;

                nordic::reset::reset_to_application();

                return Ok(());
            } else if setup.bRequest == DFURequestType::DFU_CLRSTATUS as u8 {
                self.status_code = DFUStatusCode::OK;
                req.read(&mut []).await?;
                return Ok(());
            } else if setup.bRequest == DFURequestType::DFU_DNLOAD as u8 {
                if let State::Idle = &self.state {
                    self.state = State::Downloading(DownloadingState {
                        next_flash_offset: 0,
                        next_block_number: 0,
                        application_hasher: CRC32CHasher::new(),
                        application_size: 0,
                        total_written: 0,
                    });
                }

                let state = match &mut self.state {
                    State::Downloading(s) => s,
                    _ => {
                        log!(b"DFU_DNLOAD: Wrong state\n");
                        self.status_code = DFUStatusCode::errSTALLEDPKT;
                        req.stale();
                        return Ok(());
                    }
                };

                let nread = req.read(&mut *self.buffer).await?;
                if nread == 0 {
                    // Enter manifestation mode. We already wrote the flash in previous requests so
                    // just reset.

                    if state.total_written != 0 {
                        if state.application_size != 0 {
                            self.params.set_application_size(state.application_size);
                            self.params
                                .set_application_crc32c(state.application_hasher.finish_u32());
                        }

                        self.params.set_num_flashes(self.params.num_flashes() + 1);
                        write_bootloader_params(&self.params, &mut self.nvmc);
                    }

                    self.status_code = DFUStatusCode::OK;
                    self.state = State::Manifesting;
                    return Ok(());
                }

                let block = match UF2Block::cast_from(&self.buffer[0..nread]) {
                    Some(v) => v,
                    None => {
                        log!(b"DFU_DNLOAD: Bad UF2 block\n");
                        self.status_code = DFUStatusCode::errSTALLEDPKT;
                        return Ok(());
                    }
                };

                let dfu_block_num = setup.wValue;
                if state.next_block_number as u16 != dfu_block_num
                    || state.next_block_number != block.block_number
                {
                    log!(b"DFU_DNLOAD: Non-monotonic block num\n");
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                if block.flags != UF2Flags::empty() {
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                // NOTE: We don't care about the num_blocks value in the UF2.
                // TODO: Prevent this from overflowing.
                state.next_block_number += 1;

                // TODO: Validate the block's family_id if it is present.

                // Writes must only go forward in flash addresses to ensure that we properly
                // handle erases.
                if block.target_addr < state.next_flash_offset {
                    log!(b"DFU_DNLOAD: Non-monotonic addr\n");
                    self.status_code = DFUStatusCode::errADDRESS;
                    return Ok(());
                }

                // We are only allowed to write full words at word offsets.
                if block.target_addr % 4 != 0 || block.payload_size % 4 != 0 {
                    log!(b"DFU_DNLOAD: Unaligned write\n");
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                let mut in_application_code = false;

                // Validate that the target address is in a range that is ok to write.
                // This also needs to update our state to enter the current segment being
                // written.
                if block.target_addr >= BOOTLOADER_OFFSET
                    && block.target_addr < BOOTLOADER_PARAMS_OFFSET
                {
                    // Writing to bootloader code

                    // TODO: Require a special flag to be flipped if we attempt to overwrite the
                    // bootloader itself

                    // Must always start writing to the vector table of the bootloader.
                    if state.next_flash_offset == 0 && block.target_addr != 0 {
                        log!(b"DFU_DNLOAD: Missing bootloader start\n");
                        self.status_code = DFUStatusCode::errADDRESS;
                        return Ok(());
                    }
                } else if block.target_addr >= APPLICATION_CODE_OFFSET
                    && block.target_addr < flash_size()
                {
                    // Writing to application code.
                    in_application_code = true;

                    if state.next_flash_offset < APPLICATION_CODE_OFFSET {
                        // Must always write the first bytes of the application (doesn't make sense
                        // to have an application without a vector table).
                        if block.target_addr != APPLICATION_CODE_OFFSET {
                            log!(b"DFU_DNLOAD: Missing app start\n");
                            self.status_code = DFUStatusCode::errADDRESS;
                            return Ok(());
                        }

                        // Advance forward our flash offset to this segment.
                        // Don't need to erase any partially completed pages in previous segments.
                        state.next_flash_offset = APPLICATION_CODE_OFFSET;
                    }
                } else {
                    // Not a supported flash segment.
                    // Note that we don't support writing to the
                    // BOOTLOADER_PARAMS segment directly.

                    log!(b"DFU_DNLOAD: Unsupported addr\n");
                    self.status_code = DFUStatusCode::errADDRESS;
                    return Ok(());
                }

                // TODO: Implement UICR writing, but only if the bootloader is being written.

                // Explicitly write all flash space between our last written offset and the next
                // one. This has the following purposes:
                // - Ensures that if block.target_addr is midway through a page, the page is
                //   erased.
                // - For application code, ensures that we CRC a contiguous segment of code with
                //   deterministic zero padding for undefined regions.
                let mut empty_word = [0u8; 4];
                while state.next_flash_offset < block.target_addr {
                    write_to_flash(state.next_flash_offset, &empty_word, &mut self.nvmc);
                    state.next_flash_offset += empty_word.len() as u32;

                    state.total_written += empty_word.len() as u32;
                    if in_application_code {
                        state.application_hasher.update(&empty_word);
                        state.application_size += empty_word.len() as u32;
                    }
                }
                assert!(state.next_flash_offset == block.target_addr);

                write_to_flash(block.target_addr, block.payload(), &mut self.nvmc);
                state.next_flash_offset = block.target_addr + block.payload_size;

                state.total_written += block.payload_size;
                if in_application_code {
                    state.application_hasher.update(block.payload());
                    state.application_size += block.payload_size;
                }

                // log!(b"Done block\n");

                return Ok(());
            }
        }

        USBDeviceDefaultHandler::new(BOOTLOADER_USB_DESCRIPTORS)
            .handle_control_request(setup, req)
            .await
    }

    async fn handle_control_response_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut res: USBDeviceControlResponse<'a>,
    ) -> Result<(), USBError> {
        if setup.bmRequestType == 0b10100001
            && setup.wIndex
                == get_attr!(&BOOTLOADER_USB_DESCRIPTORS, usb::dfu::DFUInterfaceNumberTag) as u16
        {
            if setup.bRequest == DFURequestType::DFU_GETSTATUS as u8 {
                let status = DFUStatus {
                    bStatus: self.status_code,
                    bwPollTimeout: [0u8; 3], // TODO u24
                    bState: self.state.to_dfu_state(),
                    iString: 0,
                };

                res.write(unsafe {
                    core::slice::from_raw_parts(
                        core::mem::transmute(&status),
                        core::mem::size_of::<DFUStatus>(),
                    )
                })
                .await;

                if let State::Manifesting = &self.state {
                    // Give the application enough time to notice the response.
                    self.timer.wait_ms(10).await;

                    nordic::reset::reset_to_application();
                }

                return Ok(());
            }
        }

        USBDeviceDefaultHandler::new(BOOTLOADER_USB_DESCRIPTORS)
            .handle_control_response(setup, res)
            .await
    }
}

define_thread!(Main, main_thread_fn, params: BootloaderParams);
async fn main_thread_fn(params: BootloaderParams) {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);
    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    {
        let mut serial = UARTE::new(peripherals.uarte0, pins.P0_30, pins.P0_31, 115200);
        log::setup(serial).await;
    }

    log!(b"Enter Bootloader!\n");

    let mut usb_controller = USBDeviceController::new(peripherals.usbd, peripherals.power);
    usb_controller
        .run(BootloaderUSBHandler::new(params, peripherals.nvmc, timer))
        .await;

    // Never reached
    loop {}
}

// NOTE: This code should not depend on any peripherals being initialized as we
// want to run it as early in the boot process as possible and not initialize
// any peripherals to a non-initial state which the application may not be able
// to handle.
fn maybe_enter_application(params: &BootloaderParams) {
    let mut peripherals = peripherals::raw::Peripherals::new();

    let reset_reason = peripherals.power.resetreas.read();
    let reset_state = ResetState::from_value(peripherals.power.gpregret.read());

    peripherals
        .power
        .gpregret
        .write(ResetState::Default.to_value());

    // Clear by setting to all 1's
    peripherals
        .power
        .resetreas
        .write(RESETREAS_VALUE::from_raw(0xffffffff));

    let mut should_enter_bootloader = false;

    // Enter the bootloader if the reset was triggered by the RESET pin.
    should_enter_bootloader |= reset_reason.resetpin().is_detected();

    match reset_state {
        ResetState::Default => {}
        ResetState::EnterBootloader => {
            should_enter_bootloader = true;
        }
        ResetState::EnterApplication => {
            should_enter_bootloader = false;
        }
        ResetState::Unknown(_) => {}
    }

    // We can only enter the application if it is valid.
    if !should_enter_bootloader {
        if !has_valid_application(params) {
            return;
        }

        enter_application();
    }
}

fn has_valid_application(params: &BootloaderParams) -> bool {
    if params.application_size() == 0 || params.num_flashes() == 0 {
        return false;
    }

    let mut app_data = unsafe { application_code_data() };
    if (params.application_size() as usize) > app_data.len() {
        return false;
    }

    app_data = &app_data[..(params.application_size() as usize)];

    let expected_sum = {
        let mut hasher = CRC32CHasher::new();
        hasher.update(app_data);
        hasher.finish_u32()
    };

    if expected_sum != params.application_crc32c() {
        return false;
    }

    true
}

fn enter_application() {
    // See https://developer.arm.com/documentation/ka001423/1-0

    // TODO: Do this as early as possible (ideally in main() before peripherals are
    // loaded).
    unsafe {
        let sp = read_volatile(APPLICATION_CODE_OFFSET as *mut u32);
        let ep = read_volatile((APPLICATION_CODE_OFFSET + 4) as *mut u32);

        core::ptr::write_volatile(NVIC_VTOR, APPLICATION_CODE_OFFSET);
        asm!(
            "mov sp, {sp}",
            "bx {ep}",
            sp = in(reg) sp,
            ep = in(reg) ep,
        )
    };
}

entry!(main);
fn main() -> () {
    let params = read_bootloader_params();
    maybe_enter_application(&params);

    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);

    // TODO: If we are not using an external crystal, this needs to derive from
    // HFCLK.
    nordic::clock::init_low_freq_clk(&mut peripherals.clock);

    Main::start(params);

    // TODO: Setup the NRESET pin.

    // Enable interrupts.
    unsafe { asm!("cpsie i") };
    loop {
        unsafe { asm!("nop") };
    }
}
