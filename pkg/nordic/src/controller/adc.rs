use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};


use crate::adc::*;
use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;

type AdcBufferType = [i16; 1024];

static mut ADC_BUFFER: AdcBufferType = [0i16; 1024];
static mut ADC_BUFFER_LEN: usize = 0;


define_thread!(
    ADCCalibratePeripheralThread,
    adc_calibrate_worker_thread,
    controller: &'static PeripheralsController,
    request_sequence: u32,
    inst: WindowADC
);

async fn adc_calibrate_worker_thread(
    controller: &'static PeripheralsController,
    request_sequence: u32,
    mut inst: WindowADC
) {
    inst.calibrate_offset().await;

    lock!(state <= controller.state.lock().await.unwrap(), {
        state.adc = Some(inst);
        let mut res = PeripheralResponse::default();
        res.set_request_sequence(request_sequence);
        controller.write_response(&mut state, &res);
    });
}

define_thread!(
    ADCSamplePeripheralThread,
    adc_sample_worker_thread,
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    inst: WindowADC,
    config: ADCChannelConfig,
    window_sample: bool
);

async fn adc_sample_worker_thread(
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    mut inst: WindowADC,
    config: ADCChannelConfig,
    window_sample: bool
) {
    let mut res = PeripheralResponse::default();
    res.set_request_sequence(request_sequence);

    if window_sample {
        let status = inst.window_sample(&config, unsafe { &mut ADC_BUFFER }).await;
        unsafe {
            ADC_BUFFER_LEN = status.num_samples;
        }

        if status.limit_high_exceeded || status.limit_low_exceeded {
            res.set_uint_val(1u32);
        }
    } else {
        let value = inst.single_sample(&config).await as u16 as u32;
        res.set_uint_val(value);
    }

    lock!(state <= controller.state.lock().await.unwrap(), {
        state.entries[peripheral_index] = PeripheralEntry::ADC {
            config
        };

        state.adc = Some(inst);
        controller.write_response(&mut state, &res);
    });
}

pub fn read_adc_buffer(offset: usize, res: &mut PeripheralResponse) {
    let data: &[u8] = unsafe {
        let len = core::mem::size_of::<i16>() * ADC_BUFFER_LEN;
        core::slice::from_raw_parts(
            core::mem::transmute::<*const i16, *const u8>(ADC_BUFFER.as_ptr()),
            len
        )
    };

    if offset >= data.len() {
        return;
    }

    let offset_end = core::cmp::min(offset + 32, data.len());
    
    res.data_val_mut().extend_from_slice(&data[offset..offset_end]);
}