
/*
Mapped to memory:
Channel 1: FF10 - FF14
Channel 2: FF16 - FF19
Channel 3: FF1A - FF1E
Channel 4: FF20 - FF23
Registers: FF24 - FF26
*/

use gameboy::memory::MemoryInterface;
use crate::errors::*;

#[derive(Default)]
pub struct SoundController {
	channel1: [u8; 5],
	channel2: [u8; 5],
	channel3: [u8; 5],
	channel3_wave: [u8; 16],
	channel4: [u8; 5],
	registers: [u8; 3]
}

impl SoundController {
	pub fn can_access(addr: u16) -> bool {
		match addr {
			0xFF10..=0xFF14 | 0xFF16..=0xFF19 | 0xFF1A..=0xFF1E |
			0xFF20..=0xFF23 | 0xFF24..=0xFF26 | 0xFF30..=0xFF3F => true,
			_ => false
		}
	}
}

impl MemoryInterface for SoundController {
	fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
		match addr {
			0xFF10..=0xFF14 => {
				self.channel1[(addr - 0xFF10) as usize] = value; },
			0xFF16..=0xFF19 => {
				self.channel2[(addr - 0xFF16) as usize] = value; },
			0xFF1A..=0xFF1E => {
				self.channel3[(addr - 0xFF1A) as usize] = value; },
			0xFF20..=0xFF23 => {
				self.channel4[(addr - 0xFF20) as usize] = value; },
			0xFF24..=0xFF26 => {
				self.registers[(addr - 0xFF24) as usize] = value; },
			0xFF30..=0xFF3F => {
				self.channel3_wave[(addr - 0xFF30) as usize] = value; }
			_ => { return Err(err_msg("Unimplemented sound addr")) }
		}

		Ok(())
	}

	fn load8(&mut self, addr: u16) -> Result<u8> {
		Ok(match addr {
			0xFF10..=0xFF14 => { self.channel1[(addr - 0xFF10) as usize] },
			0xFF16..=0xFF19 => { self.channel2[(addr - 0xFF16) as usize] },
			0xFF1A..=0xFF1E => { self.channel3[(addr - 0xFF1A) as usize] },
			0xFF20..=0xFF23 => { self.channel4[(addr - 0xFF20) as usize] },
			0xFF24..=0xFF26 => { self.registers[(addr - 0xFF24) as usize] },
			0xFF30..=0xFF3F => { self.channel3_wave[(addr - 0xFF30) as usize] }
			_ => { return Err(err_msg("Unimplemented sound addr")) }
		})
	}
}