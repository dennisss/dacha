
use base_error::*;

/*

cargo run --bin builder -- build //pkg/rpi/pio_rp1:pio_rp1 --config=//pkg/builder/config:rpi64
scp -r -i ~/.ssh/id_cluster built/pkg/rpi/pio_rp1/pio_rp1 cluster-user@10.1.1.3:~/
*/

/*
/usr/include/misc/rp1_pio_if.h

https://github.com/raspberrypi/utils/blob/master/piolib/include/rp1_pio_if.h

Example Program: https://github.com/raspberrypi/utils/blob/master/piolib/examples/pwm.c

The current program is

    .program fast_mirror
    .wrap_target
        mov pins, pins      ; Copy IN pins state directly to OUT pins
    .wrap

*/

pub mod bindings {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

mod ioctl;

use bindings::*;
use ioctl::*;



pub struct PIO {
    pio: file::LocalFile
}

impl PIO {

    pub fn create_pin_forwarder(in_pin: u32, out_pin: u32) -> Result<Self> {
        let mut inst = Self::create()?;

        let state_machine = inst.claim_unused_state_machine()?;

        // println!("Adding program..");
        let program_offset = inst.add_program(&[
            // "MOV PINS, PINS"
            0b101_00000_000_00_000
        ])?;
        // println!("Program offset: {}", program_offset);

        // println!("claiming gpio..");

        // IN
        inst.claim_gpio(in_pin)?;
        inst.set_gpio_dir(state_machine, in_pin, false)?;

        // OUT
        inst.claim_gpio(out_pin)?;
        inst.set_gpio_dir(state_machine, out_pin, true)?;


        let config = StateMachineConfig {
            clkdiv_int: 1,
            clkdiv_frac: 0,

            // Just one instruction so loop it.
            wrap_bottom: program_offset,
            wrap_top: program_offset,

            /// Index of the first IN pin.
            in_base: in_pin,

            /// Index of the first OUT pin.
            out_base: out_pin,

            /// Number of OUT pins.
            out_count: 1,

            set_base: 0,
            set_count: 0,

            sideset_base: 0,
            sideset_count: 0,
        };

        inst.init_state_machine(state_machine, program_offset, &config)?;
    
        inst.enable_state_machine(state_machine)?;

        Ok(inst)
    }

    pub fn create() -> Result<Self> {

        let mut pio = file::LocalFile::open_with_options(
            "/dev/pio0",
            file::LocalFileOpenOptions::new().write(true),
        )?;

        let mut inst = Self {
            pio
        };

        inst.clear_instruction_memory()?;


        Ok(inst)
    }

    fn clear_instruction_memory(&mut self) -> Result<()> {
        unsafe {
            pio_ioc_clear_instr_mem(self.pio.as_raw_fd())?
        };

        Ok(())
    }

    fn claim_unused_state_machine(&mut self) -> Result<u16> {
        let req = bindings::rp1_pio_sm_claim_args {
            mask: 0,
        };

        let sm = unsafe { pio_ioc_sm_claim(self.pio.as_raw_fd(), &req)? } as u16;
        Ok(sm)
    }

    fn add_program(&mut self, program: &[u16]) -> Result<u32> {
        let mut args = rp1_pio_add_program_args::default();
        args.origin = !0; // PIO_ORIGIN_ANY; // #define RP1_PIO_ORIGIN_ANY          ((uint16_t)(~0))
        args.num_instrs = program.len() as u16;
        args.instrs[0..program.len()].copy_from_slice(program);

        Ok(unsafe {
            pio_ioc_add_program(self.pio.as_raw_fd(), &args)
        }? as u32)
    }


    fn claim_gpio(&mut self, pin: u32) -> Result<()> {
        if pin >= RP1_PIO_GPIO_COUNT {
            return Err(err_msg("Invalid pin index"));
        }

        let args = rp1_gpio_set_function_args {
            gpio: (pin as u16),
            fn_: (RP1_GPIO_FUNC_PIO as u16)
        };

        unsafe {
            pio_ioc_gpio_set_function(self.pio.as_raw_fd(), &args)
        }?;

        Ok(())
    }

    fn set_gpio_dir(&mut self, state_machine: u16, pin: u32, is_out: bool) -> Result<()> {
        let mask = 1u32 << pin;

        let args = rp1_pio_sm_set_pindirs_args {
            sm: state_machine,
            dirs: if { is_out } { mask } else { 0 },
            mask,
            rsvd: 0,
        };

        unsafe {
            pio_ioc_sm_set_pindirs(self.pio.as_raw_fd(), &args)
        }?;

        Ok(())
    }

    fn init_state_machine(&mut self, state_machine: u16, program_offset: u32, config: &StateMachineConfig) -> Result<()> {
        let args = rp1_pio_sm_init_args {
            sm: state_machine,
            initial_pc: program_offset as u16,
            config: config.to_raw()
        };

        unsafe {
            pio_ioc_sm_init(self.pio.as_raw_fd(), &args)
        }?;

        Ok(())
    }

    fn enable_state_machine(&mut self, state_machine: u16) -> Result<()> {
        let args = rp1_pio_sm_set_enabled_args {
            mask: 1 << state_machine,
            enable: 1,
            rsvd: 0,
        };
        unsafe { pio_ioc_sm_set_enabled(self.pio.as_raw_fd(), &args)? };
        Ok(())
    }
}

struct StateMachineConfig {

    clkdiv_int: u32,
    clkdiv_frac: u32,

    wrap_bottom: u32,
    wrap_top: u32,

    /// Index of the first IN pin.
    in_base: u32,

    /// Index of the first OUT pin.
    out_base: u32,

    /// Number of OUT pins.
    out_count: u32,

    set_base: u32,
    set_count: u32,

    sideset_base: u32,
    sideset_count: u32,
}

impl StateMachineConfig {
    fn to_raw(&self) -> rp1_pio_sm_config {
        rp1_pio_sm_config {
            clkdiv: (
                (self.clkdiv_int << PROC_PIO_SM0_CLKDIV_INT_LSB) |
                (self.clkdiv_frac << PROC_PIO_SM0_CLKDIV_FRAC_LSB)
            ),
            execctrl: (
                (self.wrap_bottom << PROC_PIO_SM0_EXECCTRL_WRAP_BOTTOM_LSB) |
                (self.wrap_top << PROC_PIO_SM0_EXECCTRL_WRAP_TOP_LSB)
            ),
            shiftctrl: 0,
            pinctrl: (
                (self.in_base << PROC_PIO_SM0_PINCTRL_IN_BASE_LSB) |
                (self.out_base << PROC_PIO_SM0_PINCTRL_OUT_BASE_LSB) |
                (self.out_count << PROC_PIO_SM0_PINCTRL_OUT_COUNT_LSB) |
                (self.set_base << PROC_PIO_SM0_PINCTRL_SET_BASE_LSB) |
                (self.set_count << PROC_PIO_SM0_PINCTRL_SET_COUNT_LSB) |
                (self.sideset_base << PROC_PIO_SM0_PINCTRL_SIDESET_BASE_LSB) |
                (self.sideset_count << PROC_PIO_SM0_PINCTRL_SIDESET_COUNT_LSB)
            ),
        }

    }

}

