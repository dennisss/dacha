use gameboy::memory::MemoryInterface;
use errors::*;

// TODO: Dedup these functions with everywhere.
fn bitset(i: &mut u8, val: bool, bit: u8) {
	let mask = 1 << bit;
	*i = (*i & !mask);
	if val {
		*i |= mask;
	}
}

fn bitget(v: u8, bit: u8) -> bool {
	if v & (1 << bit) != 0 {
		true
	} else {
		false
	}
}


enum JoypadMode {
	ButtonKeys,
	DirectionKeys
}

impl Default for JoypadMode {
	// TODO: Check this
	fn default() -> Self {
		JoypadMode::ButtonKeys
	}
}


#[derive(Default)]
pub struct Joypad {
	mode: JoypadMode,

	pub up_pressed: bool,
	pub down_pressed: bool,
	pub left_pressed: bool,
	pub right_pressed: bool,

	pub start_pressed: bool,
	pub select_pressed: bool,
	pub a_pressed: bool,
	pub b_pressed: bool
}

impl MemoryInterface for Joypad {
	fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
		if addr != 0xff00 { return Err(err_msg("Unsupported joypad address")); }

		let direction_enable = bitget(value, 4);
		let buttons_enable = bitget(value, 5);

		// TODO: It does seem possible to have neither selected.
//		if !(direction_enable ^ buttons_enable) {
//			println!("Joypad byte {:X?}", value);
//			return Err(err_msg("Expected to select exactly one joypad mode"));
//		}

		if direction_enable { self.mode = JoypadMode::DirectionKeys; }
		if buttons_enable { self.mode = JoypadMode::ButtonKeys; }
		Ok(())
	}

	fn load8(&mut self, addr: u16) -> Result<u8> {
		if addr != 0xff00 { return Err(err_msg("Unsupported joypad address")); }

		let mut value = 0xff;
		match self.mode {
			JoypadMode::ButtonKeys => {
				bitset(&mut value, false, 5);
				bitset(&mut value, !self.a_pressed, 0);
				bitset(&mut value, !self.b_pressed, 1);
				bitset(&mut value, !self.select_pressed, 2);
				bitset(&mut value, !self.start_pressed, 3);
			},
			JoypadMode::DirectionKeys => {
				bitset(&mut value, false, 4);
				bitset(&mut value, !self.right_pressed, 0);
				bitset(&mut value, !self.left_pressed, 1);
				bitset(&mut value, !self.up_pressed, 2);
				bitset(&mut value, !self.down_pressed, 3);
			}
		}

		Ok(value)
	}
}