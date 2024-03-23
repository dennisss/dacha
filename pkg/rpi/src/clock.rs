use std::sync::Arc;
use std::time::Duration;

use common::errors::*;

use crate::memory::*;
use crate::registers::*;

const CM_PASSWORD: u8 = 0x5a;

const PCM_CLOCK_OFFSET: usize = 0x98;
const PWM_CLOCK_OFFSET: usize = 0xa0;

pub use crate::registers::ClockSource;

pub struct ClockManager {
    mem: Arc<MemoryBlock>,
}

impl ClockManager {
    // TODO: We need to open exclusive access to this peripheral.
    pub fn open() -> Result<Self> {
        let mem = MemoryBlock::open_peripheral(
            CLOCK_MANAGER_PERIPHERAL_OFFSET,
            CLOCK_MANAGER_PERIPHERAL_SIZE,
        )?;

        Ok(Self { mem: Arc::new(mem) })
    }

    /// Gets the rate in Hertz of the oscillator clock
    ///
    /// - Will return 54MHz on a Pi 4B
    /// - Otherwise will return 19.2MHz
    pub async fn oscillator_rate(&self) -> Result<usize> {
        let value =
            file::read("/sys/firmware/devicetree/base/clocks/clk-osc/clock-frequency").await?;
        if value.len() != 4 {
            return Err(err_msg("Unknown clock frequency format"));
        }

        Ok(u32::from_be_bytes(*array_ref![value, 0, 4]) as usize)

        /*
        // This file requires root access to open

        Ok(file::read_to_string("/sys/kernel/debug/clk/osc/clk_rate")
            .await?
            .trim()
            .parse()?)
        */
    }

    // TODO: Limit max time of all loops.
    pub fn configure(&mut self, clock: Clock, source: ClockSource, divi: u16) {
        let offset = match clock {
            Clock::PCM => PCM_CLOCK_OFFSET,
            Clock::PWM => PWM_CLOCK_OFFSET,
        };

        let mut ctl = ClockControl::default();
        ctl.passwd = CM_PASSWORD;
        ctl.kill = true;

        {
            let mut v = vec![];
            ctl.serialize(&mut v).unwrap();
            let v = u32::from_be_bytes(*array_ref![v, 0, 4]);

            self.mem.write_register(offset + 0, v);
        }

        // Wait for the clock to stop
        // println!("Wait for stop");
        loop {
            let ctl = ClockControl::parse(&self.mem.read_register(offset + 0).to_be_bytes())
                .unwrap()
                .0;

            if !ctl.busy {
                break;
            }

            std::thread::sleep(Duration::from_micros(1));
        }

        // println!("=> Done");

        let mut div = ClockDivisor::default();
        div.passwd = CM_PASSWORD;
        div.divi = divi;

        {
            let mut v = vec![];
            div.serialize(&mut v).unwrap();
            let v = u32::from_be_bytes(*array_ref![v, 0, 4]);
            // println!("DIV: {:?}", v);
            self.mem.write_register(offset + 4, v);
        }

        let mut ctl = ClockControl::default();
        ctl.passwd = CM_PASSWORD;
        ctl.mash = MASHControl::IntegerDivision;
        ctl.source = source;

        {
            let mut v = vec![];
            ctl.serialize(&mut v).unwrap();
            let v = u32::from_be_bytes(*array_ref![v, 0, 4]);

            // println!("CTL: {:?}", v);

            self.mem.write_register(offset + 0, v);
        }

        ctl.enable = true;

        {
            let mut v = vec![];
            ctl.serialize(&mut v).unwrap();
            let v = u32::from_be_bytes(*array_ref![v, 0, 4]);

            // println!("CTL: {:?}", v);

            self.mem.write_register(offset + 0, v);
        }

        // Wait for start
        // println!("Wait for start");
        loop {
            let ctl = ClockControl::parse(&self.mem.read_register(offset + 0).to_be_bytes())
                .unwrap()
                .0;

            if ctl.busy {
                break;
            }

            std::thread::sleep(Duration::from_micros(1));
        }

        // println!("=> Done")
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Clock {
    PWM,
    PCM,
}

/*
        Example of stopping a clock:

        ws2811_device_t *device = ws2811->device;
        volatile pcm_t *pcm = device->pcm;
        volatile cm_clk_t *cm_clk = device->cm_clk;

        // Turn off the PCM in case already running
        pcm->cs = 0;
        usleep(10);

        // Kill the clock if it was already running
        cm_clk->ctl = CM_CLK_CTL_PASSWD | CM_CLK_CTL_KILL;
        usleep(10);
        while (cm_clk->ctl & CM_CLK_CTL_BUSY)
            ;


*/

/*
We can get the current oscillator speed in '/sys/kernel/debug/clk/osc/clk_rate' (19.2Mhz on old Pis, but higher on )


- 0x 7e101098 : CM_PCMCTL
- 0x 7e10 109c : CM_PCMDIV
-
- 0x 7e10 10a0 : CM_PWMCTL
- 0x 7e10 10a4 : CM_PWMDIV
*/
