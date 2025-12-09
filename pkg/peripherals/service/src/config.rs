use std::collections::{HashMap, HashSet};

use base_error::*;
use peripherals_proto::peripherals::*;
use file::project_path;

fn names_iter(pin: &BoardConfig_Pin) -> impl Iterator<Item = &str> {
    core::iter::once(pin.name()).chain(pin.alias().iter().map(|s| s.as_str()))
}

fn peripheral_pins_mut<F: FnMut(&str, &mut u32) -> Result<()>>(
    periph: &mut BoardConfig_Peripheral,
    mut callback: F
) -> Result<()> {
    match periph.config_case_mut() {
        BoardConfig_PeripheralConfigCase::Gpio(config) => {
            let name = config.pin_name().to_string();
            callback(&name, config.config_mut().pin_mut())?;
        }
        BoardConfig_PeripheralConfigCase::Pwm(config) => {
            let name = config.pin_name().to_string();
            callback(&name, config.config_mut().pin_mut())?;
        }
        BoardConfig_PeripheralConfigCase::Neopixel(config) => {
            let name = config.pin_name().to_string();
            callback(&name, config.config_mut().pin_mut())?;
        }
        BoardConfig_PeripheralConfigCase::Uart(config) => {
            let name = config.tx_pin_name().to_string();
            callback(&name, config.config_mut().tx_pin_mut())?;

            let name = config.rx_pin_name().to_string();
            callback(&name, config.config_mut().rx_pin_mut())?;
        }
        BoardConfig_PeripheralConfigCase::Stepper(config) => {
            let name = config.step_pin_name().to_string();
            callback(&name, config.config_mut().step_pin_mut())?;

            let name = config.dir_pin_name().to_string();
            callback(&name, config.config_mut().dir_pin_mut())?;
        }
        BoardConfig_PeripheralConfigCase::Adc(config) => {
            let name = config.pin_name().to_string();
            callback(&name, config.config_mut().pin_mut())?;

            if !config.negative_pin_name().is_empty() {
                let name = config.negative_pin_name().to_string();
                callback(&name, config.config_mut().negative_pin_mut())?;
            }
        }
        BoardConfig_PeripheralConfigCase::Buffer(_) => {}
        BoardConfig_PeripheralConfigCase::Spi(config) => {
            let name = config.mosi_pin_name().to_string();
            callback(&name, config.config_mut().mosi_pin_mut())?;

            let name = config.miso_pin_name().to_string();
            callback(&name, config.config_mut().miso_pin_mut())?;

            let name = config.cs_pin_name().to_string();
            callback(&name, config.config_mut().cs_pin_mut())?;

            let name = config.sclk_pin_name().to_string();
            callback(&name, config.config_mut().sclk_pin_mut())?;
        }
        BoardConfig_PeripheralConfigCase::I2c(config) => {
            let name = config.scl_pin_name().to_string();
            callback(&name, config.config_mut().scl_pin_mut())?;

            let name = config.sda_pin_name().to_string();
            callback(&name, config.config_mut().sda_pin_mut())?;
        }
        BoardConfig_PeripheralConfigCase::NOT_SET => {
            return Err(err_msg("Unconfigured peripheral"));
        }
    };

    Ok(())
}

/// 
pub fn compile_board_config(configs: &[&BoardConfig]) -> Result<BoardConfig> {
    let mut out = BoardConfig::default();

    // Pin name to the index of the pin in out.pins()
    let mut pins_by_name: HashMap<&str, usize> = HashMap::new();

    // Set of all MCU pin indexes defined. 
    let mut pin_numbers: HashSet<u32> = HashSet::new();

    let mut assigned_pin_numbers: HashSet<u32> = HashSet::new();

    let mut peripheral_names: HashMap<&str, usize> = HashMap::new();

    for config in configs {
        // TODO: Make merging of these simple fields more scalable.
        if config.product_id() != 0 {
            out.set_product_id(config.product_id());
        }


        for pin in config.pins() {
            let mut existing_pin_i = None;

            for name in names_iter(pin) {
                if name.is_empty() {
                    return Err(err_msg("Pin has an empty name"));
                }
                
                if let Some(i) = pins_by_name.get(name) {
                    existing_pin_i = Some(*i);
                    break;
                }
            }

            if let Some(pin_i) = existing_pin_i {
                let existing_pin = &mut out.pins_mut()[pin_i];
                if pin.has_index() && existing_pin.index() != pin.index() {
                    return Err(err_msg("Conflicting pin indexes"));
                }

                // Mark all aliases
                for name in names_iter(pin) {
                    if let Some(i) = pins_by_name.get(name).cloned() {
                        if i != pin_i {
                            return Err(err_msg("Conclicting pin names"));
                        }
                    } else {
                        pins_by_name.insert(name, pin_i);
                        existing_pin.add_alias(name.to_string());
                    }
                }

                // TODO: Make the new name the main name of the 
                let old_name = existing_pin.name().to_string();
                existing_pin.set_name(pin.name());
                existing_pin.alias_mut().retain(|n| n != pin.name());
                existing_pin.add_alias(old_name);

            } else {
                if !pin.has_index() {
                    return Err(format_err!("Pin '{}' missing index", pin.name()));
                }

                if !pin_numbers.insert(pin.index()) {
                    return Err(err_msg("Duplicate pin definition"));
                }

                let pin_i = out.pins().len();
                out.add_pins(pin.as_ref().clone());

                for name in names_iter(pin) {
                    if pins_by_name.insert(name, pin_i).is_some() {
                        return Err(err_msg("Duplicate pin name"));
                    }
                }
            }
        }

        for periph in config.peripherals() {
            let periph_i = out.peripherals().len();
            if peripheral_names.insert(periph.name(), periph_i).is_some() {
                return Err(err_msg("Duplicate peripheral name"));
            }

            let mut periph = periph.as_ref().clone();

            peripheral_pins_mut(&mut periph, |pin_name, pin_num| {
                let pin_i = *pins_by_name.get(pin_name)
                    .ok_or_else(|| format_err!("No pin named: {}", pin_name))?;

                *pin_num = out.pins()[pin_i].index();
                
                if !assigned_pin_numbers.insert(*pin_num) {
                    return Err(err_msg("Pin assigned to multiple roles or peripherals"));
                }

                Ok(())
            })?;

            if periph.adc().has_resistor_divider() {
                let divider = periph.adc().resistor_divider();
                let max_output_voltage = electronics::divide_voltage(
                    divider.max_input_voltage(),
                    divider.top_resistor(),
                    divider.bottom_resistor()
                );

                periph.adc_mut().config_mut().set_max_voltage(max_output_voltage);
            }

            if periph.adc().has_current_sense_resistor() {
                let c = periph.adc().current_sense_resistor();
                let max_voltage = c.max_current() * c.resistor_value();

                periph.adc_mut().config_mut().set_max_voltage(max_voltage);
            }

            if periph.adc().has_thermistor() {
                let c = periph.adc().thermistor();
                let therm = electronics::thermistor_by_name(c.model())
                    .ok_or_else(|| format_err!("Unknown thermistor model: {}", c.model()))?;

                let r0 = therm.temperature_to_resistance(c.min_temp())
                    .ok_or_else(|| format_err!("Temp {} can't be used", c.min_temp()))?;
                let r1 = therm.temperature_to_resistance(c.max_temp())
                    .ok_or_else(|| format_err!("Temp {} can't be used", c.max_temp()))?;

                let v0 = electronics::divide_voltage(3.3, c.pull_up_resistance(), r0);
                let v1 = electronics::divide_voltage(3.3, c.pull_up_resistance(), r1);

                periph.adc_mut().config_mut().set_max_voltage(v0.max(v1));
            }


            periph.set_index(periph_i as u32);
            out.add_peripherals(periph);
        }

        for m in config.macros() {
            let mut m = m.as_ref().clone();

            // TODO: Validate commands make sense for the peripheral types (e.g. run simulation of state changes)
            for cmd in m.commands_mut() {
                let periph_i = *peripheral_names.get(cmd.peripheral_name())
                    .ok_or_else(|| err_msg("Unknown peripheral"))?;
                cmd.request_mut().set_peripheral_index(periph_i as u32);
            } 


            out.add_macros(m);
        }
    }

    Ok(out)
}

pub fn build_configuration_requests(config: &BoardConfig) -> Result<(Vec<PeripheralRequest>, PeripheralsState)> {
    let mut out = vec![];
    let mut state = PeripheralsState::default();

    out.push({
        let mut req = PeripheralRequest::default();
        req.unconfigure_all_mut();
        req
    });

    for periph in config.peripherals() {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(periph.index() as u32);

        let s = state.new_states();
        s.set_index(periph.index());

        match periph.config_case() {
            BoardConfig_PeripheralConfigCase::Gpio(config) => {
                req.set_configure_gpio(config.config().clone());

                if config.has_tachometer() {
                    s.tachometer_mut();

                } else {
                    s.gpio_mut().set_high(config.config().default_value());
                }
            }
            BoardConfig_PeripheralConfigCase::Pwm(config) => {
                req.set_configure_pwm(config.config().clone());
                s.pwm_mut().set_value(config.config().default_value());
            }
            BoardConfig_PeripheralConfigCase::Uart(config) => {
                req.set_configure_uart(config.config().clone());
            }
            BoardConfig_PeripheralConfigCase::Neopixel(config) => {
                req.set_configure_neopixel(config.config().clone());
            }
            BoardConfig_PeripheralConfigCase::Stepper(config) => {
                req.set_configure_stepper(config.config().clone());
            }
            BoardConfig_PeripheralConfigCase::Adc(config) => {
                req.set_configure_adc(config.config().clone());
                s.adc_mut();
            }
            BoardConfig_PeripheralConfigCase::Buffer(config) => {
                req.set_allocate_buffer(config.config().clone());
            }
            BoardConfig_PeripheralConfigCase::I2c(config) => {
                req.set_configure_i2c(config.config().clone());
            }
            BoardConfig_PeripheralConfigCase::Spi(config) => {
                req.set_configure_spi(config.config().clone());
            }
            BoardConfig_PeripheralConfigCase::NOT_SET => {
                return Err(err_msg("Unconfigured peripheral"));
            }
        };

        out.push(req);
    }

    out.push({
        let mut req = PeripheralRequest::default();
        req.finalize_config_mut();
        req
    });

    Ok((out, state))
}

pub fn apply_state_change(request: &PeripheralRequest, state: &mut PeripheralsState) -> Result<()> {
    let s: &mut PeripheralState = state.states_mut().iter_mut().find(|s| s.index() == request.peripheral_index())
        .map(|s| s.as_mut())    
        .ok_or_else(|| err_msg("No peripheral with the given index"))?;
    
    match s.state_case_mut() {
        PeripheralStateStateCase::NOT_SET => {}
        PeripheralStateStateCase::Gpio(s) => {
            match request.command_case() {
                PeripheralRequestCommandCase::SetGpioLevel(v) => {
                    *s.as_mut() = v.as_ref().clone();
                }
                _ => {}
            }
        }
        PeripheralStateStateCase::Pwm(s) => {
            match request.command_case() {
                PeripheralRequestCommandCase::SetPwm(v) => {
                    *s.as_mut() = v.as_ref().clone();
                }
                _ => {}
            }
        }
        PeripheralStateStateCase::Adc(_) => {

        }
        PeripheralStateStateCase::Tachometer(_) => {

        }
    }

    Ok(())
}


pub fn nrf52840_config() -> BoardConfig {
    let mut config = BoardConfig::default();
    config.set_name("nrf52840");

    // Port 0 has 32 pins.
    // Port 1 has 16 pins.
    for i in 0..(32 + 16) {
        let port_num = i / 32;
        let port_pin = i % 32;

        let pin = config.new_pins();
        pin.set_name(format!("P{}.{:02}", port_num, port_pin));
        pin.set_index(i as u32);
    }

    config
}

pub struct BoardConfigRegistry {
    configs: HashMap<String, BoardConfig>,
}

impl BoardConfigRegistry {
    pub async fn defaults() -> Result<Self> {
        let mut raw_configs = vec![];
        raw_configs.push(nrf52840_config());

        let dir = project_path!("pkg/peripherals/config/boards");
        for entry in file::read_dir(&dir)? {
            // TODO: Switch to a glob
            if !entry.name().ends_with(".txtpb") {
                continue;
            }

            let data: String = file::read_to_string(&dir.join(entry.name())).await?;

            let mut preset = BoardConfig::default();
            protobuf::text::parse_text_proto(&data, &mut preset)
                .map_err(|e| format_err!("While trying to load: {}; {}", entry.name(), e))?;

            raw_configs.push(preset);
        }

        let mut raw_config_map = HashMap::new();
        for config in &raw_configs {
            if raw_config_map.insert(config.name(), config).is_some() {
                return Err(err_msg("Duplicate config"));
            }
        }

        let mut out = HashMap::new();

        for config in &raw_configs {
            let mut config_chain = vec![ config ];

            let mut visited = HashSet::new();
            visited.insert(config.name());

            let mut base_config = config.base_config();

            while !base_config.is_empty() {
                if !visited.insert(base_config) {
                    return Err(err_msg("Cycle in resolving config"));
                }

                let config = *raw_config_map.get(base_config)
                    .ok_or_else(|| err_msg("base_config not found"))?;
                config_chain.push(config);

                base_config = config.base_config();
            }

            config_chain.reverse();

            out.insert(config.name().to_string(), compile_board_config(&config_chain)?);
        }

        Ok(Self {
            configs: out
        })
    }

    pub fn remove(&mut self, name: &str) -> Option<BoardConfig> {
        self.configs.remove(name)
    }

    pub fn compile(&self, config: &BoardConfig) -> Result<BoardConfig> {
        let mut config_chain = vec![];
        if !config.base_config().is_empty() {
            config_chain.push(self.configs.get(config.base_config())
                .ok_or_else(|| err_msg("base_config not found"))?);
        }

        config_chain.push(config);

        compile_board_config(&config_chain)
    }

}



/*

    let pwm_pins: Vec<u32> = vec![12, 26, 32 + 8, 24];

    let tachometer_pins: Vec<u32> = vec![11, 4, 7, 28, 14, 16, 25, 20];

    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(0 as u32);
        req.unconfigure_all_mut();
        dev.send_request(&req).await?;
    }

    for (i, pin) in pwm_pins.iter().cloned().enumerate() {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(i as u32);
        req.configure_pwm_mut();
        req.configure_pwm_mut().set_pin(pin);
        req.configure_pwm_mut().set_inverted(true);
        req.configure_pwm_mut()
            .set_default_value(((u16::MAX as f32) * 0.8) as u32);
        req.configure_pwm_mut().set_frequency(25000 as u32);
        req.configure_pwm_mut().set_timeout_millis(10000 as u32);
        dev.send_request(&req).await?;
    }

    for (i, pin) in tachometer_pins.iter().cloned().enumerate() {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index((pwm_pins.len() + i) as u32);
        req.configure_gpio_mut().set_is_input(true);
        req.configure_gpio_mut().set_pin(pin);
        req.configure_gpio_mut().set_pull_up(true);
        dev.send_request(&req).await?;
    }

    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(0 as u32);
        req.finalize_config_mut();
        dev.send_request(&req).await?;
    }

    println!("===");

    /*
    TODO: For some reason, sending exactly 9 bytes breaks things.

    */

    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(0 as u32);
        req.set_measure_mcu_temperature(true);
        dev.send_request(&req).await?;
    }

    loop {
        println!("CYCLE ===");

        for i in 0..pwm_pins.len() {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(i as u32);
            req.set_pwm_mut()
                .set_value(((((1 << 16) - 1) as f32) * (0.5)) as u32);
            dev.send_request(&req).await?;
        }

        let mut samples = vec![];
        for i in 0..tachometer_pins.len() {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index((pwm_pins.len() + i) as u32);
            req.read_tachometer_mut();
            let mut res = dev.send_request(&req).await?;
            samples.push(res.uint_val());
        }

        println!("Speed: {:?}", samples);

        executor::sleep(Duration::from_secs(2)).await;
    }
*/

