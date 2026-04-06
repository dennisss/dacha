use std::collections::HashMap;
use std::time::{Instant, Duration};
use std::future::Future;

use common::errors::*;
use nordic_driver::usb_radio::{USBRadio, ClockTimeResponse, USBSOFResponse};
use peripherals_proto::peripherals::*;

use crate::config::*;

/*
TODO: Refactoring this:
- Peripherals should be instantiatable and internally group all the logic they support 
- Need to figure out some good way to reason about peripheral exclusivity
    - At least during configuration, we can exclusively lock pins and references to other periphs.
*/


pub struct PeripheralsDevice {
    config: BoardConfig,
    usb_device: USBRadio,
    config_responses: HashMap<u32, PeripheralResponse> 
}

pub struct AnalogReadWindowResult {
    pub sampling_completion_time: u32,
    pub triggered: bool, 
}

pub struct I2CTransferResult {
    pub transfer_time: u32,
}


impl PeripheralsDevice {

    pub async fn create(config: &BoardConfig) -> Result<(Self, PeripheralsState)> {
        // TODO: Think about decoupling the state stuff.
        let (reqs, peripherals_state) = build_configuration_requests(config)?;

        let mut selector = usb::DeviceSelector::default();
        selector.vendor_id = Some(0x8888);
        selector.product_id = Some(if config.product_id() != 0 { config.product_id() as u16 } else { 0x0004 });
        let mut usb_device = USBRadio::find(&selector).await?;

        let mut config_responses = HashMap::default();

        // TODO: Need to support batch sending of requests. 
        for req in reqs {
            let res = usb_device.send_request(&req).await?;

            if req.has_finalize_config()  {
                continue;
            }

            config_responses.insert(req.peripheral_index(), res);
        }

        Ok((Self {
            config: config.clone(),
            usb_device,
            config_responses
        }, peripherals_state))
    }

    pub fn raw(&self) -> &USBRadio {
        &self.usb_device
    }

    pub fn config(&self) -> &BoardConfig {
        &self.config
    }

    pub fn periph_config<'a>(&'a self, periph_name: &str) -> Result<&'a BoardConfig_Peripheral> {
        self.config.peripherals().iter()
            .find(|p| p.name() == periph_name)
            .map(|v| v.as_ref())
            .ok_or_else(|| format_err!("No peripheral configured with name: {}", periph_name))
    }

    pub async fn send_request(&self, req: &PeripheralRequest) -> Result<PeripheralResponse> {
        self.usb_device.send_request(&req).await
    }

    pub async fn send_request_batch(&self, req: &[PeripheralRequest]) -> Result<Vec<PeripheralResponse>> {
        self.usb_device.send_request_batch(req).await
    }

    pub async fn get_clock_time(&self) -> Result<ClockTimeResponse> {
        self.usb_device.get_clock_time().await
    }

    pub async fn get_usb_sof_time(&self) -> Result<USBSOFResponse> {
        self.usb_device.get_usb_sof_time().await
    }

    pub async fn get_idle_counter(&self) -> Result<u32> {
        self.usb_device.get_idle_counter().await
    }

    pub async fn uart_transfer(
        &self,
        periph_name: &str,
        send: &[u8],
        receive: &mut [u8]
    ) -> Result<usize> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.uart_transmit_mut().data_mut().extend_from_slice(send);
        req.uart_transmit_mut().rx_after_tx_mut().set_num_bytes(receive.len() as u32);
        
        let res = self.usb_device.send_request(&req).await?;

        let n = res.data_val().len();
        receive[..n].copy_from_slice(res.data_val());
        Ok(n)
    }

    pub async fn spi_transfer(
        &self,
        periph_name: &str,
        send: &[u8],
        receive: &mut [u8]
    ) -> Result<usize> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.spi_transfer_mut().data_mut().extend_from_slice(send);
        
        let res = self.usb_device.send_request(&req).await?;

        // TODO: Check in range of 'receive' size.
        let n = res.data_val().len();
        receive[..n].copy_from_slice(res.data_val());
        Ok(n)
    }

    pub async fn spi_transfer_timed(
        &self,
        periph_name: &str,
        send: &[u8],
        read_buffer: &str,
        start_time: u32,
        transfer_count: usize,
        transfer_inverval: u32,
    ) -> Result<Vec<u8>> {
        let periph_index = self.periph_config(periph_name)?.index();
        let buffer_idx = self.periph_config(read_buffer)?.index();

        {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(periph_index);
            req.spi_transfer_mut().data_mut().extend_from_slice(send);
            req.spi_transfer_mut().set_read_buffer(buffer_idx);
            req.spi_transfer_mut().set_start_time(start_time);
            req.spi_transfer_mut().set_transfer_count(transfer_count as u32);
            req.spi_transfer_mut().set_transfer_interval(transfer_inverval);

            let res = self.usb_device.send_request(&req).await?;
        }

        let data = self.fetch_buffer_impl(buffer_idx).await?;

        Ok(data)
    }

    pub async fn enqueue_spi_transfer_timed<'a>(
        &'a self,
        periph_name: &str,
        send: &[u8],
        read_buffer: &str,
        start_time: u32,
        transfer_count: usize,
        transfer_inverval: u32,
    ) -> Result<impl Future<Output = Result<()>> + 'a> {

        let periph_index = self.periph_config(periph_name)?.index();
        let buffer_idx = self.periph_config(read_buffer)?.index();

        let res = {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(periph_index);
            req.spi_transfer_mut().data_mut().extend_from_slice(send);
            req.spi_transfer_mut().set_read_buffer(buffer_idx);
            req.spi_transfer_mut().set_start_time(start_time);
            req.spi_transfer_mut().set_transfer_count(transfer_count as u32);
            req.spi_transfer_mut().set_transfer_interval(transfer_inverval);

            self.usb_device.enqueue_request(&req).await?
        };

        Ok(async move {
            res.await?;
            Ok(())
        })
    }

    pub async fn i2c_transfer(
        &self,
        periph_name: &str,
        address: u8,
        send: &[u8],
        receive: &mut [u8],
    ) -> Result<I2CTransferResult> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.i2c_transfer_mut().set_address(address as u32);
        req.i2c_transfer_mut().write_data_mut().extend_from_slice(send);
        req.i2c_transfer_mut().set_read_count(receive.len() as u32);

        let res = self.usb_device.send_request(&req).await?;

        // TODO: Check in range of 'receive' size.
        let n = res.data_val().len();
        if n != receive.len() {
            return Err(err_msg("Did not receive the requested amount"));
        }

        receive[..n].copy_from_slice(res.data_val());
        Ok(I2CTransferResult {
            transfer_time: res.time()
        })
    }

    pub async fn neopixel_transfer(
        &self,
        periph_name: &str,
        mut index: usize,
        mut data: &[u8],
    ) -> Result<()> {
        let periph_index = self.periph_config(periph_name)?.index();

        let max_bytes_per_request = {
            let mut req = PeripheralRequest::default();            
            req.neopixel_transfer_mut().data_mut().capacity()
        };

        while !data.is_empty() {
            let n = core::cmp::min(max_bytes_per_request, data.len());
            let chunk = &data[0..n];

            // TODO: These can be pipelined.

            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(periph_index);
            req.neopixel_transfer_mut().set_index(index as u32);
            req.neopixel_transfer_mut().data_mut().extend_from_slice(chunk);

            self.usb_device.send_request(&req).await?;

            data = &data[n..];
            index += n;
        }


        Ok(())
    }

    pub async fn neopixel_show(
        &self,
        periph_name: &str,
    ) -> Result<()> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.neopixel_show_mut();

        self.usb_device.send_request(&req).await?;

        Ok(())
    }

    pub async fn gpio_write(
        &self,
        periph_name: &str,
        high: bool
    ) -> Result<()> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_gpio_level_mut().set_high(high);
        self.usb_device.send_request(&req).await?;
        Ok(())
    }

    pub fn gpio_write_request(
        &self,
        periph_name: &str,
        high: bool
    ) -> Result<PeripheralRequest> {
        let periph_index = self.periph_config(periph_name)?.index();
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_gpio_level_mut().set_high(high);
        Ok(req)
    }

    pub async fn gpio_read(
        &self,
        periph_name: &str
    ) -> Result<bool> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.get_gpio_level_mut();
        let res = self.usb_device.send_request(&req).await?;
        Ok(res.uint_val() != 0)
    }

    pub fn gpio_read_request(&self, periph_name: &str) -> Result<PeripheralRequest> {
        let periph_index = self.periph_config(periph_name)?.index();
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.get_gpio_level_mut();
        Ok(req)
    }

    pub async fn poll_gpio_interrupt(&self, periph_name: &str) -> Result<()> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_poll_gpio_interrupt(true);
        let res = self.usb_device.send_request(&req).await?;
        Ok(())
    }

    pub async fn pwm_write(
        &self,
        periph_name: &str,
        v: f32
    ) -> Result<()> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_pwm_mut().set_value(((((1 << 16) - 1) as f32) * v) as u32);
        self.usb_device.send_request(&req).await?;
        Ok(())
    }

    pub async fn calibrate_adc(&self) -> Result<()> {
        let mut req = PeripheralRequest::default();
        req.set_calibrate_adc(true);
        self.usb_device.send_request(&req).await?;
        Ok(())
    }

    /// Returns RPM
    pub async fn read_tachometer(
        &self,
        periph_name: &str
    ) -> Result<f32> {
        let periph_index = self.periph_config(periph_name)?.index();

        let start_time = {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(periph_index);
            req.set_start_tachometer(true);
            let res = self.usb_device.send_request(&req).await?;
            res.uint_val()
        };

        // TODO: Make this customizable based on how long we expect the fan
        // to spin a reasonable number of times.
        executor::sleep(Duration::from_secs(1)).await?;

        let (end_time, cycle_count) = {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(periph_index);
            req.set_end_tachometer(true);
            let res = self.usb_device.send_request(&req).await?;
            (res.time(), res.uint_val())
        };

        // 2 cycles per rotation
        let rotations = (cycle_count as f64) / 2.0;

        let time_delta = ((time_remaining_u32(end_time, start_time) as f64) / 16_000_000.0);

        let rps = rotations / time_delta;

        // Converting from rotations per second to per minute.
        Ok((rps * 60.0) as f32)
    }

    pub async fn read_tachometers(
        &self, periph_names: &[String]
    ) -> Result<Vec<f32>> {
        let mut out = vec![];

        // TODO: Make it more configurable what the max concurrency is on the device based on number of
        // unused timers / GPIOTEs
        for names in periph_names.chunks(3) {

            let mut futures = vec![];
            for name in names {
                futures.push(self.read_tachometer(name));
            }

            let results = common::futures::future::join_all(futures).await;

            for res in results {
                out.push(res?);
            }
        }

        Ok(out)
    }

    pub async fn analog_read(
        &self,
        periph_name: &str, 
    ) -> Result<f32> {
        let periph = self.periph_config(periph_name)?;
        let periph_index = periph.index();

        let config_res = self.config_responses.get(&periph_index)
            .ok_or_else(|| err_msg("No config for ADC"))?;
        if !config_res.has_adc_format() {
            return Err(err_msg("Config missing adc_format"));
        }

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.sample_adc_mut();

        let res = self.usb_device.send_request(&req).await?;

        let v = Self::convert_raw_analog_output((res.uint_val() as u16) as i16, periph, config_res)?;

        // TODO: Also return the time. 
        Ok(v)
    }

    /// Returns the time at which the user defined trigger was hit (if it was hit).
    /// Returns whether or not the window contains any values that hit the user defined trigger. 
    pub async fn analog_read_window(
        &self,
        periph_name: &str,
        buffer_name: &str,
    ) -> Result<AnalogReadWindowResult> {
        self.enqueue_analog_read_window(periph_name, buffer_name).await?.await
    }

    pub async fn enqueue_analog_read_window<'a>(
        &'a self,
        periph_name: &str,
        buffer_name: &str,
    ) -> Result<impl Future<Output = Result<AnalogReadWindowResult>> + 'a> {
        let periph = self.periph_config(periph_name)?;
        let periph_index = periph.index();

        let buffer_index = self.periph_config(buffer_name)?.index();

        let config_res = self.config_responses.get(&periph_index)
            .ok_or_else(|| err_msg("No config for ADC"))?;
        if !config_res.has_adc_format() {
            return Err(err_msg("Config missing adc_format"));
        }
        
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.sample_adc_mut().set_buffer(buffer_index);
        let res = self.usb_device.enqueue_request(&req).await?;

        Ok(async move {
            let res = res.await?;

            Ok(AnalogReadWindowResult {
                sampling_completion_time: res.time(),
                triggered: res.uint_val() != 0
            })
        })
    }

    /// TODO: We should have the window internally marked with which peripheral it can from to
    /// avoid reading another peripheral's data.
    pub async fn analog_fetch_window(
        &self,
        periph_name: &str,
        buffer_name: &str,
    ) -> Result<Vec<f32>> {
        let periph = self.periph_config(periph_name)?;
        let periph_index = periph.index();

        let buffer_index = self.periph_config(buffer_name)?.index();

        let config_res = self.config_responses.get(&periph_index)
            .ok_or_else(|| err_msg("No config for ADC"))?;
        if !config_res.has_adc_format() {
            return Err(err_msg("Config missing adc_format"));
        }

        // TODO: I should know the size of the buffer from the config so I should be able to tell how many requests I need.

        let buf = self.fetch_buffer_impl(buffer_index).await?;

        // tODO: verify multiple of 2

        let mut buf_f32 = vec![];
        for c in buf.chunks(2) {
            let v_i16 = i16::from_le_bytes(*array_ref![c, 0, 2]);
            let v = Self::convert_raw_analog_output(v_i16, periph, config_res)?;
            buf_f32.push(v);
        }

        Ok(buf_f32)
    }

    pub async fn fetch_buffer(&self, buffer_name: &str) -> Result<Vec<u8>> {
        let buffer_index = self.periph_config(buffer_name)?.index();
        self.fetch_buffer_impl(buffer_index).await
    }

    async fn fetch_buffer_impl(&self, buffer_index: u32) -> Result<Vec<u8>> {
        let mut request_queue = vec![];

        let mut buf = vec![];
        loop {
            while request_queue.len() < 4 {
                let mut req = PeripheralRequest::default();
                req.set_peripheral_index(buffer_index);
                req.set_read_buffer(true);
                request_queue.push(self.usb_device.enqueue_request(&req).await?);
            }

            let res = request_queue.remove(0).await?;

            if res.data_val().len() == 0 {
                break;
            }

            buf.extend_from_slice(res.data_val());
        }

        Ok(buf)
    }

    fn convert_raw_analog_output(
        raw: i16,
        periph: &BoardConfig_Peripheral,
        config_res: &PeripheralResponse
    ) -> Result<f32> {
        let mut v = (raw as f32) / config_res.adc_format().units_per_volt();

        if periph.adc().has_calibration() {
            v += periph.adc().calibration().offset();
            v *= periph.adc().calibration().scale();
        }

        if periph.adc().has_resistor_divider() {
            let c = periph.adc().resistor_divider();
            v = electronics::undivide_voltage(
                v,
                c.top_resistor(),
                c.bottom_resistor()
            );
        }

        if periph.adc().has_current_sense_resistor() {
            v = v / periph.adc().current_sense_resistor().resistor_value();
        }

        if periph.adc().has_thermistor() {
            let c = periph.adc().thermistor();
            let r = electronics::undivide_voltage_lower(
                3.3, v, c.pull_up_resistance() 
            );

            let therm = electronics::thermistor_by_name(c.model())
                .ok_or_else(|| err_msg("Unknown thermistor model"))?;

            v = therm.resistance_to_temperature(r)
                .unwrap_or(1000.0);
                // .ok_or_else(|| err_msg("Out of range resistance measurements"))?;
        }

        Ok(v)
    }

    pub async fn recv_radio_packet(&self, buffer_name: &str) -> Result<Vec<u8>> {
        let buffer_index = self.periph_config(buffer_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(buffer_index);
        req.set_recv_radio_packet(true);
        let res = self.usb_device.send_request(&req).await?;

        self.fetch_buffer_impl(buffer_index).await
    }

    pub async fn send_radio_packet(&self, buffer_name: &str, data: &[u8]) -> Result<()> {
        let buffer_index = self.periph_config(buffer_name)?.index();

        let mut batch = vec![];

        batch.push({
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(buffer_index);
            req.set_clear_buffer(true);
            req
        });

        self.send_request_batch(&batch[..]).await?;
        batch.clear();

        for chunk in data.chunks(32) {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(buffer_index);
            req.set_write_buffer(chunk);
            batch.push(req);
        }

        self.send_request_batch(&batch[..]).await?;
        batch.clear();

        batch.push({
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(buffer_index);
            req.set_send_radio_packet(true);
            req
        });

        self.send_request_batch(&batch[..]).await?;
       
        Ok(())
    }


    pub async fn enqueue_stepper_motion(
        &self,
        periph_name: &str,
        motion: StepperMotorMotion
    ) -> Result<()> {
        let periph_index = self.periph_config(periph_name)?.index();
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_enqueue_stepper_motion(motion);
        self.usb_device.send_request(&req).await?;
        Ok(())
    }

    pub fn make_enqueue_stepper_motion(
        &self,
        periph_name: &str,
        motion: StepperMotorMotion
    ) -> Result<PeripheralRequest> {
        let periph_index = self.periph_config(periph_name)?.index();
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_enqueue_stepper_motion(motion);
        Ok(req)
    }

    pub async fn get_stepper_motor_status(&self, periph_name: &str) -> Result<StepperMotorStatus> {
        let req = self.get_stepper_motor_status_request(periph_name)?;
        let res = self.usb_device.send_request(&req).await?;
        if !res.has_stepper_status() {
            return Err(err_msg("No stepper_status returned"));
        }

        Ok(res.stepper_status().clone())
    }

    pub fn get_stepper_motor_status_request(&self, periph_name: &str) -> Result<PeripheralRequest> {
        let periph_index = self.periph_config(periph_name)?.index();
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_get_stepper_motor_status(true);
        Ok(req)
    }

    pub async fn clear_stepper_queue(&self, periph_name: &str) -> Result<u32> {
        let req = self.clear_stepper_queue_request(periph_name)?;
        let res = self.usb_device.send_request(&req).await?;
        Ok(res.uint_val())
    }

    pub fn clear_stepper_queue_request(&self, periph_name: &str) -> Result<PeripheralRequest> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_clear_stepper_queue(true);

        Ok(req)
    }

    pub async fn reset_stepper_motor_queue(&self, periph_name: &str) -> Result<()> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.reset_stepper_motor_mut();
        let res = self.usb_device.send_request(&req).await?;
        Ok(())
    }

}


// TODO: Dedup me.
fn time_remaining_u32(next_time: u32, current_time: u32) -> u32 {
    let mut t = next_time.wrapping_sub(current_time);
    if next_time < current_time {
        t = t.wrapping_add(u32::max_value());
    }

    t
}