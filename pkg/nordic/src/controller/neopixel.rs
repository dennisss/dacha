use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};

use crate::neopixel::Neopixel;
use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;


define_thread!(
    NeopixelPeripheralThread,
    neopixel_worker_thread,
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    inst: Neopixel,
    data: FixedVec<u8, 16>
);

async fn neopixel_worker_thread(
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    mut inst: Neopixel,
    data: FixedVec<u8, 16>
) {
    inst.write(&data).await;

    lock!(state <= controller.state.lock().await.unwrap(), {
        state.entries[peripheral_index] = PeripheralEntry::Neopixel {
            inst
        };

        let mut res = PeripheralResponse::default();
        res.set_request_sequence(request_sequence);
        controller.write_response(&mut state, &res);
    });
}