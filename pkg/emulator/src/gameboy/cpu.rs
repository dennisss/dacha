use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;

struct HalfRegister<'a, T: AsRef<&'a u16>> {
	full: PhantomData<'a, T>,
	shift: u8
}

impl<'a, T: AsRef<u16>> HalfRegister<'a, T> {
	pub fn get(&self) -> u8 {
		((*self.full.as_ref() >> self.shift) & 0xff) as u8
	}
}

impl<'a, T: AsRef<u16> + AsMut<u16>> HalfRegister<'a, T> {
	pub fn set(&mut self, val: u8) {
		let mut reg = *self.full.as_ref();
		reg = reg & (0xff << self.shift); // Clear bits
		reg = reg & ((val as u16) << self.shift); // Add new bits
		*self.full.as_mut() = reg;
	}
}

// https://gbdev.gg8.se/wiki/articles/CPU_Registers_and_Flags
struct CPURegisters {
	pub AF: u16,
	pub BC: u16,
	pub DE: u16,
	pub HL: u16,
	pub SP: u16,
	pub PC: u16
}

impl CPURegisters {
	pub fn A(&mut self) -> HalfRegister<&u16> {
		HalfRegister { full: &mut self.AF, shift: 8 }
	}

}

struct CPU {

}

// LD B,A  0x47
// LD D,A  0x57


// Instruction table: https://www.pastraiser.com/cpu/gameboy/gameboy_opcodes.html