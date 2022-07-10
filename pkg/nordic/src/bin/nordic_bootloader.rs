/*
da build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840

openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program built/pkg/nordic/nordic_bootloader verify" -c reset -c exit

Notes:
- The UF2 will present addresses in strictly increasing order and the data must align to 32 bits


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

use nordic::config_storage::NetworkConfigStorage;
use nordic::gpio::*;
use nordic::log;
use nordic::reset::*;
use nordic::timer::Timer;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceControlRequest;
use nordic::usb::controller::{
    USBDeviceControlResponse, USBDeviceController, USBDeviceNormalRequest,
};
use nordic::usb::default_handler::USBDeviceDefaultHandler;
use nordic::usb::handler::{USBDeviceHandler, USBError};
use nordic_proto::usb_descriptors::*;
use peripherals::raw::nvmc::NVMC;
use peripherals::raw::power::resetreas::RESETREAS_VALUE;
use peripherals::raw::register::RegisterRead;
use peripherals::raw::register::RegisterWrite;
use uf2::*;
use usb::descriptors::SetupPacket;
use usb::dfu::*;

const FLASH_START_ADDRESS: u32 = 0x1000000; // TODO

const FLASH_BLOCK_SIZE: u32 = 4096;

extern "C" {
    static _flash_start: u32;
    static _flash_end: u32;
}

pub struct BootloaderUSBHandler {
    nvmc: NVMC,

    /// Status of the last command performed.
    status_code: DFUStatusCode,

    state: State,

    /// NOTE: The size of this must match the wTransferSize in the descriptor
    buffer: [u8; 512],
}

enum State {
    Idle,
    Downloading(DownloadingState),
}

impl State {
    fn to_dfu_state(&self) -> DFUState {
        match self {
            State::Idle => DFUState::dfuIDLE,
            State::Downloading(_) => DFUState::dfuDNLOAD_IDLE,
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

    /// Total number of UF2 blocks we expect to see.
    total_blocks: u32,
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
    pub fn new(nvmc: NVMC) -> Self {
        Self {
            nvmc,
            status_code: DFUStatusCode::OK,
            state: State::Idle,
            buffer: [0u8; UF2_BLOCK_SIZE],
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
            } else if setup.bRequest == DFURequestType::DFU_CLRSTATUS as u8 {
                self.status_code = DFUStatusCode::OK;
                req.read(&mut []).await?;
                return Ok(());
            } else if setup.bRequest == DFURequestType::DFU_DNLOAD as u8 {
                if let State::Idle = &self.state {
                    self.state = State::Downloading(DownloadingState {
                        next_flash_offset: FLASH_START_ADDRESS,
                        next_block_number: 0,
                        total_blocks: 0,
                    });
                }

                let state = match &mut self.state {
                    State::Downloading(s) => s,
                    _ => {
                        self.status_code = DFUStatusCode::errSTALLEDPKT;
                        req.stale();
                        return Ok(());
                    }
                };

                let nread = req.read(&mut self.buffer).await?;
                if nread == 0 {
                    // Enter manifestation mode. We already wrote the flash in previous requests so
                    // just reset.
                    nordic::reset::reset_to_application();
                }

                // let block = UF2Block::default()

                let block = match UF2Block::cast_from(&self.buffer[0..nread]) {
                    Some(v) => v,
                    None => {
                        self.status_code = DFUStatusCode::errSTALLEDPKT;
                        return Ok(());
                    }
                };

                let dfu_block_num = setup.wValue;
                if state.next_block_number as u16 != dfu_block_num
                    || state.next_block_number != block.block_number
                {
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                if block.flags != UF2Flags::empty() {
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                // TODO: Prevent this from overflowing.
                state.next_block_number += 1;

                // TODO: Validate this hasn't changed?
                state.total_blocks = block.num_blocks;

                // TODO: Require a special flag to be flipped if we attempt to overwrite the
                // bootloader itself
                // TODO: Double check this against INFO.FLASH in the FICR registers.
                if block.target_addr < unsafe { _flash_start }
                    || block.target_addr >= unsafe { _flash_end }
                {
                    self.status_code = DFUStatusCode::errADDRESS;
                    return Ok(());
                }

                // TODO: Validate the block's family_id if it is present.

                // TODO: Require that the first written address is at the start of the
                // application memory (as defined by the bootloader).

                if block.target_addr < state.next_flash_offset {
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                // We are only allowed to write full words at word offsets.
                if block.target_addr % 4 != 0 || block.payload_size % 4 != 0 {
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                if block.target_addr % FLASH_BLOCK_SIZE == 0 {
                    self.nvmc
                        .config
                        .write_with(|v| v.set_wen_with(|v| v.set_een()));
                    self.nvmc.erasepage.write(block.target_addr);
                    self.nvmc
                        .config
                        .write_with(|v| v.set_wen_with(|v| v.set_ren()));
                } else if block.target_addr != state.next_flash_offset {
                    // We are writing somewhere in the middle of a memory block. This means we need
                    // to make sure the old data is erases. But, becuase we can't partially erase a
                    // flash page, we can only allow writes if the previous UF2 block was also
                    // writing to this page.
                    self.status_code = DFUStatusCode::errSTALLEDPKT;
                    return Ok(());
                }

                let words = unsafe {
                    core::slice::from_raw_parts::<u32>(
                        core::mem::transmute(block.data.as_ptr()),
                        (block.payload_size / 4) as usize,
                    )
                };

                self.nvmc
                    .config
                    .write_with(|v| v.set_wen_with(|v| v.set_wen()));
                state.next_flash_offset = block.target_addr;
                for w in words {
                    while self.nvmc.readynext.read().is_busy() {
                        continue;
                    }

                    unsafe { core::ptr::write_volatile(state.next_flash_offset as *mut u32, *w) };

                    state.next_flash_offset += 4;
                }
                self.nvmc
                    .config
                    .write_with(|v| v.set_wen_with(|v| v.set_ren()));

                // Wait for all writes to complete.
                while self.nvmc.ready.read().is_busy() {
                    continue;
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

                return res
                    .write(unsafe {
                        core::slice::from_raw_parts(
                            core::mem::transmute(&status),
                            core::mem::size_of::<DFUStatus>(),
                        )
                    })
                    .await;
            }
        }

        USBDeviceDefaultHandler::new(BOOTLOADER_USB_DESCRIPTORS)
            .handle_control_response(setup, res)
            .await
    }
}

define_thread!(Main, main_thread_fn);
async fn main_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);
    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    {
        let mut serial = UARTE::new(peripherals.uarte0, pins.P0_30, pins.P0_31, 115200);
        log::setup(serial).await;
    }

    log!(b"Started up!\n");

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

    // if reset_reason.resetpin().is_detected() || reset_reason.sreq().is_detected()
    // {

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

    // TODO: Check the integrity of the app code

    should_enter_bootloader = true;

    if should_enter_bootloader {
        let mut usb_controller = USBDeviceController::new(peripherals.usbd, peripherals.power);
        usb_controller
            .run(BootloaderUSBHandler::new(peripherals.nvmc))
            .await;

        // Never reached
        loop {}
    }

    // Otherwise, jump to software

    loop {
        log!(b"Hi!\n");

        timer.wait_ms(1500).await;
    }
}

entry!(main);
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);
    nordic::clock::init_low_freq_clk(&mut peripherals.clock);

    Main::start();

    // TODO: Setup the NRESET pin.

    // Enable interrupts.
    unsafe { asm!("cpsie i") };
    loop {
        unsafe { asm!("nop") };
    }
}
