use common::register::RegisterWrite;
use peripherals::raw::power::POWER;
use peripherals::raw::power::ram::power::POWER_VALUE;

/*
RAM layout of an nRF52840:
- overall 256Kb
- slaves 0-7 have 2 x 4KB sections
- slave  8 has 6 x 32KB sections

Minimal infill with 1.6mm wall thickness.
*/


pub const RAM_START_ADDRESS: u32 = 0x20000000;


macro_rules! const_for {
    (for $i:ident in $s:literal .. $e:literal $body:expr) => {{
        let mut $i = $s;
        loop {
            if $i >= $e {
                break;
            }

            $body

            $i += 1;
        }
    }}
}


#[derive(Default, Clone, Copy)]
struct RAMSection {
    slave: usize,
    section: usize,
    /// Start offset of this RAM chunk relative to the start RAM address (first section at 0) 
    start: u32,
    size: u32,
}

const NUM_RAM_SECTIONS: usize = 8 * 2 + 6;

const RAM_SECTIONS: [RAMSection; NUM_RAM_SECTIONS] = {

    let mut sections = [RAMSection { slave: 0, section: 0, start: 0, size: 0 }; NUM_RAM_SECTIONS];

    let mut i = 0;
    let mut start = 0;

    const_for!(for slave in 0..8 {
        const_for!(for section in 0..2 {
            let size = 4 * 1024;
            sections[i] = RAMSection {
                slave,
                section,
                start,
                size
            };
            start += size;
            i += 1;
        });
    });

    const_for!(for section in 0..6 {
        let size = 32 * 1024;
        sections[i] = RAMSection {
            slave: 8,
            section,
            start,
            size,
        };
        start += size;
        i += 1;
    });

    // assert_eq!(i, sections.len());

    sections
};


/// Configures how much RAM should be retained when in the System ON state.
///
/// - 'start': First offset (starting at 0) in RAM that should be retained.
/// - 'amount' is the number of bytes that should be retained after 'start'
pub fn configure_retained_ram(start: u32, amount: u32, power: &mut POWER) {
    for slave in 0..power.ram.len() {

        let mut power_reg: u32 = 0;

        for section in RAM_SECTIONS {
            if section.slave != slave {
                continue;
            }

            if amount > section.start {
                power_reg |= 1 << section.section;
            }
        }

        power.ram[slave].power.write(POWER_VALUE::from_raw(power_reg));
    }
}