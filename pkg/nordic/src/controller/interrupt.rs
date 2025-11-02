use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};
use executor::interrupts::wait_for_irq;
use peripherals::raw::EventRegister;
use peripherals::raw::Interrupt;

use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;


define_thread!(
    InterruptPeripheralThread,
    interrupt_worker_thread,
    controller: &'static PeripheralsController
);

async fn interrupt_worker_thread(
    controller: &'static PeripheralsController
) {
    loop {
        lock!(state <= controller.state.lock().await.unwrap(), {
            for entry in &mut state.entries {
                let mut interrupt = match entry {
                    PeripheralEntry::GPIO { interrupt, .. } => interrupt,
                    _ => continue
                };

                if let Some(interrupt) = &mut interrupt {
                    interrupt.fired |= interrupt.channel.pending_events();
                }
            }
        });

        wait_for_irq(Interrupt::GPIOTE).await;
    }
}