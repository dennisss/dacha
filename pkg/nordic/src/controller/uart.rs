use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode, UARTReceiveRequest
};
use peripherals::raw::gpiote::GPIOTE;

use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;
use crate::rtc::RTC;
use crate::uarte::UARTE;

// TODO: Make this configurable.
const TIMEOUT_MILLIS: u32 = 100;

define_thread!(
    UartTransmitPeripheralThread,
    uart_transmit_worker_thread,
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    uarte: UARTE,
    data: FixedVec<u8, 8>,
    receive_request: Option<UARTReceiveRequest>
);

async fn uart_transmit_worker_thread(
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    mut uarte: UARTE,
    data: FixedVec<u8, 8>,
    receive_request: Option<UARTReceiveRequest>
) {
    executor::interrupts::yield_now().await;

    if data.len() > 0 {
        uarte.write(&data).await;
    }

    let mut res = PeripheralResponse::default();
    res.set_request_sequence(request_sequence);
    if let Some(req) = receive_request {
        if req.num_bytes() > 0 {
            // Clear the RX buffer of any bytes read before the TX finished.
            uarte.flush();

            let mut clock1 = controller.clock.clone();
            let timeout = async {
                clock1.wait_ms(TIMEOUT_MILLIS).await;
                false
            };

            let mut buf = [0u8; 8];
            let n = req.num_bytes() as usize;
            // TODO: Need to verify it isn't too big.

            let read_future = uarte.read_exact(&mut buf[0..n]);
            
            let result = race!(async move { read_future.await; true }, timeout).await;

            if result {
                res.data_val_mut().extend_from_slice(&buf[0..n]);
            }
        }
    }


    lock!(state <= controller.state.lock(), {
        state.entries[peripheral_index] = PeripheralEntry::UARTE(uarte);

        controller.write_response(&mut state, &res);
    });
}