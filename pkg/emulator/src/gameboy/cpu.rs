use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use std::borrow::{Borrow, BorrowMut};
use crate::errors::*;
use crate::gameboy::memory::*;
use failure::_core::cell::RefCell;

pub struct HalfRegister<T: Borrow<u16>> {
	full: T,
	shift: u16,
}

impl<T: Borrow<u16>> HalfRegister<T> {
	pub fn high(full: T) -> Self {
		HalfRegister { full, shift: 8 }
	}

	pub fn low(full: T) -> Self {
		Self { full, shift: 0 }
	}

	pub fn get(&self) -> u8 {
		((*self.full.borrow() >> self.shift) & 0xff) as u8
	}
}

impl<T: Borrow<u16> + BorrowMut<u16>> HalfRegister<T> {
	pub fn set(&mut self, val: u8) {
		let mut reg = *self.full.borrow();
		reg = reg & (!(0xff << self.shift)); // Clear bits
		reg = reg | ((val as u16) << self.shift); // Add new bits
		*self.full.borrow_mut() = reg;
	}
}

macro_rules! half_register_accessors {
    ($name:ident, $name_mut:ident, $prop:ident, $half:ident) => {
    	pub fn $name(&self) -> HalfRegister<&u16> {
			HalfRegister::$half(&self.$prop)
		}
		pub fn $name_mut(&mut self) -> HalfRegister<&mut u16> {
			HalfRegister::$half(&mut self.$prop)
		}
    };
}

macro_rules! flag_accessors {
	($name: ident, $set_name:ident, $bit:expr) => {
		pub fn $name(&self) -> bool {
			(self.AF >> $bit) & 1 != 0
		}
		pub fn $set_name(&mut self, set: bool) {
			let mut val = 1 << $bit;
			let mask = !val;
			self.AF = (self.AF & mask) | (if set { val } else { 0 });
		}
	}
}


// https://gbdev.gg8.se/wiki/articles/CPU_Registers_and_Flags
#[derive(Default, Debug, Clone, PartialEq)]
pub struct CPURegisters {
	// TODO: Why not just wrap these in a 'Wrapping' type?
	// Accumulator and flags
	pub AF: u16,
	pub BC: u16,
	pub DE: u16,
	pub HL: u16,
	// Stack Pointer
	pub SP: u16,
	// Program Counter
	pub PC: u16
}


impl CPURegisters {
	half_register_accessors!(a, a_mut, AF, high);

	// Only used in special circumstances
	half_register_accessors!(unsafe_f, unsafe_f_mut, AF, low);

	half_register_accessors!(b, b_mut, BC, high);
	half_register_accessors!(c, c_mut, BC, low);
	half_register_accessors!(d, d_mut, DE, high);
	half_register_accessors!(e, e_mut, DE, low);
	half_register_accessors!(h, h_mut, HL, high);
	half_register_accessors!(l, l_mut, HL, low);

	// Zero flag. Set when the result is 0.
	flag_accessors!(flag_z, set_flag_z, 7);
	// Set to 1 by subtraction ops. Set to 0 by addition ops.
	flag_accessors!(flag_n, set_flag_n, 6);
	flag_accessors!(flag_h, set_flag_h, 5);
	// Carry flag. Set when addition exceed 0xff or 0xffff depending on mode, or
	// when a subtraction goes below 0.
	flag_accessors!(flag_c, set_flag_c, 4);

}


#[derive(Debug, Default)]
pub struct CPU {
	pub registers: CPURegisters,

	/// Number of cycles remaining in 'executing' the previous instruction. For
	/// simplicity, we execute the full instruction on the 0th cycle, but we
	/// still block for the whole duration before executing future instructions.
	pub remaining_cycles: usize,

	// If true, then instructions are actively executing. STOP causes this to
	// be set to false.
	stopped: bool,

	// If true, then the CPU is executing HALT until an interrupt is received.
	sleeping: bool,

	/// This is the 'Interrupt Master Enable' (IME) flag.
	interrupts_enabled: bool,

	done_initial: bool
}

// Use an async function?

impl CPU {
	// Mainly to be used for debugging.
	pub fn step_full(&mut self, memory: &mut dyn MemoryInterface,
					 interrupts: &RefCell<InterruptState>) -> Result<()> {
		self.step(memory, interrupts, 0)?;
		while self.remaining_cycles > 0 {
			self.step(memory, interrupts, 0)?;
		}

		Ok(())
	}

	// TODO: When reading from memory, we need to block for 4 cycles.

	// NOTE: Only works if called at 4MHz.
	pub fn step(&mut self, memory: &mut dyn MemoryInterface,
				interrupts: &RefCell<InterruptState>, cycles: u64) -> Result<()> {
		if self.registers.PC >= 0x100 {
			self.done_initial = true;
		}

		// Hack to make all instructions run 4 cycles ahead (time after the
		// first opcode byte was read).
//		if cycles < 4 {
//			return Ok(());
//		}

		if self.remaining_cycles > 0 {
			self.remaining_cycles -= 1;
			return Ok(());
		}

		if self.stopped {
			println!("CPU STOPPED");
			return Ok(());
		}

		// TODO: Will halt wakeup immediately if existing interrupts are already
		// being requested or only on new ones?
		if self.sleeping {
			if interrupts.borrow_mut().some_requested() {
				self.sleeping = false;
			} else {
//				println!("SLEEPING");
				return Ok(());
			}
		}

		if self.interrupts_enabled {
			if let Some(interrupt_addr) = interrupts.borrow_mut().next() {
				self.interrupts_enabled = false;

				// TODO: Debug with below.
				let mut state = InstructionState {
					code_length: 1,
					cpu: self,
					mem: memory
				};

				match call_impl(&mut state, interrupt_addr) {
					Ok(_) => {},
					Err(e) => {
						println!("Failed to start interrupt: {:04X}", interrupt_addr);
						println!("{:?}", self.registers);

						return Err(e);
					}
				}

				// I don't actually know how many cycles it takes, but it should
				// take ~8 as we stores 2 bytes into memory (by storing the old PC
				// onto the stack).
				//
				// NOTE: We shouldn't need to wait a full 'CALL' op duration as we
				// don't need to look up the op code or immediate address from
				// memory.
				self.remaining_cycles = 8 - 1;

				return Ok(());
			}
		}

		let code = memory.load8(self.registers.PC)?;

		let inst = &INSTRUCTION_SET[code as usize];

//		if self.done_initial {
//			println!("A:{:02x} F:{}{}{}{} BC:{:04x} DE:{:04x} HL:{:04x} SP:{:04x} PC:{:04x} (cy: {})",
//					 self.registers.a().get(),
//					 if self.registers.flag_z() { 'Z' } else { '-' },
//					 if self.registers.flag_n() { 'N' } else { '-' },
//					 if self.registers.flag_h() { 'H' } else { '-' },
//					 if self.registers.flag_c() { 'C' } else { '-' },
//					 self.registers.BC,
//					 self.registers.DE,
//					 self.registers.HL,
//					 self.registers.SP,
//					 self.registers.PC, cycles);
//		}

		self.registers.PC += 1; // TODO: Wrapping add?

		let mut state = InstructionState {
			code_length: 1, cpu: self, mem: memory
		};


		let step = match (inst.handler)(&mut state) {
			Ok(step) => step,
			Err(e) => {
				println!("{:04X}: {}: {}", self.registers.PC, code, inst.mnemonic);
				println!("{:?}", self.registers);
				return Err(e);
			}
		};

		// NOTE: We subtract one as we just executed one cycle.
		self.remaining_cycles = step.cycles - 1;

		Ok(())
	}

	/*
		NOTE: THe interrupt registers are likely implemented in the CPU so don't
		need a memory hop?Th

		Running an interrupt:
		- Disable interrupts
		- Clear one interrupt bit
		- Push PC onto stack
		- set PC to the interrupt handler.
	*/
}

#[derive(Debug)]
pub enum MemoryAction {
	Read, Write
}

#[derive(Default, Debug)]
pub struct BlockingMemoryTape {
	// TODO: It would also be cool if we record the exact cycle timestamp of each
	// action?
	events: Vec<(MemoryAction, u16, u8)>
}

pub struct BlockingMemory<'a> {
	inner: &'a mut dyn MemoryInterface,
	tape: &'a mut BlockingMemoryTape,
	index: usize
}

impl MemoryInterface for BlockingMemory<'_> {
	fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
		if self.index < self.tape.events.len() {
			// TODO: Check that previous event is the same
			self.index += 1;
			Ok(())
		} else {
			self.inner.store8(addr, value)?;
			self.tape.events.push((MemoryAction::Write, addr, value));
			Err(err_msg("WOULD_BLOCK"))
		}
	}

	fn load8(&mut self, addr: u16) -> Result<u8> {
		if self.index < self.tape.events.len() {
			let v = self.tape.events[self.index].2;
			self.index += 1;
			// TODO: Check event type and address
			Ok(v)
		} else {
			let v = self.inner.load8(addr)?;
			self.tape.events.push((MemoryAction::Read, addr, v));
			Err(err_msg("WOULD_BLOCK"))
		}
	}
}


struct InstructionState<'a> {
	code_length: usize,
	pub cpu: &'a mut CPU,

	// TODO: Whenever reading/writing from/to memory, trace how many bytes have
	// been transfered to measure the total duration of the instruction in 4
	// cycles per byte intervals.
	pub mem: &'a mut dyn MemoryInterface
}

// Raw machine code for a single instruction. Contains the opcode and operands.
//struct Code<'a> {
//	opcode: u8,
//	operands: &'a [u8]
//}

impl<'a> InstructionState<'a> {
	fn imm8(&mut self) -> Result<u8> {
		let v = self.mem.load8(self.cpu.registers.PC)?;
		self.cpu.registers.PC = self.cpu.registers.PC.wrapping_add(1);
		self.code_length += 1;
		Ok(v)
	}
	fn imm16(&mut self) -> Result<u16> {
		let v = self.mem.load16(self.cpu.registers.PC)?;
		self.cpu.registers.PC = self.cpu.registers.PC.wrapping_add(2);
		self.code_length += 2;
		Ok(v)
	}
}

struct Step {
	cycles: usize
}

impl Step {
	fn duration(cycles: usize) -> Self {
		Self { cycles }
	}
}

// TODO: Verify that all 'wrapping_*' operations are assigned back somewhere.

struct Instruction {
	mnemonic: &'static str,
	handler: &'static Fn(&mut InstructionState) -> Result<Step>
}

macro_rules! inc16 {
	($r:tt) => {
		Instruction {
			mnemonic: stringify!(INC $r),
			handler: &|state| {
				let mut v = load16!(state, $r);
				v = v.wrapping_add(1);
				store16!(state, $r <- v);
				Ok(Step::duration(8))
			}
		}
	}
}

macro_rules! dec16 {
    ($r:ident) => {
    	Instruction {
			mnemonic: stringify!(DEC $r),
			handler: &|state| {
				let mut v = load16!(state, $r);
				v = v.wrapping_sub(1);
				store16!(state, $r <- v);
				Ok(Step::duration(8))
			}
		}
    };
}

fn add8_half_carry(a: u8, b: u8) -> bool {
	(((a & 0x0f) + (b & 0x0f)) & (1 << 4)) != 0
}

fn add16_half_carry(a: u16, b: u16) -> bool {
	((a & 0x0fff) + (b & 0x0fff)) & (1 << 12) != 0
}


// e.g. 'ADD HL, BC'
macro_rules! add16 {
    ($a:ident, $b:ident) => {
    	Instruction {
			mnemonic: stringify!(ADD $a, $b),
			handler: &|state| {
				let a = load16!(state, $a);
				let b = load16!(state, $b);

				let (v, carry) = a.overflowing_add(b);
				store16!(state, $a <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_n(false);
				regs.set_flag_h(add16_half_carry(a, b));
				regs.set_flag_c(carry);

				Ok(Step::duration(8))
			}
		}
    };
}

macro_rules! inc8 {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(INC $r),
			handler: &|state| {
				let mut old_v = load8!(state, $r);
				let v = old_v.wrapping_add(1);
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(add8_half_carry(old_v, 1));
				// C unchanged

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! dec8 {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(DEC $r),
			handler: &|state| {
				let mut old_v = load8!(state, $r);
				let v = old_v.wrapping_sub(1);
				store8!(state, $r <- v);

				// TODO: Check this
				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(true);
				regs.set_flag_h((v & 0b1111) > (old_v & 0b1111));
				// C unchanged

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! add8 {
	($a:tt, $b:tt | $duration:expr) => {
		Instruction {
			mnemonic: stringify!(ADD $a, $b),
			handler: add8!($a, $b + 0 | $duration)
		}
	};

	// Internal use only
    ($a:tt, $b:tt + $n:tt | $duration:expr) => {
    	&|state: &mut InstructionState| {
			let a = load8!(state, $a);
			let b = load8!(state, $b);
			let (v, carry1) = a.overflowing_add(b);
			let (v2, carry2) = v.overflowing_add($n);
			store8!(state, $a <- v2);

			let regs = &mut state.cpu.registers;
			regs.set_flag_z(v2 == 0);
			regs.set_flag_n(false);
			regs.set_flag_h(add8_half_carry(a, b) || add8_half_carry(v, $n));
			regs.set_flag_c(carry1 || carry2);

			Ok(Step::duration($duration))
		}
    };
}

macro_rules! adc8 {
	($a:tt, $b:tt | $duration:expr) => {
		Instruction {
			mnemonic: stringify!(AFC $a, $b),
			handler: &|state| {
				let c = if state.cpu.registers.flag_c() { 1 } else { 0 };
				let f = add8!($a, $b + c | $duration);
				f(state)
			}
		}
	};
}


macro_rules! sub8 {
	($b:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(SUB $b),
			handler: &|state| {
				let v = sub8_inner!(state, $b + 0);
				store8!(state, A <- v);
				Ok(Step::duration($duration))
			}
		}
    };
}

// Performs the subtraction but does not assign anything
// TODO: THis would be easy to make into a regular function.
macro_rules! sub8_inner {
    ($state:ident, $b:tt + $n:expr) => {{
	    let a = load8!($state, A);
		let b = load8!($state, $b);
		let (v, carry1) = a.overflowing_sub(b);
		let (v2, carry2) = v.overflowing_sub($n);

		let regs = &mut $state.cpu.registers;
		regs.set_flag_z(v2 == 0);
		regs.set_flag_n(true);
		regs.set_flag_h((v & 0x0f) > (a & 0x0f) || (v2 & 0x0f) > (v & 0x0f));
		regs.set_flag_c(carry1 || carry2);

    	v2
    }};
}

macro_rules! sbc8 {
    (A, $b:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(SBC $a $b),
			handler: &|state| {
				let cy = if state.cpu.registers.flag_c() { 1 } else { 0 };
				let v = sub8_inner!(state, $b + cy);
				store8!(state, A <- v);
				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! cp8 {
    ($b:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(CP $b),
			handler: &|state| {
				sub8_inner!(state, $b + 0);
				// NOTE: This instruction only changes flags. Not registers.
				Ok(Step::duration($duration))
			}
		}
    };
}


macro_rules! or8 {
    ($b:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(OR $b),
			handler: &|state| {
				let a = load8!(state, A);
				let b = load8!(state, $b);
				let v = a | b;
				store8!(state, A <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(false);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! and8 {
    ($b:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(AND $b),
			handler: &|state| {
				let a = load8!(state, A);
				let b = load8!(state, $b);
				let v = a & b;
				store8!(state, A <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(true);
				regs.set_flag_c(false);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! xor8 {
    ($b:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(XOR $b),
			handler: &|state| {
				let a = load8!(state, A);
				let b = load8!(state, $b);
				let v = a ^ b;
				store8!(state, A <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(false);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! bit8 {
    ($n:expr, $r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(BIT $n, $r),
			handler: &|state| {
				let mut v = load8!(state, $r);

				v = (v >> $n) & 1;

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(true);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! set8 {
    ($n:expr, $r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(SET $n, $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				v = v | (1 << $n);
				store8!(state, $r <- v);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! res8 {
    ($n:expr, $r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(RES $n, $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				v = v & !(1 << $n);
				store8!(state, $r <- v);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! wrapping_assign {
    ($a:expr, += $b:expr) => { $a = $a.wrapping_add($b); };
    ($a:expr, -= $b:expr) => { $a = $a.wrapping_sub($b); };
}

macro_rules! load8 {
	($state:ident, imm8) => { $state.imm8()? };
	($state:ident, (imm16)) => {{
		let addr = $state.imm16()?;
		$state.mem.load8(addr)?
	}};
	($state:ident, A) => { $state.cpu.registers.a().get() };
	($state:ident, B) => { $state.cpu.registers.b().get() };
	($state:ident, C) => { $state.cpu.registers.c().get() };
	($state:ident, D) => { $state.cpu.registers.d().get() };
	($state:ident, E) => { $state.cpu.registers.e().get() };
	($state:ident, H) => { $state.cpu.registers.h().get() };
	($state:ident, L) => { $state.cpu.registers.l().get() };
	($state:ident, $r8:ident) => { $state.cpu.registers.$r8().get() };
	($state:ident, (C)) => {
		$state.mem.load8(0xFF00 + ($state.cpu.registers.c().get() as u16))?
	};
	// TODO: Must consistently do a wrapping_add/sub here and in the other one too
	($state:ident, (HL+)) => {{
		let v = load8!($state, (HL));
		wrapping_assign!($state.cpu.registers.HL, += 1);
		v
	}};
	($state:ident, (HL-)) => {{
		let v = load8!($state, (HL));
		wrapping_assign!($state.cpu.registers.HL, -= 1);
		v
	}};
	($state:ident, ($r16:ident)) => {
		$state.mem.load8($state.cpu.registers.$r16)?
	};
}

macro_rules! load16 {
	($state:ident, imm16) => { $state.imm16()? };
	($state:ident, $r16:ident) => { $state.cpu.registers.$r16 };
	($state:ident, ($r16:ident)) => {
		$state.mem.load16($state.cpu.registers.$r16)?
	};
}

macro_rules! store8 {
	($state:ident, (imm16) <- $v:expr) => {
		let addr = $state.imm16()?;
		$state.mem.store8(addr, $v)?
	};
	($state:ident, A <- $v:expr) => { $state.cpu.registers.a_mut().set($v) };
	($state:ident, B <- $v:expr) => { $state.cpu.registers.b_mut().set($v) };
	($state:ident, C <- $v:expr) => { $state.cpu.registers.c_mut().set($v) };
	($state:ident, D <- $v:expr) => { $state.cpu.registers.d_mut().set($v) };
	($state:ident, E <- $v:expr) => { $state.cpu.registers.e_mut().set($v) };
	($state:ident, H <- $v:expr) => { $state.cpu.registers.h_mut().set($v) };
	($state:ident, L <- $v:expr) => { $state.cpu.registers.l_mut().set($v) };
	($state:ident, (C) <- $v:expr) => {
		$state.mem.store8(0xff00 + ($state.cpu.registers.c().get() as u16), $v)?
	};
	($state:ident, (HL+) <- $v:expr) => {
		store8!($state, (HL) <- $v);
		wrapping_assign!($state.cpu.registers.HL, += 1);
	};
	($state:ident, (HL-) <- $v:expr) => {
		store8!($state, (HL) <- $v);
		wrapping_assign!($state.cpu.registers.HL, -= 1);
	};
	($state:ident, ($r16:ident) <- $v:expr) => {
		$state.mem.store8($state.cpu.registers.$r16, $v)?
	};
}

macro_rules! store16 {
	($state:ident, (imm16) <- $v:expr) => {
		let addr = $state.imm16()?;
		$state.mem.store16(addr, $v)?
	};
	($state:ident, $r16:ident <- $v:expr) => { $state.cpu.registers.$r16 = $v; };
	($state:ident, ($r16:ident) <- $v:expr) => {
		$state.mem.store16($state.cpu.registers.$r16, $v)?
	};
}

macro_rules! ld8 {
    ($a:tt, $b:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(LD $a, $b),
			handler: &|state| {
				let b = load8!(state, $b);
				store8!(state, $a <- b);
				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! ld16 {
    ($a:tt, $b:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(LD $a, $b),
			handler: &|state| {
				let b = load16!(state, $b);
				store16!(state, $a <- b);
				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! push16 {
    ($a:tt) => {
    	Instruction {
			mnemonic: stringify!(PUSH $a),
			handler: &|state| {
				let a = load16!(state, $a);
				state.cpu.registers.SP = state.cpu.registers.SP.wrapping_sub(2);
				store16!(state, (SP) <- a);
				Ok(Step::duration(16))
			}
		}
    };
}

macro_rules! pop16 {
    ($a:tt) => {
    	Instruction {
			mnemonic: stringify!(POP $a),
			handler: &|state| {
				let f = state.cpu.registers.unsafe_f().get();

				let a = load16!(state, (SP));
				state.cpu.registers.SP = state.cpu.registers.SP.wrapping_add(2);
				store16!(state, $a <- a);

				// The lower 4 bits of AF are not writeable.
				// TODO: Implement this more genericly to gurantee that no one
				// can write to those bits
				let f2 = state.cpu.registers.unsafe_f().get();
				state.cpu.registers.unsafe_f_mut().set((f & 0x0f) | (f2 & 0xf0));

				Ok(Step::duration(12))
			}
		}
    };
}

macro_rules! call {
    (imm16) => {
    	Instruction {
			mnemonic: stringify!(CALL imm16),
			handler: &|state| {
				let addr = load16!(state, imm16);
				call_impl(state, addr)
			}
		}
    };
    ($cond:ident, imm16) => {
    	Instruction {
			mnemonic: stringify!(CALL $cond a16),
			handler: &|state| {
				let addr = load16!(state, imm16);
				if cond!(state, $cond) {
					call_impl(state, addr)
				} else {
					Ok(Step::duration(12))
				}
			}
		}
    };
}

fn call_impl(state: &mut InstructionState, addr: u16) -> Result<Step> {
	wrapping_assign!(state.cpu.registers.SP, -= 2);
	store16!(state, (SP) <- state.cpu.registers.PC);
	store16!(state, PC <- addr);
	Ok(Step::duration(24))
}



const RET: Instruction = Instruction {
	mnemonic: "RET",
	handler: &|state| {
		state.cpu.registers.PC = load16!(state, (SP));
		wrapping_assign!(state.cpu.registers.SP, += 2);
		Ok(Step::duration(16))
	}
};

const RETI: Instruction = Instruction {
	mnemonic: "RETI",
	handler: &|state| {
		state.cpu.interrupts_enabled = true;
		(RET.handler)(state)
	}
};

macro_rules! ret {
    ($cond:ident) => {
    	Instruction {
			mnemonic: stringify!(RET $cond),
			handler: &|state| {
				if cond!(state, $cond) {
					(RET.handler)(state)?;
					Ok(Step::duration(20))
				} else {
					Ok(Step::duration(8))
				}
			}
		}
    };
}


macro_rules! cond {
    ($state:ident, NZ) => { !$state.cpu.registers.flag_z() };
    ($state:ident, Z) => { $state.cpu.registers.flag_z() };
    ($state:ident, NC) => { !$state.cpu.registers.flag_c() };
    ($state:ident, C) => { $state.cpu.registers.flag_c() };
}

macro_rules! rst {
    ($imm8:expr) => {
    	// TODO: Dedup with CALL
		Instruction {
			mnemonic: stringify!(RST $imm8),
			handler: &|state| {
				state.cpu.registers.SP = state.cpu.registers.SP.wrapping_sub(2);
				store16!(state, (SP) <- state.cpu.registers.PC);
				store16!(state, PC <- $imm8 as u16);
				Ok(Step::duration(16))
			}
		}
    };
}

macro_rules! jp {
    ($r:tt | $duration:expr) => {
		Instruction {
			mnemonic: stringify!(JP $r),
			handler: &|state| {
				let addr = load16!(state, $r);
				state.cpu.registers.PC = addr;
				Ok(Step::duration($duration))
			}
		}
    };
    ($cond:ident, $r:tt) => {
		Instruction {
			mnemonic: stringify!(JP $cond, $r),
			handler: &|state| {
				let addr = load16!(state, $r);
				if cond!(state, $cond) {
					state.cpu.registers.PC = addr;
					Ok(Step::duration(16))
				} else {
					Ok(Step::duration(12))
				}
			}
		}
    };
}

// TODO: Verify that all adds and subtracts are wrapping (or is at least unchecked)

macro_rules! jr {
    (PC + imm8) => {
		Instruction {
			mnemonic: "JR PC + imm8",
			handler: &|state| {
				let offset = load8!(state, imm8);
				jr_impl(state, offset)
			}
		}
    };
    ($cond:ident, PC + imm8) => {
		Instruction {
			mnemonic: stringify!(JR $cond, PC + imm8),
			handler: &|state| {
				let offset = load8!(state, imm8);
				if cond!(state, $cond) {
					jr_impl(state, offset)
				} else {
					Ok(Step::duration(8))
				}
			}
		}
    }
}

fn jr_impl(state: &mut InstructionState, offset: u8) -> Result<Step> {
	// The offset is a signed integer.
	let offset = ((offset as i8) as i16) as u16;

	state.cpu.registers.PC = state.cpu.registers.PC.wrapping_add(offset);
	Ok(Step::duration(12))
}


macro_rules! swap8 {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(SWAP $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				v = ((v & 0xf0) >> 4) | ((v & 0x0f) << 4);
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(false);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! rlc {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(RLC $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				v = v.rotate_left(1);
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(v & 1 != 0);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! rrc {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(RRC $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				v = v.rotate_right(1);
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(v & 0b10000000 != 0);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! rr {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(RR $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				let new_carry = v & 1 != 0;
				v = v.overflowing_shr(1).0 |
					(if state.cpu.registers.flag_c() { 1 } else { 0 } << 7);
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(new_carry);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! rl {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(RL $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				let new_carry = v & 0b10000000 != 0;
				v = v.overflowing_shl(1).0 |
					(if state.cpu.registers.flag_c() { 1 } else { 0 });
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(new_carry);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! sla {
    ($r:tt | $duration:expr) => {
    	/// Shift Left with overflow bit going into carry.
		/// cy <- Register <- 0
    	Instruction {
			mnemonic: stringify!(SLA $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				let new_carry = v & 0b10000000 != 0;
				v = v.overflowing_shl(1).0;
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(new_carry);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! sra {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(SRA $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				let new_carry = v & 1 != 0;
				v = (v >> 1) | (v & 0b10000000);
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(new_carry);

				Ok(Step::duration($duration))
			}
		}
    };
}

macro_rules! srl {
    ($r:tt | $duration:expr) => {
    	Instruction {
			mnemonic: stringify!(SRL $r),
			handler: &|state| {
				let mut v = load8!(state, $r);
				let new_carry = v & 1 != 0;
				v = v >> 1;
				store8!(state, $r <- v);

				let regs = &mut state.cpu.registers;
				regs.set_flag_z(v == 0);
				regs.set_flag_n(false);
				regs.set_flag_h(false);
				regs.set_flag_c(new_carry);

				Ok(Step::duration($duration))
			}
		}
    };
}


const RESERVED: Instruction = Instruction {
	mnemonic: "RESERVED",
	handler: &|state| {
		Err(err_msg("Executing undefined instruction."))
	},
};


// TODO: Ensure that the memory at which an instruction is located isn't edited
// mid instruction.


// TODO: Enforce that this buffer is 256 elements.
const INSTRUCTION_SET: &'static [Instruction; 256] = &[
	// 0x
	Instruction { mnemonic: "NOP", handler: &|_| Ok(Step::duration(4)) },
	ld16!(BC, imm16 | 12),
	ld8!((BC), A | 8),
	inc16!(BC),
	inc8!(B | 4),
	dec8!(B | 4),
	ld8!(B, imm8 | 8),
	Instruction {
		mnemonic: "RLCA",
		handler: &|state| {
			let regs = &mut state.cpu.registers;
			let a = regs.a().get().rotate_left(1);
			regs.a_mut().set(a);

			regs.set_flag_z(false);
			regs.set_flag_n(false);
			regs.set_flag_h(false);
			regs.set_flag_c(a & 1 != 0);

			Ok(Step::duration(4))
		}
	},
	ld16!((imm16), SP | 20),
	add16!(HL, BC),
	ld8!(A, (BC) | 8),
	dec16!(BC),
	inc8!(C | 4),
	dec8!(C | 4),
	ld8!(C, imm8 | 8),
	Instruction {
		mnemonic: "RRCA",
		handler: &|state| {
			let regs = &mut state.cpu.registers;
			let a = regs.a().get();
			regs.a_mut().set(a.rotate_right(1));

			regs.set_flag_z(false);
			regs.set_flag_n(false);
			regs.set_flag_h(false);
			regs.set_flag_c(a & 1 != 0);

			Ok(Step::duration(4))
		}
	},

	// 1x
	Instruction {
		// TODO: Technically is 2 bytes, but always works if only 1 is given.
		mnemonic: "STOP",
		handler: &|state| {
			state.cpu.stopped = true;
			Ok(Step::duration(4))
		}
	},
	ld16!(DE, imm16 | 12),
	ld8!((DE), A | 8),
	inc16!(DE),
	inc8!(D | 4),
	dec8!(D | 4),
	ld8!(D, imm8 | 8),
	Instruction {
		// Rotate Left [cy, A]
		mnemonic: "RLA",
		handler: &|state| {
			let regs = &mut state.cpu.registers;
			let a = regs.a().get();
			let c = if regs.flag_c() { 1 } else { 0 };

			regs.a_mut().set( a.overflowing_shl(1).0 | c );
			regs.set_flag_c((a >> 7) != 0);

			regs.set_flag_z(false);
			regs.set_flag_n(false);
			regs.set_flag_h(false);

			Ok(Step::duration(4))
		}
	},
	jr!(PC + imm8),
	add16!(HL, DE),
	ld8!(A, (DE) | 8),
	dec16!(DE),
	inc8!(E | 4),
	dec8!(E | 4),
	ld8!(E, imm8 | 8),
	Instruction {
		// Rotate right: [A, cy]
		mnemonic: "RRA",
		handler: &|state| {
			let regs = &mut state.cpu.registers;
			let a = regs.a().get();
			let c = if regs.flag_c() { 1 } else { 0 };

			regs.a_mut().set( a.overflowing_shr(1).0 | (c << 7) );
			regs.set_flag_c((a & 1) != 0);

			regs.set_flag_z(false);
			regs.set_flag_n(false);
			regs.set_flag_h(false);

			Ok(Step::duration(4))
		}
	},

	// 2x
	jr!(NZ, PC + imm8),
	ld16!(HL, imm16 | 12),
	ld8!((HL+), A | 8),
	inc16!(HL),
	inc8!(H | 4),
	dec8!(H | 4),
	ld8!(H, imm8 | 8),
	Instruction {
		mnemonic: "DAA",
		handler: &|state| {
			// TODO: See https://ehaskins.com/2018-01-30%20Z80%20DAA/

			let regs = &mut state.cpu.registers;

			let mut value = regs.a().get();

			let mut correction = 0;

			if regs.flag_h() || (!regs.flag_n() && (value & 0xf) > 9) {
				correction = 0x6;
			}

			let mut carry = false;
			if regs.flag_c() || (!regs.flag_n() && value > 0x99) {
				correction |= 0x60;
				carry = true;
			}

//			value += regs.flag_n()? -correction : correction;
			if regs.flag_n() {
				value = value.wrapping_sub(correction);
			} else {
				value = value.wrapping_add(correction);
			}

			regs.a_mut().set(value);

			regs.set_flag_z(value == 0);
			regs.set_flag_h(false);
			regs.set_flag_c(carry);

			Ok(Step::duration(4))
		}
	},
	jr!(Z, PC + imm8),
	add16!(HL, HL),
	ld8!(A, (HL+) | 8),
	dec16!(HL),
	inc8!(L | 4),
	dec8!(L | 4),
	ld8!(L, imm8 | 8),
	Instruction {
		mnemonic: "CPL",
		handler: &|state| {
			let mut a = state.cpu.registers.a_mut();
			let v = a.get() ^ 0xff;
			a.set(v);

			state.cpu.registers.set_flag_n(true);
			state.cpu.registers.set_flag_h(true);

			Ok(Step::duration(4))
		}
	},

	// 3x
	jr!(NC, PC + imm8),
	ld16!(SP, imm16 | 12),
	ld8!((HL-), A | 8),
	inc16!(SP),
	inc8!((HL) | 12),
	dec8!((HL) | 12),
	ld8!((HL), imm8 | 12),
	Instruction {
		mnemonic: "SCF",
		handler: &|state| {
			state.cpu.registers.set_flag_n(false);
			state.cpu.registers.set_flag_h(false);
			state.cpu.registers.set_flag_c(true);
			Ok(Step::duration(4))
		}
	},
	jr!(C, PC + imm8),
	add16!(HL, SP),
	ld8!(A, (HL-) | 8),
	dec16!(SP),
	inc8!(A | 4),
	dec8!(A | 4),
	ld8!(A, imm8 | 8),
	Instruction {
		mnemonic: "CCF",
		handler: &|state| {
			let mut cy = if state.cpu.registers.flag_c() { 1 } else { 0 };
			cy ^= 1;

			state.cpu.registers.set_flag_n(false);
			state.cpu.registers.set_flag_h(false);
			state.cpu.registers.set_flag_c(cy != 0);

			Ok(Step::duration(4))
		}
	},

	// 4x
	ld8!(B, B | 4),
	ld8!(B, C | 4),
	ld8!(B, D | 4),
	ld8!(B, E | 4),
	ld8!(B, H | 4),
	ld8!(B, L | 4),
	ld8!(B, (HL) | 8),
	ld8!(B, A | 4),
	ld8!(C, B | 4),
	ld8!(C, C | 4),
	ld8!(C, D | 4),
	ld8!(C, E | 4),
	ld8!(C, H | 4),
	ld8!(C, L | 4),
	ld8!(C, (HL) | 8),
	ld8!(C, A | 4),

	// 5x
	ld8!(D, B | 4),
	ld8!(D, C | 4),
	ld8!(D, D | 4),
	ld8!(D, E | 4),
	ld8!(D, H | 4),
	ld8!(D, L | 4),
	ld8!(D, (HL) | 8),
	ld8!(D, A | 4),
	ld8!(E, B | 4),
	ld8!(E, C | 4),
	ld8!(E, D | 4),
	ld8!(E, E | 4),
	ld8!(E, H | 4),
	ld8!(E, L | 4),
	ld8!(E, (HL) | 8),
	ld8!(E, A | 4),

	// 6x
	ld8!(H, B | 4),
	ld8!(H, C | 4),
	ld8!(H, D | 4),
	ld8!(H, E | 4),
	ld8!(H, H | 4),
	ld8!(H, L | 4),
	ld8!(H, (HL) | 8),
	ld8!(H, A | 4),
	ld8!(L, B | 4),
	ld8!(L, C | 4),
	ld8!(L, D | 4),
	ld8!(L, E | 4),
	ld8!(L, H | 4),
	ld8!(L, L | 4),
	ld8!(L, (HL) | 8),
	ld8!(L, A | 4),

	// 7x
	ld8!((HL), B | 8),
	ld8!((HL), C | 8),
	ld8!((HL), D | 8),
	ld8!((HL), E | 8),
	ld8!((HL), H | 8),
	ld8!((HL), L | 8),
	Instruction {
		// NOTE: This should get waken up even if interrupts are disabled.
		mnemonic: "HALT",
		handler: &|state| {
//			panic!("HALTED");

			state.cpu.sleeping = true;
			Ok(Step::duration(4))
		}
	},
	ld8!((HL), A | 8),
	ld8!(A, B | 4),
	ld8!(A, C | 4),
	ld8!(A, D | 4),
	ld8!(A, E | 4),
	ld8!(A, H | 4),
	ld8!(A, L | 4),
	ld8!(A, (HL) | 8),
	ld8!(A, A | 4),

	// 8x
	add8!(A, B | 4),
	add8!(A, C | 4),
	add8!(A, D | 4),
	add8!(A, E | 4),
	add8!(A, H | 4),
	add8!(A, L | 4),
	add8!(A, (HL) | 8),
	add8!(A, A | 4),
	adc8!(A, B | 4),
	adc8!(A, C | 4),
	adc8!(A, D | 4),
	adc8!(A, E | 4),
	adc8!(A, H | 4),
	adc8!(A, L | 4),
	adc8!(A, (HL) | 8),
	adc8!(A, A | 4),

	// 9x
	sub8!(B | 4),
	sub8!(C | 4),
	sub8!(D | 4),
	sub8!(E | 4),
	sub8!(H | 4),
	sub8!(L | 4),
	sub8!((HL) | 8),
	sub8!(A | 4),
	sbc8!(A, B | 4),
	sbc8!(A, C | 4),
	sbc8!(A, D | 4),
	sbc8!(A, E | 4),
	sbc8!(A, H | 4),
	sbc8!(A, L | 4),
	sbc8!(A, (HL) | 8),
	sbc8!(A, A | 4),

	// Ax
	and8!(B | 4),
	and8!(C | 4),
	and8!(D | 4),
	and8!(E | 4),
	and8!(H | 4),
	and8!(L | 4),
	and8!((HL) | 8),
	and8!(A | 4),
	xor8!(B | 4),
	xor8!(C | 4),
	xor8!(D | 4),
	xor8!(E | 4),
	xor8!(H | 4),
	xor8!(L | 4),
	xor8!((HL) | 8),
	xor8!(A | 4),

	// Bx
	or8!(B | 4),
	or8!(C | 4),
	or8!(D | 4),
	or8!(E | 4),
	or8!(H | 4),
	or8!(L | 4),
	or8!((HL) | 8),
	or8!(A | 4),
	cp8!(B | 4),
	cp8!(C | 4),
	cp8!(D | 4),
	cp8!(E | 4),
	cp8!(H | 4),
	cp8!(L | 4),
	cp8!((HL) | 8),
	cp8!(A | 4),

	// Cx
	ret!(NZ),
	pop16!(BC),
	jp!(NZ, imm16),
	jp!(imm16 | 16),
	call!(NZ, imm16),
	push16!(BC),
	add8!(A, imm8 | 8),
	rst!(0x00),
	ret!(Z),
	RET,
	jp!(Z, imm16),
	Instruction {
		mnemonic: "PREFIX CB",
		handler: &|state| {
			let code = state.imm8()?;
			let inst = &INSTRUCTION_SET_CB[code as usize];
			(inst.handler)(state)
		}
	},
	call!(Z, imm16),
	call!(imm16),
	adc8!(A, imm8 | 8),
	rst!(0x08),

	// Dx
	ret!(NC),
	pop16!(DE),
	jp!(NC, imm16),
	RESERVED,
	call!(NC, imm16),
	push16!(DE),
	sub8!(imm8 | 8),
	rst!(0x10),
	ret!(C),
	RETI,
	jp!(C, imm16),
	RESERVED,
	call!(C, imm16),
	RESERVED,
	sbc8!(A, imm8 | 8),
	rst!(0x18),

	// Ex
	Instruction {
		mnemonic: "LDH (imm8), A",
		handler: &|state| {
			let offset = state.imm8()?;
			let v = state.cpu.registers.a().get();
			state.mem.store8(0xff00 + (offset as u16), v)?;
			Ok(Step::duration(12))
		}
	},
	pop16!(HL),
	ld8!((C), A | 8),
	RESERVED,
	RESERVED,
	push16!(HL),
	and8!(imm8 | 8),
	rst!(0x20),
	Instruction {
		mnemonic: "ADD SP, imm8",
		handler: &|state| {
			let mut arg = state.imm8()? as i8;

			let regs = &mut state.cpu.registers;
			let mut sp = regs.SP;

			let (v, _) = sp.overflowing_add((arg as i16) as u16);
			let hcarry = (v & 0x0f) < (sp & 0x0f);
			let carry = (v & 0xff) < (sp & 0xff);

			regs.SP = v;

//			return Err(err_msg("Incomplete 1"));

			// TODO: Incomplete.

			regs.set_flag_z(false);
			regs.set_flag_n(false);
			regs.set_flag_h(hcarry);
			regs.set_flag_c(carry);

			Ok(Step::duration(16))
		},
	},

	jp!(HL | 4), // TODO: Check this (originally 'JP (HL)')
	ld8!((imm16), A | 16),
	RESERVED,
	RESERVED,
	RESERVED,
	xor8!(imm8 | 8),
	rst!(0x28),

	// Fx
	Instruction {
		mnemonic: "LDH A, (imm8)",
		handler: &|state| {
			let offset = state.imm8()?;
			let v = state.mem.load8(0xff00 + (offset as u16))?;
			state.cpu.registers.a_mut().set(v);
			Ok(Step::duration(12))
		}
	},
	pop16!(AF),
	ld8!(A, (C) | 8),
	Instruction {
		mnemonic: "DI",
		handler: &|state| {
			state.cpu.interrupts_enabled = false;
			Ok(Step::duration(4))
		}
	},
	RESERVED,
	push16!(AF),
	or8!(imm8 | 8),
	rst!(0x30),
	Instruction {
		mnemonic: "LD HL, SP + imm8",
		handler: &|state| {
			// NOTE: Signed i8
			let arg = state.imm8()? as i8;

			let regs = &mut state.cpu.registers;

			// TODO: Dedup with the other case that is like this.
			let (v, _) = regs.SP.overflowing_add((arg as i16) as u16);
			let hcarry = (v & 0x0f) < (regs.SP & 0x0f);
			let carry = (v & 0xff) < (regs.SP & 0xff);

			regs.HL = v;

			regs.set_flag_z(false);
			regs.set_flag_n(false);
			regs.set_flag_h(hcarry);
			regs.set_flag_c(carry);

			Ok(Step::duration(12))
		}
	},
	ld16!(SP, HL | 8),
	ld8!(A, (imm16) | 16),
	Instruction {
		mnemonic: "EI",
		handler: &|state| {
			state.cpu.interrupts_enabled = true;
			Ok(Step::duration(4))
		}
	},
	RESERVED,
	RESERVED,
	cp8!(imm8 | 8),
	rst!(0x38),
];

const INSTRUCTION_SET_CB: &'static [Instruction; 256] = &[
	// 0x
	rlc!(B | 8),
	rlc!(C | 8),
	rlc!(D | 8),
	rlc!(E | 8),
	rlc!(H | 8),
	rlc!(L | 8),
	rlc!((HL) | 16),
	rlc!(A | 8),
	rrc!(B | 8),
	rrc!(C | 8),
	rrc!(D | 8),
	rrc!(E | 8),
	rrc!(H | 8),
	rrc!(L | 8),
	rrc!((HL) | 16),
	rrc!(A | 8),

	// 1x
	rl!(B | 8),
	rl!(C | 8),
	rl!(D | 8),
	rl!(E | 8),
	rl!(H | 8),
	rl!(L | 8),
	rl!((HL) | 16),
	rl!(A | 8),
	rr!(B | 8),
	rr!(C | 8),
	rr!(D | 8),
	rr!(E | 8),
	rr!(H | 8),
	rr!(L | 8),
	rr!((HL) | 16),
	rr!(A | 8),

	// 2x
	sla!(B | 8),
	sla!(C | 8),
	sla!(D | 8),
	sla!(E | 8),
	sla!(H | 8),
	sla!(L | 8),
	sla!((HL) | 16),
	sla!(A | 8),
	sra!(B | 8),
	sra!(C | 8),
	sra!(D | 8),
	sra!(E | 8),
	sra!(H | 8),
	sra!(L | 8),
	sra!((HL) | 16),
	sra!(A | 8),

	// 3x
	swap8!(B | 8),
	swap8!(C | 8),
	swap8!(D | 8),
	swap8!(E | 8),
	swap8!(H | 8),
	swap8!(L | 8),
	swap8!((HL) | 16),
	swap8!(A | 8),
	srl!(B | 8),
	srl!(C | 8),
	srl!(D | 8),
	srl!(E | 8),
	srl!(H | 8),
	srl!(L | 8),
	srl!((HL) | 16),
	srl!(A | 8),

	// 4x
	bit8!(0, B | 8),
	bit8!(0, C | 8),
	bit8!(0, D | 8),
	bit8!(0, E | 8),
	bit8!(0, H | 8),
	bit8!(0, L | 8),
	bit8!(0, (HL) | 12),
	bit8!(0, A | 8),
	bit8!(1, B | 8),
	bit8!(1, C | 8),
	bit8!(1, D | 8),
	bit8!(1, E | 8),
	bit8!(1, H | 8),
	bit8!(1, L | 8),
	bit8!(1, (HL) | 12),
	bit8!(1, A | 8),

	// 5x
	bit8!(2, B | 8),
	bit8!(2, C | 8),
	bit8!(2, D | 8),
	bit8!(2, E | 8),
	bit8!(2, H | 8),
	bit8!(2, L | 8),
	bit8!(2, (HL) | 12),
	bit8!(2, A | 8),
	bit8!(3, B | 8),
	bit8!(3, C | 8),
	bit8!(3, D | 8),
	bit8!(3, E | 8),
	bit8!(3, H | 8),
	bit8!(3, L | 8),
	bit8!(3, (HL) | 12),
	bit8!(3, A | 8),

	// 6x
	bit8!(4, B | 8),
	bit8!(4, C | 8),
	bit8!(4, D | 8),
	bit8!(4, E | 8),
	bit8!(4, H | 8),
	bit8!(4, L | 8),
	bit8!(4, (HL) | 12),
	bit8!(4, A | 8),
	bit8!(5, B | 8),
	bit8!(5, C | 8),
	bit8!(5, D | 8),
	bit8!(5, E | 8),
	bit8!(5, H | 8),
	bit8!(5, L | 8),
	bit8!(5, (HL) | 12),
	bit8!(5, A | 8),

	// 7x
	bit8!(6, B | 8),
	bit8!(6, C | 8),
	bit8!(6, D | 8),
	bit8!(6, E | 8),
	bit8!(6, H | 8),
	bit8!(6, L | 8),
	bit8!(6, (HL) | 12),
	bit8!(6, A | 8),
	bit8!(7, B | 8),
	bit8!(7, C | 8),
	bit8!(7, D | 8),
	bit8!(7, E | 8),
	bit8!(7, H | 8),
	bit8!(7, L | 8),
	bit8!(7, (HL) | 12),
	bit8!(7, A | 8),

	// 8x
	res8!(0, B | 8),
	res8!(0, C | 8),
	res8!(0, D | 8),
	res8!(0, E | 8),
	res8!(0, H | 8),
	res8!(0, L | 8),
	res8!(0, (HL) | 16),
	res8!(0, A | 8),
	res8!(1, B | 8),
	res8!(1, C | 8),
	res8!(1, D | 8),
	res8!(1, E | 8),
	res8!(1, H | 8),
	res8!(1, L | 8),
	res8!(1, (HL) | 16),
	res8!(1, A | 8),

	// 9x
	res8!(2, B | 8),
	res8!(2, C | 8),
	res8!(2, D | 8),
	res8!(2, E | 8),
	res8!(2, H | 8),
	res8!(2, L | 8),
	res8!(2, (HL) | 16),
	res8!(2, A | 8),
	res8!(3, B | 8),
	res8!(3, C | 8),
	res8!(3, D | 8),
	res8!(3, E | 8),
	res8!(3, H | 8),
	res8!(3, L | 8),
	res8!(3, (HL) | 16),
	res8!(3, A | 8),

	// Ax
	res8!(4, B | 8),
	res8!(4, C | 8),
	res8!(4, D | 8),
	res8!(4, E | 8),
	res8!(4, H | 8),
	res8!(4, L | 8),
	res8!(4, (HL) | 16),
	res8!(4, A | 8),
	res8!(5, B | 8),
	res8!(5, C | 8),
	res8!(5, D | 8),
	res8!(5, E | 8),
	res8!(5, H | 8),
	res8!(5, L | 8),
	res8!(5, (HL) | 16),
	res8!(5, A | 8),

	// Bx
	res8!(6, B | 8),
	res8!(6, C | 8),
	res8!(6, D | 8),
	res8!(6, E | 8),
	res8!(6, H | 8),
	res8!(6, L | 8),
	res8!(6, (HL) | 16),
	res8!(6, A | 8),
	res8!(7, B | 8),
	res8!(7, C | 8),
	res8!(7, D | 8),
	res8!(7, E | 8),
	res8!(7, H | 8),
	res8!(7, L | 8),
	res8!(7, (HL) | 16),
	res8!(7, A | 8),

	// Cx
	set8!(0, B | 8),
	set8!(0, C | 8),
	set8!(0, D | 8),
	set8!(0, E | 8),
	set8!(0, H | 8),
	set8!(0, L | 8),
	set8!(0, (HL) | 16),
	set8!(0, A | 8),
	set8!(1, B | 8),
	set8!(1, C | 8),
	set8!(1, D | 8),
	set8!(1, E | 8),
	set8!(1, H | 8),
	set8!(1, L | 8),
	set8!(1, (HL) | 16),
	set8!(1, A | 8),

	// Dx
	set8!(2, B | 8),
	set8!(2, C | 8),
	set8!(2, D | 8),
	set8!(2, E | 8),
	set8!(2, H | 8),
	set8!(2, L | 8),
	set8!(2, (HL) | 16),
	set8!(2, A | 8),
	set8!(3, B | 8),
	set8!(3, C | 8),
	set8!(3, D | 8),
	set8!(3, E | 8),
	set8!(3, H | 8),
	set8!(3, L | 8),
	set8!(3, (HL) | 16),
	set8!(3, A | 8),

	// Ex
	set8!(4, B | 8),
	set8!(4, C | 8),
	set8!(4, D | 8),
	set8!(4, E | 8),
	set8!(4, H | 8),
	set8!(4, L | 8),
	set8!(4, (HL) | 16),
	set8!(4, A | 8),
	set8!(5, B | 8),
	set8!(5, C | 8),
	set8!(5, D | 8),
	set8!(5, E | 8),
	set8!(5, H | 8),
	set8!(5, L | 8),
	set8!(5, (HL) | 16),
	set8!(5, A | 8),

	// Fx
	set8!(6, B | 8),
	set8!(6, C | 8),
	set8!(6, D | 8),
	set8!(6, E | 8),
	set8!(6, H | 8),
	set8!(6, L | 8),
	set8!(6, (HL) | 16),
	set8!(6, A | 8),
	set8!(7, B | 8),
	set8!(7, C | 8),
	set8!(7, D | 8),
	set8!(7, E | 8),
	set8!(7, H | 8),
	set8!(7, L | 8),
	set8!(7, (HL) | 16),
	set8!(7, A | 8)
];

pub struct FakeMemory {
	pub buf: [u8; 0x10000],
	pub write_enable: bool
}

impl FakeMemory {
	pub fn new() -> Self {
		Self { buf: [0u8; 0x10000], write_enable: false }
	}

	pub fn set_write_enabled(&mut self, enabled: bool) {
		self.write_enable = enabled;
	}
}

impl MemoryInterface for FakeMemory {
	fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
		if !self.write_enable {
			panic!("Attempting to write to {}", addr);
		}

		self.buf[addr as usize] = value;
		Ok(())
	}

	fn load8(&mut self, addr: u16) -> Result<u8> {
		Ok(self.buf[addr as usize])
	}
}


#[cfg(test)]
mod tests {
	use super::*;

	struct CPUTest {
		cpu: CPU,
		mem: FakeMemory,
		intr: RefCell<InterruptState>
	}

	impl CPUTest {
		fn new() -> Self {
			let mut inst = Self {
				cpu: CPU::default(),
				mem: FakeMemory::new(),
				intr: RefCell::new(InterruptState::default())
			};

			// Set to a reasonably high value to avoid colliding with
			// instructions.
			inst.cpu.registers.SP = 0x8000;

			inst
		}

		fn run(&mut self, codes: &[u8]) {
			self.cpu.registers.PC = 0;
			self.mem.buf[0..codes.len()].copy_from_slice(&codes);

			while (self.cpu.registers.PC as usize) < codes.len() {
				self.cpu.step_full(&mut self.mem, &self.intr).unwrap();
			}

			assert_eq!(self.cpu.registers.PC as usize, codes.len());
		}
	}

	#[test]
	fn test_inc16() {
		let mut this = CPUTest::new();

		{
			this.cpu.registers.BC = 0x1234;
			this.cpu.registers.unsafe_f_mut().set(0);
			let initial_regs = this.cpu.registers.clone();
			this.run(&[0x03]); // INC BC

			let mut expected_regs = initial_regs.clone();
			expected_regs.BC = 0x1235;
			expected_regs.PC = 1;
			assert_eq!(this.cpu.registers, expected_regs);
		}
		// Existing flags should not get unset.
		{
			this.cpu.registers.DE = 0x8000;
			this.cpu.registers.unsafe_f_mut().set(0xff);
			let initial_regs = this.cpu.registers.clone();
			this.run(&[0x13]); // INC DE

			let mut expected_regs = initial_regs.clone();
			expected_regs.DE = 0x8001;
			expected_regs.PC = 1;
			assert_eq!(this.cpu.registers, expected_regs);
		}
		// Overflow
		{
			this.cpu.registers.SP = 0xffff;
			let initial_regs = this.cpu.registers.clone();
			this.run(&[0x33]); // INC SP

			let mut expected_regs = initial_regs.clone();
			expected_regs.SP = 0x0000;
			expected_regs.PC = 1;
			assert_eq!(this.cpu.registers, expected_regs);
		}
	}

	#[test]
	fn test_or8() {
		let mut this = CPUTest::new();

		this.cpu.registers.a_mut().set(0xf0);
		this.cpu.registers.b_mut().set(0x04);
		this.cpu.registers.set_flag_z(true);
		let initial_regs = this.cpu.registers.clone();
		this.run(&[0xB0]); // OR B

		let mut expected_regs = initial_regs.clone();
		expected_regs.a_mut().set(0xf4);
		expected_regs.set_flag_z(false);
		expected_regs.PC = 1;
		assert_eq!(this.cpu.registers, expected_regs);
	}

	#[test]
	fn test_cp8() {
		let mut this = CPUTest::new();

		this.cpu.registers.a_mut().set(0x10);
		this.cpu.registers.c_mut().set(2);
		let initial_regs = this.cpu.registers.clone();
		this.run(&[0xB9]); // OR B

		let mut expected_regs = initial_regs.clone();
		expected_regs.set_flag_z(false);
		expected_regs.set_flag_n(true);
		expected_regs.set_flag_h(true);
		expected_regs.set_flag_c(false);
		expected_regs.PC = 1;
		assert_eq!(this.cpu.registers, expected_regs);

	}

	#[test]
	fn it_works() {
		let mut this = CPUTest::new();
		this.mem.set_write_enabled(true);

		let codes: &'static [u8] = &[
			0x01, 0x01, 0x12, // ld bc, 0x1200
			0xc5, // push bc
			0xf1, // pop af
			0xf5, // push af
			0xd1, // pop de
			0x79, // ld a, c
			0xe6, 0xf0, // and 0xf0
			0xbb, // cp e
		];
		this.mem.buf[0..codes.len()].copy_from_slice(&codes);

		for i in 0..8 {
			this.cpu.step_full(&mut this.mem, &this.intr).unwrap();
//			println!("{:x?} {}", this.cpu.registers, this.cpu.registers.flag_z());
		}
	}
}

/*
	Executing an instruction:
	- Need registers and need memory
*/


// Instruction table: https://www.pastraiser.com/cpu/gameboy/gameboy_opcodes.html

// Array.from(document.getElementsByTagName('td')).map((el) => { return el.innerText.split('\n')[0]; }).join('\n')