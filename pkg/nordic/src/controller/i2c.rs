use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode, I2CTransfer
};

use crate::twim::*;
use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;


define_thread!(
    I2CPeripheralThread,
    i2c_worker_thread,
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    inst: TWIM,
    request: I2CTransfer
);

async fn i2c_worker_thread(
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    mut inst: TWIM,
    request: I2CTransfer
) {
    let mut buf = [0u8; 32];

    // TODO: Bounda check this.
    let read_buffer = &mut buf[0..(request.read_count() as usize)];

    // TODO: Double check if anything needs to be aligned in memory.

    let r = inst.write_then_read(
        request.address() as u8,
        // TODO: Support zero length reads/writes.
        if request.write_data().len() > 0 {
            Some(request.write_data())
        } else {
            None
        },
        if request.read_count() > 0 {
            Some(read_buffer)
        } else {
            None
        }
    ).await;

    let mut res = PeripheralResponse::default();
    res.set_request_sequence(request_sequence);
    
    if r.is_err() {
        // Probably no acknowledgement was received.
        res.set_error_code(PeripheralResponse_ErrorCode::UNKNOWN);
    } else {
        res.data_val_mut().extend_from_slice(read_buffer);
    }

    lock!(state <= controller.state.lock(), {
        state.entries[peripheral_index] = PeripheralEntry::I2C(inst);
        controller.write_response(&mut state, &res);
    });
}