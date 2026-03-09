use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use common::array_ref;
use peripherals_service::device::PeripheralsDevice;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorStatus, TMC2209Config};
use peripherals_proto::peripherals::PeripheralRequest;

use crate::tmc2209::registers::*;
use crate::tmc2209::utils::Register;
use crate::tmc2209::datagrams::*;

const USE_STEALTH_CHOP: bool = false;

pub struct TMC2209Device {
    device: Arc<PeripheralsDevice>,
    config: TMC2209Config,
}

impl TMC2209Device {

    pub async fn create(config: TMC2209Config, device: Arc<PeripheralsDevice>) -> Result<Self> {

        let mut inst = Self {
            device,
            config,
        };

        inst.configure().await?;

        Ok(inst)
    }

    fn default_en_spreadcycle(&self) -> u8 {
        if USE_STEALTH_CHOP {
            0
        } else {
            1
        }
    }

    async fn configure(&mut self) -> Result<()> {
        // Default GCONF: 0100000001
        let mut gconf = self.read_register::<GCONF>().await?;

        // TODO: What should I do about multistep_filt (on by default?
        gconf
        // internal reference (don't use vref input)
        .set_i_scale_analog(0)
        // use the external 110mOhm sense resistors
        .set_internal_rsense(0)
        // disable usage of pdn_uart pin for non-uart stuff
        .set_pdn_disable(1)
        // Use mres register to determine microstepping
        .set_mstep_reg_select(1)
        .set_en_spreadcycle(self.default_en_spreadcycle());

        self.write_register(&gconf).await?;

        let irun = {
            if self.config.irun() != 0 {
                self.config.irun()
            } else {
                16
            }
        } as u8;

        let ihold = {
            if self.config.ihold() != 0 {
                self.config.ihold()
            } else {
                4
            }
        } as u8;

        // For IRUN, values between 16 and 31 are recommended by the TMC2209 datasheet
        // for best microstepping performance.
        self.write_register(
            IHOLD_IRUN::from_raw(0)
            .set_ihold(ihold)
            .set_irun(irun)
            .set_iholddelay(2) // TODO: Tune
        ).await?;

        self.write_register(&VACTUAL::from_raw(0)).await?;

        // Reset value is 0x10000053
        let mut chopperconf = self.read_register::<CHOPCONF>().await?;
        chopperconf
        .set_mres(4) // 2^4 = 16 microsteps per pulse (16 steps per full step)
        .set_dedge(1)
        .set_vsense(0)
        .set_intpol(1) // interpolate to 256 microsteps
        .set_toff(4)
        .set_hstrt(4)
        .set_hend(0)
        .set_tbl(2);
        
        self.write_register(&chopperconf).await?;

        self.write_register(&TPOWERDOWN::from_raw(20)).await?;

        // From the datasheet:
        // "DIAG is pulsed by StallGuard when SG_RESULT falls below SGTHRS. It is only enabled in StealthChop
        // mode, and when TCOOLTHRS ≥ TSTEP > TPWMTHRS"

        // Always use StealthChop even at high speed.
        // This ensures StallGuard will fire on the DIAG pin.
        self.write_register(&TPWMTHRS::from_raw(0)).await?;

        // This is basically the maximum value of TSTEP at which StallGuard will work.
        // (basically disable stallguard at low speeds)
        //
        // Setting an very large value for this tends to lead to 
        self.write_register(&TCOOLTHRS::from_raw(600 /* (1 << 20) - 1*/)).await?;

        // SG_RESULT <= 2*10 will trigger DIAG to fire
        // TODO: Probably we want one value for motion and one for homing.
        self.write_register(&SGTHRS::from_raw(50)).await?;

        // Disable CoolStep
        self.write_register(&COOLCONF::from_raw(0)).await?;

        // let drv_status = stepper.read_register::<DRV_STATUS>().await?;
        // println!("DRV_STATUS: {:032b?}", drv_status);
        

        Ok(())
    }

    pub async fn enable_homing_mode(&self, enabled: bool) -> Result<()> {

        for i in 0..3 {
            match self.try_enable_homing_mode(enabled).await {
                Ok(()) => {
                    return Ok(())
                }
                Err(e) => {
                    eprintln!("Failed to change homing mode: (attempt: {}) {}", i, e);
                    executor::sleep(Duration::from_millis(200)).await?;
                }
            }
        }

        Err(err_msg("Ran out of tried while setting homing mode"))
    }

    async fn try_enable_homing_mode(&self, enabled: bool) -> Result<()> {
        let mut gconf = self.read_register::<GCONF>().await?;

        // TODO: What should I do about multistep_filt (on by default?
        gconf
        .set_en_spreadcycle(if enabled { 0 } else { self.default_en_spreadcycle() });

        self.write_register(&gconf).await?;

        Ok(())
    }

    pub fn device_name(&self) -> &str {
        self.config.device_name()
    }

    pub async fn read_register<Reg: Register>(&self) -> Result<Reg> {
        let mut raw = self.read_register_raw(Reg::addr()).await?;
        Ok(Reg::from_raw(raw))
    }

    async fn read_register_raw(&self, register_addr: u8) -> Result<u32> {
        // println!("READ: {} on {}", register_addr, self.config.addr());

        let mut out = [0u8; 8];

        let n = self.device.uart_transfer(
            self.config.uart_peripheral(),
            &create_tmc2209_read_request(self.config.addr() as u8, register_addr),
            &mut out[..]
        ).await?;

        if n != out.len() {
            return Err(format_err!("No complete response. Received: {} of {}", n, out.len()));
        }

        parse_tmc2209_read_reply(&out, register_addr)
    }

    // TODO: Find a better way to make this require '&mut self'
    pub async fn write_register<Reg: Register>(&self, reg: &Reg) -> Result<()> {
        self.write_register_raw(Reg::addr(), reg.to_raw()).await
    }

    async fn write_register_raw(&self, register_addr: u8, value: u32) -> Result<()> {

        let ifcnt = self.read_register::<IFCNT>().await?.ifcnt();

        // println!("WRITE: {} on {}", register_addr, self.config.addr());

        self.device.uart_transfer(
            self.config.uart_peripheral(),
            &create_tmc2209_write_request(self.config.addr() as u8, register_addr, value),
            &mut []
        ).await?;

        let ifcnt_end = self.read_register::<IFCNT>().await?.ifcnt();
        if ifcnt_end != ifcnt.wrapping_add(1) {
            return Err(err_msg("Bad IFCNT. Some writes lost."));
        }

        Ok(())
    }

    pub async fn enable(&self) -> Result<()> {
        self.device.gpio_write(self.config.enable_peripheral(), false).await?;
        Ok(())
    }

    pub async fn disable(&self) -> Result<()> {
        self.device.gpio_write(self.config.enable_peripheral(), true).await?;
        Ok(())
    }

    pub fn disable_request(&self) -> Result<PeripheralRequest> {
        self.device.gpio_write_request(self.config.enable_peripheral(), true)
    }

    pub async fn enqueue_stepper_motion(
        &self,
        motion: StepperMotorMotion
    ) -> Result<()> {
        self.device.enqueue_stepper_motion(self.config.step_peripheral(), motion).await
    }

    pub fn make_enqueue_stepper_motion(
        &self,
        motion: StepperMotorMotion
    ) -> Result<PeripheralRequest> {
        self.device.make_enqueue_stepper_motion(self.config.step_peripheral(), motion)
    }

    pub async fn get_stepper_motor_status(&self) -> Result<StepperMotorStatus> {
        self.device.get_stepper_motor_status(self.config.step_peripheral()).await
    }

    pub fn get_stepper_motor_status_request(&self) -> Result<PeripheralRequest> {
        self.device.get_stepper_motor_status_request(self.config.step_peripheral())
    }

    pub async fn reset_stepper_motor_queue(&self) -> Result<()> {
        self.device.reset_stepper_motor_queue(self.config.step_peripheral()).await
    }

    pub async fn sg_result(&self) -> Result<u16> {
        let r = self.read_register::<SG_RESULT>().await?;
        Ok(r.sg_result())
    }

    pub async fn tstep(&self) -> Result<u32> {
        let r = self.read_register::<TSTEP>().await?;
        Ok(r.tstep())
    }

    pub async fn clear_stepper_queue(&self) -> Result<u32> {
        self.device.clear_stepper_queue(self.config.step_peripheral()).await
    }

    pub fn clear_stepper_queue_request(&self) -> Result<PeripheralRequest> {
        self.device.clear_stepper_queue_request(self.config.step_peripheral())
    }

    /*
    /*
    let tstep = self.read_register::<tmc2209::TSTEP>().await?.to_raw();
    println!("TSTEP: {}", tstep);


    // TODO: For good measure,
    {
        let mut req = PeripheralRequest::default();
        req.set_peripheral_index(diag_periph_index);
        req.set_poll_gpio_interrupt(true);
        let res = self.usb_device.send_request(&req).await?;

        if res.uint_val() != 0 {
            println!("DIAG FIRED");
            running = false;
            break;
        }

    }

    */
    */
    
}

