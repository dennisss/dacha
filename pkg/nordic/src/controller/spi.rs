use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};

use crate::spi::*;
use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;


define_thread!(
    SPIPeripheralThread,
    spi_worker_thread,
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    inst: SPIHost,
    data: FixedVec<u8, 8>
);

async fn spi_worker_thread(
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    mut inst: SPIHost,
    data: FixedVec<u8, 8>
) {
    executor::interrupts::yield_now().await;

    let mut out = FixedVec::<u8, 8>::new();
    out.resize(data.len(), 0u8);

    inst.transfer(&data, &mut out).await;

    lock!(state <= controller.state.lock(), {
        state.entries[peripheral_index] = PeripheralEntry::SPI(inst);

        let mut res = PeripheralResponse::default();
        res.set_request_sequence(request_sequence);
        res.data_val_mut().extend_from_slice(&out);
        controller.write_response(&mut state, &res);
    });
}