use common::fixed::vec::FixedVec;
use common::fixed::queue::FixedQueue;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};


use crate::adc::*;
use crate::controller::peripherals_controller::{PeripheralsController, PeripheralsControllerState};
use crate::controller::PeripheralEntry;
use crate::controller::buffer::Buffer;


#[derive(Default)]
pub struct ADCRequestQueue {
    requests: FixedQueue<ADCRequest, 16>
}

// TODO: Double check this only takes 4 bytes?
pub struct ADCRequest {
    pub request_sequence: u8,
    pub typ: ADCRequestType,
}

pub enum ADCRequestType {
    Calibrate,
    SingleSample {
        peripheral_index: u8,

    },
    WindowSample {
        peripheral_index: u8,
        buffer_index: u8,
    },
}

impl ADCRequestQueue {
    /// Returns true if it was successfully enqueued.
    pub fn enqueue(&mut self, request: ADCRequest) -> bool {
        if self.requests.is_full() {
            return false;
        }

        self.requests.push_back(request);

        true
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub fn clear(&mut self) {
        self.requests.clear();
    }
}

define_thread!(
    ADCSamplePeripheralThread,
    adc_sample_worker_thread,
    controller: &'static PeripheralsController
);

struct InternalADCRequest {
    adc: WindowADC,
    request_sequence: u32,
    typ: InternalADCRequestType
}

enum InternalADCRequestType {
    Calibrate,

    SingleSample {
        config: WindowADCChannelConfig,
    },

    WindowSample {
        config: WindowADCChannelConfig,
        buffer: Buffer,
        buffer_index: usize,
    }
}

impl InternalADCRequest {

    fn resolve(
        state: &mut PeripheralsControllerState,
        req: &ADCRequest
    ) -> Result<Self, PeripheralResponse_ErrorCode> {

        let typ = match &req.typ {
            ADCRequestType::Calibrate => {
                InternalADCRequestType::Calibrate
            },
            ADCRequestType::SingleSample { peripheral_index } => {

                let config = match &mut state.entries[*peripheral_index as usize] {
                    PeripheralEntry::ADC(config) => config.clone(),
                    _ => {
                        return Err(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        );
                    }
                };

                InternalADCRequestType::SingleSample {
                    config
                }
            },
            ADCRequestType::WindowSample { peripheral_index, buffer_index } => {

                // TODO: Dedup me with above.
                let config = match &mut state.entries[*peripheral_index as usize] {
                    PeripheralEntry::ADC(config) => config.clone(),
                    _ => {
                        return Err(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        );
                    }
                };

                let buffer_index = *buffer_index as usize;

                match &mut state.entries[buffer_index] {
                    PeripheralEntry::Buffer(_) => {}
                    _ => {
                        return Err(PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND);
                    }
                };

                let buffer = {
                    let mut e = PeripheralEntry::Borrowed;
                    core::mem::swap(&mut e, &mut state.entries[buffer_index]);
                    match e {
                        PeripheralEntry::Buffer(buffer) => buffer,
                        _ => panic!(),
                    }
                };

                InternalADCRequestType::WindowSample {
                    config,
                    buffer,
                    buffer_index
                }
            },
        };

        // This is the only thread using the ADC so it should always be available.
        let adc = state.adc.take().unwrap();

        Ok(Self {
            adc,
            request_sequence: req.request_sequence as u32,
            typ
        })
    }

    async fn execute(&mut self) -> PeripheralResponse {
        let mut res = PeripheralResponse::default();
        res.set_request_sequence(self.request_sequence);

        match &mut self.typ {
            InternalADCRequestType::Calibrate => {
                self.adc.calibrate_offset().await;
            }
            InternalADCRequestType::SingleSample { config } => {
                let value = self.adc.single_sample(&config).await as u16 as u32;
                res.set_uint_val(value);
            }
            InternalADCRequestType::WindowSample { config, buffer, .. } => {
                let mut buffer_view = buffer.view_mut::<i16>();

                // TODO: Check for failure.
                let status = match self.adc.window_sample(&config, buffer_view.raw()).await {
                    Some(v) => v,
                    None => {
                        // TODO: Pick a better error.
                        res.set_error_code(PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND);
                        return res;
                    }
                };

                buffer_view.set_used(status.num_samples);

                if status.limit_high_exceeded || status.limit_low_exceeded {
                    // TODO: Mark the current time (if zero, return 1)
                    // return_time = true;
                    res.set_uint_val(1u32);
                }
            }
        }

        res
    }

    fn reclaim(self, state: &mut PeripheralsControllerState) {
        state.adc = Some(self.adc);

        if let InternalADCRequestType::WindowSample { config, buffer, buffer_index } = self.typ {
            state.entries[buffer_index] = PeripheralEntry::Buffer(buffer);
        }
    }
}

async fn adc_sample_worker_thread(
    controller: &'static PeripheralsController
) {
    executor::interrupts::yield_now().await;

    loop {
        // Extract everything from the state needed to fulfill the next request.
        let request = lock!(state <= controller.state.lock(), {

            let req = match state.adc_request_queue.requests.pop_front() {
                Some(v) => v,
                None => return None
            };

            let internal_req = match InternalADCRequest::resolve(&mut state, &req) {
                Ok(v) => v,
                Err(e) => {
                    let mut res = PeripheralResponse::default();
                    res.set_request_sequence(req.request_sequence as u32);
                    res.set_error_code(PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED);
                    controller.write_response(&mut state, &res);
                    return None
                }
            };

            Some(internal_req)
        });

        // Execute the request
        if let Some(mut request) = request {
            let mut res = request.execute().await;

            lock!(state <= controller.state.lock(), {
                // Return roughly the time of the last sample.
                match controller.timer.capture() {
                    Some(v) => res.set_time(v),
                    None => {
                        res.set_error_code(PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED);
                    }
                }
                
                request.reclaim(&mut state);
                controller.write_response(&mut state, &res);
            });

            // Check immediately for more requests.
            continue;
        }

        controller.adc_request_queue_filled.recv().await;
    }
}

