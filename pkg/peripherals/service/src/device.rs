use std::collections::HashMap;

use common::errors::*;
use nordic_tools::usb_radio::{USBRadio, ClockTimeResponse};
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
            config_responses.insert(req.peripheral_index(), res);
        }

        Ok((Self {
            config: config.clone(),
            usb_device,
            config_responses
        }, peripherals_state))
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
        /*
        let mut req = PeripheralRequest::default();
        req.set_get_clock_time(true);
        let res = self.usb_device.send_request(&req).await?;
        Ok(res.uint_val())
        */
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

    pub async fn neopixel_transfer(
        &self,
        periph_name: &str,
        data: &[u8],
    ) -> Result<()> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.neopixel_transfer_mut().data_mut().extend_from_slice(data);
        
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

    
    /*
    /// Returns RPM
    pub async fn read_tachometer(
        &self,
        periph_name: &str
    ) -> Result<f32> {
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.get_gpio_level_mut();
        let res = self.usb_device.send_request(&req).await?;
        Ok(res.uint_val() != 0)
    }
    */


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

        Ok(v)
    }

    /// Returns the time at which the user defined trigger was hit (if it was hit).
    /// Returns whether or not the window contains any values that hit the user defined trigger. 
    pub async fn analog_read_window(&self, periph_name: &str) -> Result<Option<u32>> {
        let periph = self.periph_config(periph_name)?;
        let periph_index = periph.index();

        let config_res = self.config_responses.get(&periph_index)
            .ok_or_else(|| err_msg("No config for ADC"))?;
        if !config_res.has_adc_format() {
            return Err(err_msg("Config missing adc_format"));
        }
        
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.sample_adc_mut().set_window(true);
        let res = self.usb_device.send_request(&req).await?;
        Ok(if res.uint_val() != 0 { Some(res.uint_val()) } else { None })
    }

    /// TODO: We should have the window internally marked with which peripheral it can from to
    /// avoid reading another peripheral's data.
    pub async fn analog_fetch_window(&self, periph_name: &str) -> Result<Vec<f32>> {
        let periph = self.periph_config(periph_name)?;
        let periph_index = periph.index();

        let config_res = self.config_responses.get(&periph_index)
            .ok_or_else(|| err_msg("No config for ADC"))?;
        if !config_res.has_adc_format() {
            return Err(err_msg("Config missing adc_format"));
        }

        let mut buf = vec![];
        loop {
            let mut req = PeripheralRequest::default();
            req.set_read_adc_buffer(buf.len() as u32);

            let res = self.usb_device.send_request(&req).await?;
            if res.data_val().len() == 0 {
                break;
            }

            buf.extend_from_slice(res.data_val());
        }

        // tODO: verify multiple of 2

        let mut buf_f32 = vec![];
        for c in buf.chunks(2) {
            let v_i16 = i16::from_le_bytes(*array_ref![c, 0, 2]);
            let v = Self::convert_raw_analog_output(v_i16, periph, config_res)?;
            buf_f32.push(v);
        }

        Ok(buf_f32)
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
        let periph_index = self.periph_config(periph_name)?.index();

        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph_index);
        req.set_clear_stepper_queue(true);
        let res = self.usb_device.send_request(&req).await?;
        Ok(res.uint_val())
    }

}
