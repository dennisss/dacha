use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};

use crate::neopixel::*;
use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;
use crate::controller::allocator::*;

pub struct NeopixelPeripheral {
    inst: Neopixel,
    buf: NeopixelDataBuffer<BoxedSlice<u8>>
}

impl NeopixelPeripheral {
    pub fn new(inst: Neopixel, buf: NeopixelDataBuffer<BoxedSlice<u8>>) -> Self {
        Self {
            inst,
            buf
        }
    }

    // TODO: Propagate errors.
    pub fn write(&mut self, index: usize, data: &[u8]) {
        self.buf.write(index, data);
    }

    pub async fn show(&mut self) {
        self.inst.write(&self.buf).await
    }

    pub fn into_inner(self) -> Neopixel {
        self.inst
    }
}



define_thread!(
    NeopixelPeripheralThread,
    neopixel_worker_thread,
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    inst: NeopixelPeripheral
);

async fn neopixel_worker_thread(
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    mut inst: NeopixelPeripheral
) {
    inst.show().await;

    lock!(state <= controller.state.lock().await.unwrap(), {
        state.entries[peripheral_index] = PeripheralEntry::Neopixel(inst);

        let mut res = PeripheralResponse::default();
        res.set_request_sequence(request_sequence);
        controller.write_response(&mut state, &res);
    });
}