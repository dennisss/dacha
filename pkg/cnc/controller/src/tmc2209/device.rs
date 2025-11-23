use std::sync::Arc;

use common::errors::*;
use common::array_ref;
use peripherals_service::device::PeripheralsDevice;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorStatus, TMC2209Config};
use peripherals_proto::peripherals::PeripheralRequest;

use crate::tmc2209::registers::*;
use crate::tmc2209::utils::Register;
use crate::tmc2209::datagrams::*;


pub struct TMC2209Device {
    device: Arc<PeripheralsDevice>,
    config: TMC2209Config,
    ifcnt: u8
}

impl TMC2209Device {

    pub async fn create(config: TMC2209Config, device: Arc<PeripheralsDevice>) -> Result<Self> {

        let mut inst = Self {
            device,
            config,
            ifcnt: 0
        };

        inst.ifcnt = inst.read_register::<IFCNT>().await?.ifcnt();

        // Default GCONF: 0100000001
        let mut gconf = inst.read_register::<GCONF>().await?;

        // TODO: What should I do about multistep_filt (on by default?
        gconf
        // internal reference (don't use vref input)
        .set_i_scale_analog(1)
        // use the external 110mOhm sense resistors
        .set_internal_rsense(0)
        // disable usage of pdn_uart pin for non-uart stuff
        .set_pdn_disable(1)
        // Use mres register to determine microstepping
        .set_mstep_reg_select(1)
        .set_en_spreadcycle(0);

        inst.write_register(&gconf).await?;

        inst.write_register(
            IHOLD_IRUN::from_raw(0)
            .set_ihold(16)
            .set_irun(16)
            .set_iholddelay(15) // TODO: Tune
        ).await?;


        // Reset value is 0x10000053
        let mut chopperconf = inst.read_register::<CHOPCONF>().await?;
        chopperconf
        .set_mres(4) // 2^4 = 16 microsteps per pulse (16 steps per full step)
        .set_dedge(1);
        
        inst.write_register(&chopperconf).await?;


        // Always use StealthChop even at high speed.
        // This ensures StallGuard will fire on the DIAG pin.
        inst.write_register(&TPWMTHRS::from_raw(0)).await?;

        // This is basically the maximum value of TSTEP at which StallGuard will work.
        // (basically disable stallguard at low speeds)
        inst.write_register(&TCOOLTHRS::from_raw(/* (1 << 20) - 1 */ 600)).await?;

        // SG_RESULT <= 2*10 will trigger DIAG to fire
        inst.write_register(&SGTHRS::from_raw(50)).await?;

        // Disable CoolStep
        inst.write_register(&COOLCONF::from_raw(0)).await?;


        // TODO: Should I explicitly disable COOLCONG

        /*
        Stallguard stuff:

        - SGTHRS
        - 

        "DIAG is pulsed by StallGuard when SG_RESULT falls below SGTHRS. It is only enabled in StealthChop
            mode, and when TCOOLTHRS ≥ TSTEP > TPWMTHRS"

        */


        // TPOWERDOWN
        // TODO: Need to validate range of input registers

        /*
        stepper.write_register(
            TPOWERDOWN::addr(),
            20
        ).await?;

        /*
        (defaulti s 0xC10D0024)

        PWMCONF
        set pwm_autoscale,
        set pwm_autograd


        PWMCONF
        select PWM_FREQ with
        regard to fCLK for 20-
        40kHz PWM frequency


        CHOPCONF
        Enable chopper using basic
        config., e.g.: TOFF=5, TBL=2,
        HSTART=4, HEND=0



        */

        */


        // TODO: Auto validate this.
        let ifcnt_end = inst.read_register::<IFCNT>().await?.ifcnt();
        if ifcnt_end != inst.ifcnt {
            return Err(format_err!("Bad IFCNT. Some writes lost: {} vs {}", ifcnt_end, inst.ifcnt));
        }
        // let drv_status = stepper.read_register::<DRV_STATUS>().await?;
        // println!("DRV_STATUS: {:032b?}", drv_status);
        

        Ok(inst)
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
            return Err(err_msg("No complete response"));
        }

        parse_tmc2209_read_reply(&out, register_addr)
    }

    pub async fn write_register<Reg: Register>(&mut self, reg: &Reg) -> Result<()> {
        self.write_register_raw(Reg::addr(), reg.to_raw()).await
    }

    async fn write_register_raw(&mut self, register_addr: u8, value: u32) -> Result<()> {
        // println!("WRITE: {} on {}", register_addr, self.config.addr());

        self.device.uart_transfer(
            self.config.uart_peripheral(),
            &create_tmc2209_write_request(self.config.addr() as u8, register_addr, value),
            &mut []
        ).await?;

        self.ifcnt = self.ifcnt.wrapping_add(1);

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

