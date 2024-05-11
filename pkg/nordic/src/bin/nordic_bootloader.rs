/*

cargo run --bin builder -- build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840_bootloader

cargo run --bin flasher -- built/pkg/nordic/nordic_bootloader --usb_device_id=1

da build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840_bootloader

openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program built/pkg/nordic/nordic_bootloader verify" -c reset -c exit


- `~/apps/gcc-arm-none-eabi-10.3-2021.10/bin/arm-none-eabi-gdb /home/dennis/workspace/dacha/built-rust/bfd75a5982e33698/thumbv7em-none-eabihf/release/nordic_bootloader`
- `target extended-remote localhost:3333`
- `monitor reset halt`
- `load built/pkg/nordic/nordic_bootloader`


Bootstrapping the keyboard:

    cargo run --bin builder -- build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52833_bootloader


    openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program built/pkg/nordic/nordic_bootloader verify" -c reset -c exit

    ~/apps/gcc-arm-none-eabi-10.3-2021.10/bin/arm-none-eabi-gdb
    target extended-remote /dev/ttyACM0
    monitor swdp_scan
    attach 1
    monitor erase_mass
    load built/pkg/nordic/nordic_bootloader

    set *0xAAA = 0xAA

    set {int}0x83040 = 4

APPROTECT : 0x10001000 + 0x208

First write CONFIG (0x4001E000 + 0x504) to 2 to enable erases

ERASEALL: 0x4001E000 + 0x50C : Write 1 to trigger.

Then write CONFIG to 0


Notes:
- The UF2 will present addresses in strictly increasing order and the data must align to 32 bits
- The bootloader runs entirely from RAM so must fit in it (with room to spare for the stack).

TODOs:
- Support partially reading requests so that we can stale them.

We will implement USB DFU
- https://www.usb.org/sites/default/files/DFU_1.1.pdf

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
    impl_trait_in_assoc_type,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types
)]
#![no_std]
#![no_main]

#[macro_use]
extern crate executor;
#[macro_use]
extern crate common;
#[macro_use]
extern crate nordic;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate logging;

use core::arch::asm;
use core::future::Future;
use core::ptr::read_volatile;

use common::fixed::vec::FixedVec;
use common::register::RegisterRead;
use common::register::RegisterWrite;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
use logging::Logger;
use nordic::bootloader::app::*;
use nordic::bootloader::flash::*;
use nordic::bootloader::params::*;
use nordic::config_storage::NetworkConfigStorage;
use nordic::gpio::*;
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
use nordic_proto::nordic::BootloaderParams;
use nordic_wire::request_type::ProtocolRequestType;
use nordic_wire::usb_descriptors::*;
use peripherals::raw::nvmc::NVMC;
use peripherals::raw::uicr::UICR;
use peripherals::raw::uicr::UICR_REGISTERS;
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
    ///
    /// TODO: Move this to global memory to save space.
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
    /// written. This should only monotonically increase and can be used to
    /// track which pages we have already erased.
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

                // TODO: Dedup this with the Manifestation code.

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
                        log!("DFU_DNLOAD: Wrong state");
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
                        log!("DFU_DNLOAD: Bad UF2 block");
                        self.status_code = DFUStatusCode::errSTALLEDPKT;
                        return Ok(());
                    }
                };

                let dfu_block_num = setup.wValue;
                if state.next_block_number as u16 != dfu_block_num
                    || state.next_block_number != block.block_number
                {
                    log!("DFU_DNLOAD: Non-monotonic block num");
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
                    log!("DFU_DNLOAD: Non-monotonic addr");
                    self.status_code = DFUStatusCode::errADDRESS;
                    return Ok(());
                }

                // We are only allowed to write full words at word offsets.
                if block.target_addr % 4 != 0
                    || block.payload_size % 4 != 0
                    || block.payload_size == 0
                {
                    log!("DFU_DNLOAD: Unaligned write");
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                let mut in_application_code = false;

                let uicr_start_address =
                    unsafe { core::mem::transmute::<&UICR_REGISTERS, u32>(&*UICR::new()) };
                let uicr_end_address =
                    uicr_start_address + (core::mem::size_of::<UICR_REGISTERS>() as u32);

                let segment = match FlashSegment::from_address(block.target_addr) {
                    Some(v) => v,
                    None => {
                        log!("DFU_DNLOAD: Unknown flash addr");
                        self.status_code = DFUStatusCode::errADDRESS;
                        return Ok(());
                    }
                };

                // We only support writing to one type of segment at a time so ensure that all
                // the memory is in the same segment.
                let block_end_addr = block.target_addr + block.payload_size;
                if Some(segment) != FlashSegment::from_address(block_end_addr - 1) {
                    log!("DFU_DNLOAD: Mixing flash segments");
                    self.status_code = DFUStatusCode::errADDRESS;
                    return Ok(());
                }

                match segment {
                    // Normal flash segments.
                    FlashSegment::BootloaderCode
                    | FlashSegment::ApplicationCode
                    | FlashSegment::ApplicationParams => {
                        // TODO: Require a special flag to be flipped if we attempt to overwrite the
                        // bootloader itself

                        if state.next_flash_offset <= segment.start_address() {
                            // Must always write the first bytes of the application|bootloader
                            // (doesn't make sense to have an
                            // application|bootloader without a vector table).
                            if block.target_addr != segment.start_address() {
                                log!("DFU_DNLOAD: Must write segment start");
                                self.status_code = DFUStatusCode::errADDRESS;
                                return Ok(());
                            }

                            // Advance forward our flash offset to this segment.
                            // Don't need to erase any partially completed pages in previous
                            // segments.
                            state.next_flash_offset = segment.start_address();
                        }

                        // Explicitly write all flash space between our last written offset and the
                        // next one. This has the following purposes:
                        // - Ensures that if block.target_addr is midway through a page, the page is
                        //   erased.
                        // - For application code, ensures that we CRC a contiguous segment of code
                        //   with deterministic zero padding for undefined regions.
                        let mut empty_word = [0u8; 4];
                        while state.next_flash_offset < block.target_addr {
                            write_to_flash(state.next_flash_offset, &empty_word, &mut self.nvmc);
                            state.next_flash_offset += empty_word.len() as u32;

                            state.total_written += empty_word.len() as u32;
                            if segment == FlashSegment::ApplicationCode {
                                state.application_hasher.update(&empty_word);
                                state.application_size += empty_word.len() as u32;
                            }
                        }
                        assert_no_debug!(state.next_flash_offset == block.target_addr);

                        write_to_flash(block.target_addr, block.payload(), &mut self.nvmc);
                        state.next_flash_offset = block.target_addr + block.payload_size;

                        state.total_written += block.payload_size;
                        if segment == FlashSegment::ApplicationCode {
                            state.application_hasher.update(block.payload());
                            state.application_size += block.payload_size;
                        }
                    }
                    FlashSegment::UICR => {
                        // Write to UICR. This has a special way to perform erases and is only
                        // written to sparsely.

                        // TODO: Maybe support sparse writes by reading back the old values (the
                        // tricky part is that new values don't appear until
                        // a reset occurs so we can't perform flashing more
                        // than once until a reset occurs).

                        // TODO: Disallow writing to UICR unless the bootloader was
                        // also written? (to prevent the user firmware from
                        // accidentally overwriting stuff).

                        if state.next_flash_offset <= segment.start_address() {
                            erase_uicr_async(&mut self.nvmc);
                        }

                        write_to_uicr(block.target_addr, block.payload(), &mut self.nvmc);
                        state.total_written += block.payload_size;
                        state.next_flash_offset = block.target_addr + block.payload_size;
                    }
                    FlashSegment::BootloaderParams => {
                        log!("DFU_DNLOAD: Can not overwrite params");
                        self.status_code = DFUStatusCode::errADDRESS;
                        return Ok(());
                    }
                }

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
        if setup.bmRequestType == 0b11000000
        /* Device-to-host | Vendor | Device */
        {
            if setup.bRequest == ProtocolRequestType::ReadLog.to_value() {
                let mut buffer = [0u8; 256];
                if (setup.wLength as usize) < buffer.len() {
                    res.stale();
                    return Ok(());
                }

                let mut n = 0;

                while n < buffer.len() {
                    if let Some(len) = Logger::global().try_read(&mut buffer[(n + 1)..]).await {
                        buffer[n] = len as u8;
                        n += len + 1;
                    } else {
                        break;
                    }
                }

                res.write(&buffer[0..n]).await?;
                return Ok(());
            }
        }

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

define_thread!(
    Main,
    main_thread_fn,
    reason: EnterBootloaderReason,
    params: BootloaderParams
);
async fn main_thread_fn(reason: EnterBootloaderReason, params: BootloaderParams) {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);
    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    log!("Enter Bootloader!");
    log!("Num Flashes: ", params.num_flashes());
    log!("CRC: ", params.application_crc32c());
    log!("Reason: ", reason as u32);

    let mut usb_controller = USBDeviceController::new(peripherals.usbd, peripherals.power);
    usb_controller
        .run(BootloaderUSBHandler::new(params, peripherals.nvmc, timer))
        .await;

    // Never reached
    loop {}
}

entry!(main);

// This is not inlined into entry() to allow it to be separately stored in RAM.
#[inline(never)]
#[no_mangle]
fn main() -> () {
    // TODO: Keep all of this flash?
    let params = read_bootloader_params();
    let reason = maybe_enter_application(&params);

    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);

    // TODO: If we are not using an external crystal, this needs to derive from
    // HFCLK.
    nordic::clock::init_low_freq_clk(
        nordic::clock::LowFrequencyClockSource::RCOscillator,
        &mut peripherals.clock,
    );

    Main::start(reason, params);

    // Enable interrupts.
    unsafe { asm!("cpsie i") };
    loop {
        unsafe { asm!("nop") };
    }
}
