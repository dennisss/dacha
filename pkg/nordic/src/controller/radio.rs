use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode, I2CTransfer
};
use nordic_wire::packet::PacketBuffer;

use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;
use crate::timer::*;
use crate::ppi::*;
use crate::radio::*;
use crate::controller::buffer::Buffer;

#[derive(Default)]
pub struct RadioEntry {
    // pending_tx_buffer: Option<(RadioAddress, usize)>,
}

define_thread!(
    RadioSocketSenderThread,
    radio_socket_sender_thread,
    controller: &'static PeripheralsController,
    request_sequence: u32,
    buffer: Buffer,
    buffer_peripheral_index: usize
);

async fn radio_socket_sender_thread(
    controller: &'static PeripheralsController,
    request_sequence: u32,
    mut buffer: Buffer,
    buffer_peripheral_index: usize
) {
    executor::interrupts::yield_now().await;

    let mut buf_u8 = buffer.view_mut::<u8>();

    let mut pkt = PacketBuffer::new();
    pkt.raw_mut().copy_from_slice(buf_u8.raw());

    // let buf_packet = unsafe { core::mem::transmute::<*mut u8, &mut PacketBuffer>(buf_u8.raw().as_mut_ptr()) };

    let failed = controller.radio_socket.enqueue_tx(&mut pkt).await.is_err();

    lock!(state <= controller.state.lock(), {
        state.entries[buffer_peripheral_index] = PeripheralEntry::Buffer(buffer);

        let mut res = PeripheralResponse::default();
        if failed {
            res.set_error_code(PeripheralResponse_ErrorCode::UNKNOWN);
        }
        res.set_request_sequence(request_sequence);
        controller.write_response(&mut state, &res);
    });
}



define_thread!(
    RadioSocketReceiverThread,
    radio_socket_receiver_thread,
    controller: &'static PeripheralsController,
    request_sequence: u32,
    buffer: Buffer,
    buffer_peripheral_index: usize
);

async fn radio_socket_receiver_thread(
    controller: &'static PeripheralsController,
    request_sequence: u32,
    mut buffer: Buffer,
    buffer_peripheral_index: usize
) {
    executor::interrupts::yield_now().await;

    controller.radio_socket.wait_for_rx().await;

    let mut buf_u8 = buffer.view_mut::<u8>();

    let buf_packet = unsafe { core::mem::transmute::<*mut u8, &mut PacketBuffer>(buf_u8.raw().as_mut_ptr()) };

    // TODO: Check the return value.
    let success = controller.radio_socket.dequeue_rx(buf_packet).await;

    if success {
        let len = buf_packet.as_bytes().len();
        buf_u8.set_used(len);
    } else {
        buf_u8.set_used(0);
    }

    // TODO: Make this optional and add the sequence number.
    if success {
        let mut pkt = PacketBuffer::new();
        *pkt.remote_address_mut() = *buf_packet.remote_address();
        // TODO: Do something with this.
        let _ = controller.radio_socket.enqueue_tx(&mut pkt).await.is_err();
    }

    lock!(state <= controller.state.lock(), {
        state.entries[buffer_peripheral_index] = PeripheralEntry::Buffer(buffer);

        let mut res = PeripheralResponse::default();
        res.set_request_sequence(request_sequence);
        controller.write_response(&mut state, &res);
    });
}


// Old stuff

/*
pub struct RadioEntry {
    radio: Radio,
    time_sync: RadioTimeSyncer,
}

impl RadioEntry {
    pub fn create(
        mut radio: Radio,
        timer: &'static Timer,
        ppi: &mut PPIChannels
    ) -> Option<Self> {
        let time_sync = match RadioTimeSyncer::create(&mut radio, timer, ppi) {
            Some(v) => v,
            None => return None
        };

        Some(Self {
            radio,
            time_sync
        })
    }

    pub fn into_inner(self) -> Radio {
        self.radio
    }

}


// TODO: Replace with the TimedEvent
struct RadioTimeSyncer {
    timer_channel: TimerChannel<'static>,
    ppi_channel: PPIChannel,
}

impl RadioTimeSyncer {

    pub fn create(
        radio: &mut Radio,
        timer: &'static Timer,
        ppi: &mut PPIChannels
    ) -> Option<Self> {

        let mut timer_channel = match timer.new_channel() {
            Some(v) => v,
            None => return None
        };

        let mut ppi_channel = match ppi.new_channel(
            radio.end_event(),
            timer_channel.capture_task(),
        ) {
            Some(v) => v,
            None => return None
        };

        // Always enabled. Nothing bad happens if we keep it this way.
        ppi_channel.enable();

        Some(Self {
            timer_channel,
            ppi_channel
        })
    }
}
*/

