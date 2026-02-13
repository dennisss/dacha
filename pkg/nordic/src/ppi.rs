use common::register::RegisterRead;
use common::register::RegisterWrite;
use peripherals::raw::ppi::chenset::CHENSET_WRITE_VALUE;
use peripherals::raw::ppi::chenclr::CHENCLR_WRITE_VALUE;
use peripherals::raw::ppi::PPI;
use peripherals::raw::EventRegister;
use peripherals::raw::Interrupt;
use peripherals::raw::TaskRegister;

/// NOTE: The end of the CH array is full of fixed event->task mappings.
const NUM_USER_PROGRAMABLE_CHANNELS: usize = 20;

pub struct PPIChannels {
    periph: PPI
}

impl PPIChannels {
    pub fn new(periph: PPI) -> Self {
        Self { periph }
    }

    /// Note that the channel starts out disabled.
    pub fn new_channel(
        &mut self,
        event: &EventRegister,
        task: &mut TaskRegister
    ) -> Option<PPIChannel> {
        let mut index = None;
        for i in 0..NUM_USER_PROGRAMABLE_CHANNELS {
            if self.periph.ch[i].eep.read() == 0 {
                index = Some(i);
                break;
            }
        }

        let index = match index {
            Some(v) => v,
            None => return None
        };

        // Trigger a STEP GPIO toggle on the CC register's COMPARE event.
        self.periph.ch[index].eep.write(unsafe {
            core::mem::transmute::<&EventRegister, u32>(event)
        });
        self.periph.ch[index].tep.write(unsafe {
            core::mem::transmute::<&mut TaskRegister, u32>(task)
        });

        Some(PPIChannel {
            index,
            mask: 1 << index
        })
    }
}


pub struct PPIChannel {
    index: usize,
    mask: u32,
}

impl Drop for PPIChannel {
    fn drop(&mut self) {
        self.disable();
        
        let mut periph = unsafe { PPI::new() };
        periph.ch[self.index].eep.write(0);
    }
}

impl PPIChannel {
    pub fn enable(&mut self) {
        let mut periph = unsafe { PPI::new() };
        periph
            .chenset
            .write(CHENSET_WRITE_VALUE::from_raw(self.mask));
    }

    pub fn disable(&mut self) {
        let mut periph = unsafe { PPI::new() };
        periph
            .chenclr
            .write(CHENCLR_WRITE_VALUE::from_raw(self.mask));
    }

    pub fn set_fork_task(&mut self, task: &mut TaskRegister) {
        let mut periph = unsafe { PPI::new() };
        periph.fork[self.index].tep.write(unsafe {
            core::mem::transmute::<&mut TaskRegister, u32>(task)
        });
    }
}
