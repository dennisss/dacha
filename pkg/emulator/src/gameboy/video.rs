use std::cell::RefCell;
use std::rc::Rc;
use common::bits::{bitget, bitset};
use crate::gameboy::memory::MemoryInterface;
use common::errors::*;
use crate::gameboy::clock::*;
use crate::gameboy::memory::{InterruptState, InterruptType};

pub const SCREEN_HEIGHT: usize = 144;
pub const SCREEN_WIDTH: usize = 160;

const FRAME_WIDTH: u64 = 256;
const FRAME_HEIGHT: u64 = 256;

/// Width/height of a single tile.
const TILE_DIMENSION: u8 = 8;

const BYTES_PER_TILE: usize = 16;

const OAM_SEARCH_CYCLES: u64 = 20;
const PIXEL_TRANSFER_CYCLES: u64 = 43;
const H_BLANK_CYCLES: u64 = 51;
const CYCLES_PER_LINE: u64 =
	OAM_SEARCH_CYCLES + PIXEL_TRANSFER_CYCLES + H_BLANK_CYCLES;

const VERTICAL_LINES: u64 = 154;

// (( (134052 + 4 + 4 + 4) / 4) / (20 + 43 + 51)) % 154

// TODO: Rename 'Pixel Processing Unit'

/*
VRAM is 8000-9FFF
OAM: FE00h-FE9F

When displayying, VRAM and

*/
// NOTE: VRAM/OAM is always accessible when display is off.


/*

Display is 160 x 144 pixels

V-Blank is [144, 153]

9198

Processor clock speed is: 4.194304 MHz
Frame rate: 59.73 Hz

NOTE: We assume that the VideoController steps before the CPU??

Timing:
- Horizontal Line drawing:
	OAM Search: 20 clocks
	Pixel Transfer 43 clocks
	H-Blank: 51 clocks
	- Where clock speed is 1,048,576 Hz

One line is 114 cycles

- so one screen is 154*114 = 17,556 clock cycles
*/


// TODO: If interrupts are re-enabled, will old interrupts still be triggered.
// TODO: Can an interrupt run while an interrupt is running?




/// TODO: Debug with gzip.rs
fn bitgetv(v: u8, bit: u8) -> u8 {
	if v & (1 << bit) != 0 {
		1
	} else {
		0
	}
}



/// Abstraction around the LCD state, VRAM, etc.
pub struct VideoController {

	clock: Rc<RefCell<Clock>>,

	interrupts: Rc<RefCell<InterruptState>>,

	/// The actual pixels currently drawn to the screen (assuming the screen is
	/// on).
	pub screen_buffer: [u32; (SCREEN_HEIGHT*SCREEN_WIDTH) as usize],

	/// Each tile is 16 bytes to represent 8x8 pixels
	/// 8000-8FFF : Sprite/background tiles (numbered 0 to 255)
	/// 8800-97FF : Background/window tiles (numbered -128 to 127)
	///
	/// Background tile maps are 32x32 (so 256 x 256 pixels)
	/// 9800h-9BFF : First map
	/// 9C00h-9FFF : second map
	/// (either usable as 'background' or 'window')
	vram: VideoRAM,

	/// This continues 40 sprite entries each 4 bytes in size.
	/// FE00 - FE9F (R/W)
	oam: [u8; 160],

	registers: VideoRegisters
}

impl VideoController {
	pub fn new(clock: Rc<RefCell<Clock>>,
			   interrupts: Rc<RefCell<InterruptState>>) -> Self {
		Self {
			clock, interrupts,
			screen_buffer: [0u32; (SCREEN_HEIGHT*SCREEN_WIDTH) as usize],
			vram: VideoRAM::default(),
			oam: [0u8; 160],
			registers: VideoRegisters::default()
		}
	}

	fn draw_line(&mut self, screen_y: u8) -> Result<()> {

		let mut line_color_nums = [ColorNumber(0); SCREEN_WIDTH];

		// Draw background.
		{
			let tile_map = self.vram.tile_map(
				self.registers.control.background_tile_map_select());

			let tile_data = self.vram.tile_data(
				self.registers.control.background_window_tile_data_select());

			for screen_x in 0..(SCREEN_WIDTH as u8) {
				// Wrapping here assumes that the frame/background is 256 pixels
				// in height/width.
				let frame_x = screen_x.wrapping_add(self.registers.scroll_x);
				let frame_y = screen_y.wrapping_add(self.registers.scroll_y);

				let tile_ptr = tile_map.lookup(frame_x, frame_y);
				let tile = tile_data.get(tile_ptr.number);

//				if tile_ptr.number != 0 && tile_ptr.number != {
//					println!("TILE NUMBER {}", tile_ptr.number);
//				}

				let color_num = tile.get(frame_x - tile_ptr.x,
										 frame_y - tile_ptr.y);
				let color = self.registers.background_pallete.color(color_num);

				line_color_nums[screen_x as usize] = color_num;

				let pixel = &mut self.screen_buffer[
					(screen_y as usize)*SCREEN_WIDTH + (screen_x as usize)];
				*pixel = color.to_rgb();
			}
		}

		// TODO: THe window calcualtions are probably wrong

		// Draw window.
		// TODO: Deduplicate with above.
		if self.registers.control.window_display_enabled() &&
			screen_y >= self.registers.window_y {

			let tile_map = self.vram.tile_map(
				self.registers.control.window_tile_map_select());

			let tile_data = self.vram.tile_data(
				self.registers.control.background_window_tile_data_select());

			let start_x =
				if self.registers.window_x >= 7 {
					self.registers.window_x - 7
				} else { 0 };

			for screen_x in start_x..(SCREEN_WIDTH as u8) {
				let mut frame_x = screen_x - start_x;
				if self.registers.window_x < 7 {
					frame_x += 7 - self.registers.window_x;
				}

				let frame_y = screen_y - self.registers.window_y;

				let tile_ptr = tile_map.lookup(frame_x, frame_y);
				let tile = tile_data.get(tile_ptr.number);

				let color_num = tile.get(frame_x - tile_ptr.x,
										 frame_y - tile_ptr.y);
				let color = self.registers.background_pallete.color(color_num);

				line_color_nums[screen_x as usize] = color_num;

				let pixel = &mut self.screen_buffer[
					(screen_y as usize)*SCREEN_WIDTH + (screen_x as usize)];
				*pixel = color.to_rgb();
			}

		}



		// Draw sprites.
		if self.registers.control.object_display_enabled() {
			// TODO: Does a sprite that is not visible count towards the 10
			// sprites per line limit?

			// Always uses the tile data at 0x8000.
			let tile_data = self.vram.tile_data(true);

			let draw_sprite_tile = |
				obj: &ObjectAttributes, tile: &Tile, position_y: u8, pallete: &MonochromePallete,
				screen_buffer: &mut [u32]| {
				// Make sure the subtraction after this doesn't go negative.
				if screen_y + 16 < position_y {
					return;
				}

				// Will be the y position from 0-7 relative to the current tile
				// to draw at the current line.
				let tile_y = screen_y + 16 - position_y;
				if tile_y >= 8 {
					return;
				}

				// Ensuring the below screen_x calculations don't overflow
				if obj.position_x >= (SCREEN_WIDTH as u8) + 8 {
					return;
				}

				for tile_x in 0..8 {
					let screen_x = match (obj.position_x + tile_x).checked_sub(8) {
						Some(x) => x,
						None => { continue; }
					};

					if screen_x as usize >= SCREEN_WIDTH {
						break;
					}

					let color_num = tile.get(
						if obj.x_flip() { 7 - tile_x } else { tile_x },
						if obj.y_flip() { 7 - tile_y } else { tile_y });
					// Transparent
					if color_num.0 == 0 {
						continue;
					}

					if line_color_nums[screen_x as usize].0 == 0 {
						// always in front of bg/window color 0
					} else {
						match obj.priority() {
							ObjectPriority::AboveBackground => {},
							ObjectPriority::BehindNonZero => {
								continue;
							}
						}
					}

					let color = pallete.color(color_num);

					let pixel = &mut screen_buffer[
						(screen_y as usize)*SCREEN_WIDTH + (screen_x as usize)];
					*pixel = color.to_rgb();
				}
			};

			let mut rest = &self.oam[..];
			loop {
				let obj = match ObjectAttributes::next(rest) {
					Some((o, r)) => {
						rest = r;
						o
					},
					None => { break; }
				};

				let pallete =
					if obj.upper_pallete() {
						&self.registers.object_pallete1 }
					else { &self.registers.object_pallete0 };

				if self.registers.control.object_big() {
					let lower_tile = tile_data.get(obj.tile_number & 0xFE);
					draw_sprite_tile(
						&obj, &lower_tile, obj.position_y, pallete,
						&mut self.screen_buffer);

					println!("DRAW BIG");

					let (upper_tile_pos, carry) = obj.position_y.overflowing_add(8);
					if !carry {
						let upper_tile = tile_data.get(obj.tile_number | 0x01);
						draw_sprite_tile(
							&obj, &upper_tile, upper_tile_pos, pallete,
							&mut self.screen_buffer);

					}

				} else {
					let tile = tile_data.get(obj.tile_number);
					draw_sprite_tile(&obj, &tile, obj.position_y, pallete, &mut self.screen_buffer);
				}
			}


//			for screen_x in 0..(SCREEN_WIDTH as u8) {
//
//				let mut rest = &self.oam[..];
//				loop {
//					let obj = match ObjectAttributes::next(rest) {
//						Some((o, r)) => {
//							rest = r;
//							o
//						},
//						None => { break; }
//					};
//				}
//			}
		}

		Ok(())
	}


	/// Applies the effect of a single clock cycle.
	pub fn step(&mut self) -> Result<()> {
		// If the screen is off, clear the screen.
		// TODO: Only do this if not already cleared
		if !self.registers.control.display_enabled() {
			for pixel in self.screen_buffer.iter_mut() {
				*pixel = 0xffffff;
			}

			return Ok(());
		}

		let now = self.clock.borrow().now();
		let y = ((now.cycles_1mhz() / CYCLES_PER_LINE) % VERTICAL_LINES) as usize;

		let x = (now.cycles_1mhz() % CYCLES_PER_LINE) as usize;

		self.registers.counter_y = y as u8;

		if now.cycles_1mhz() == (134068 / 4) {
			println!("VALUE {}", y);
		}

		let coincide_y = self.registers.counter_y == self.registers.compare_y;
		self.registers.status.set_coincidence_flag(coincide_y);

		if y < SCREEN_HEIGHT {
			if x == 0 {
				self.registers.status.set_mode(LcdMode::OAMSearch);

				// Trigger OAM interrupt
				if coincide_y &&
					self.registers.status.ly_coincidence_interrupt_enabled() {
					println!("Fire 1");
					self.interrupts.borrow_mut().trigger(InterruptType::LCDStat);
				}

				if self.registers.status.oam_interrupt_enabled() {
					println!("Fire 2");
					self.interrupts.borrow_mut().trigger(InterruptType::LCDStat);
				}
			} else if x == OAM_SEARCH_CYCLES as usize {
				// TODO: ^ Check the above condition value.
				self.registers.status.set_mode(LcdMode::Transferring);
			} else if x == (OAM_SEARCH_CYCLES + PIXEL_TRANSFER_CYCLES - 1) as usize {
				// Draw line on last cycle of pixel transfer.
				self.draw_line(y as u8)?;
			} else if x == (OAM_SEARCH_CYCLES + PIXEL_TRANSFER_CYCLES) as usize {
				self.registers.status.set_mode(LcdMode::HBlank);

				// Trigger H-blank on first cycle of this region.
				if self.registers.status.hblank_interrupt_enabled() {
					println!("Fire 3");
					self.interrupts.borrow_mut().trigger(InterruptType::LCDStat);
				}
			}
		} else {
			// In V-Blank region.
			self.registers.status.set_mode(LcdMode::VBlank);

			// Queue an interrupt on the first pixel in this region.
			if y == SCREEN_HEIGHT && x == 0 {
				self.interrupts.borrow_mut().trigger(InterruptType::VBlank);

				if self.registers.status.vblank_interrupt_enabled() {
					println!("Fire 4");
					self.interrupts.borrow_mut().trigger(InterruptType::LCDStat);
				}
			}
		}

		Ok(())
	}
}

impl MemoryInterface for VideoController {
	fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
		match addr {
			0x8000..=0x9FFF => {
				self.vram.data[(addr - 0x8000) as usize] = value;
			},
			0xFE00..=0xFE9F => { self.oam[(addr - 0xFE00) as usize] = value; },
			_ => { return self.registers.store8(addr, value); }
		}

		Ok(())
	}

	fn load8(&mut self, addr: u16) -> Result<u8> {
		Ok(match addr {
			0x8000..=0x9FFF => {
				self.vram.data[(addr - 0x8000) as usize] },
			0xFE00..=0xFE9F => { self.oam[(addr - 0xFE00) as usize] },
			_ => { return self.registers.load8(addr); }
		})
	}
}

#[derive(Default)]
struct VideoRegisters {
	/// FF40 (R/W)
	control: LcdControlRegister,

	/// FF41
	status: LcdStatusRegister,

	/// FF42 (R/W)
	scroll_y: u8,

	/// FF43 (R/W)
	scroll_x: u8,

	/// Current between 0-153 of the current Y coordinate being transferred.
	/// Writing will reset the counter?
	/// FF44
	counter_y: u8,

	/// FF45 (R/W)
	compare_y: u8,

	/// FF4A (R/W)
	window_y: u8,

	/// FF4B (R/W)
	window_x: u8,

	/// FF47 (R/W)
	background_pallete: MonochromePallete,

	/// FF48 (R/W)
	object_pallete0: MonochromePallete,

	/// FF49 (R/W)
	object_pallete1: MonochromePallete,
}

impl MemoryInterface for VideoRegisters {
	fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
		match addr {
			0xFF40 => { self.control.value = value; },
			0xFF41 => { self.status.set(value); }
			0xFF42 => { self.scroll_y = value; },
			0xFF43 => { self.scroll_x = value; },
			0xFF44 => { panic!("Setting y counter not implemented"); self.counter_y = value; },
			0xFF45 => { panic!("Compare y not implemented"); self.compare_y = value; },
			0xFF47 => { self.background_pallete.value = value; }
			0xFF48 => { self.object_pallete0.value = value; },
			0xFF49 => { self.object_pallete1.value = value; },
			0xFF4A => { self.window_y = value; },
			0xFF4B => { self.window_x = value; },
			_ => { return Err(err_msg(format!("Unknown address {:04x}", addr))) }
		}

		Ok(())
	}

	fn load8(&mut self, addr: u16) -> Result<u8> {
		Ok(match addr {
			0xFF40 => { self.control.value },
			0xFF41 => { self.status.value },
			0xFF42 => { self.scroll_y },
			0xFF43 => { self.scroll_x },
			0xFF44 => { self.counter_y },
			0xFF45 => { self.compare_y } ,
			0xFF47 => { self.background_pallete.value }
			0xFF48 => { self.object_pallete0.value },
			0xFF49 => { self.object_pallete1.value },
			0xFF4A => { self.window_y },
			0xFF4B => { self.window_x },
			_ => { return Err(err_msg(format!("Unknown address {:04x}", addr))) }
		})
	}
}

#[derive(Default)]
struct LcdControlRegister {
	value: u8
}

impl LcdControlRegister {
	fn display_enabled(&self) -> bool { bitget(self.value, 7) }
	fn window_tile_map_select(&self) -> bool { bitget(self.value, 6) }
	fn window_display_enabled(&self) -> bool { bitget(self.value, 5) }
	fn background_window_tile_data_select(&self) -> bool { bitget(self.value, 4) }
	fn background_tile_map_select(&self) -> bool { bitget(self.value, 3) }
	/// If true, then sprites are 8x16, otherwise they are 8x8.
	fn object_big(&self) -> bool { bitget(self.value, 2) }

	fn object_display_enabled(&self) -> bool { bitget(self.value, 1) }

	// TODO: Verify that bit 0 is unused for non CGB
}

/// TODO: A value of zero may not be a reasonable default?
#[derive(Default)]
struct LcdStatusRegister {
	value: u8
}

impl LcdStatusRegister {
	fn set(&mut self, value: u8) {
		// Lower 3 bits are read only.
		self.value = (value & !0b111) | (self.value & 0b111);
	}

	fn ly_coincidence_interrupt_enabled(&self) -> bool { bitget(self.value, 6) }
	fn oam_interrupt_enabled(&self) -> bool { bitget(self.value, 5) }
	fn vblank_interrupt_enabled(&self) -> bool { bitget(self.value, 4) }
	fn hblank_interrupt_enabled(&self) -> bool { bitget(self.value, 3) }

	fn set_coincidence_flag(&mut self, equal: bool) {
		bitset(&mut self.value, equal, 2);
	}

	fn set_mode(&mut self, mode: LcdMode) {
		self.value = (self.value & !0b11) | (mode as u8);
	}
}

enum LcdMode {
	HBlank = 0,
	VBlank = 1,
	OAMSearch = 2,
	Transferring = 3
}


struct ObjectAttributes {
	position_y: u8,

	position_x: u8,

	tile_number: u8,

	flags: u8
}

impl ObjectAttributes {
	fn next(data: &[u8]) -> Option<(Self, &[u8])> {
		if data.len() < 4 {
			None
		} else {
			let (buf, rest) = data.split_at(4);
			let s = Self {
				position_y: buf[0],
				position_x: buf[1],
				tile_number: buf[2],
				flags: buf[3]
			};
			Some((s, rest))
		}
	}

	fn priority(&self) -> ObjectPriority {
		if bitget(self.flags, 7) {
			ObjectPriority::BehindNonZero
		} else {
			ObjectPriority::AboveBackground
		}
	}

	fn y_flip(&self) -> bool { bitget(self.flags, 6) }
	fn x_flip(&self) -> bool { bitget(self.flags, 5) }

	/// If true, use object pallete 1 instead of 0.
	fn upper_pallete(&self) -> bool { bitget(self.flags, 4) }
}

enum ObjectPriority {
	/// Sprite is always drawn above the background/window.
	AboveBackground,
	/// Sprite is drawn behind background/window colors 1-3
	BehindNonZero
}

//enum_def!(MonochromeColor u8 =>
enum MonochromeColor {
	White = 0,
	LightGray = 1,
	DarkGray = 2,
	Black = 3
}

impl MonochromeColor {
	fn from_value(value: u8) -> Self {
		match value {
			0 => Self::White,
			1 => Self::LightGray,
			2 => Self::DarkGray,
			3 => Self::Black,
			_ => panic!("Invalid color")
		}
	}

	fn to_rgb(&self) -> u32 {
		match self {
			MonochromeColor::White => 0xffffff,
			MonochromeColor::LightGray => 0xaaaaaa,
			MonochromeColor::DarkGray => 0x555555,
			MonochromeColor::Black => 0x000000
		}
	}
}

#[derive(Default)]
struct MonochromePallete {
	value: u8
}

impl MonochromePallete {
	/// Converts a 0-3 number in a tile to a color.
	fn color(&self, number: ColorNumber) -> MonochromeColor {
		assert!(number.0 < 4);
		let offset = number.0 * 2;
		let raw = (self.value >> offset) & 0b11;
		MonochromeColor::from_value(raw)
	}
}

struct VideoRAM {
	data: [u8; 8192]
}

impl Default for VideoRAM {
	fn default() -> Self {
		Self { data: [0u8; 8192] }
	}
}

impl VideoRAM {
	fn tile_data(&self, upper: bool) -> TileData {
		if upper {
			TileData {
				data: &self.data[(0x8000 - 0x8000)..(0x9000 - 0x8000)],
				offset: 0
			}
		} else {
			TileData {
				data: &self.data[(0x8800 - 0x8000)..(0x9800 - 0x8000)],
				offset: 128
			}
		}
	}

	fn tile_map(&self, upper: bool) -> TileMap {
		TileMap {
			data: if upper {
				&self.data[(0x9C00 - 0x8000)..(0xA000 - 0x8000)]
			} else {
				&self.data[(0x9800 - 0x8000)..(0x9C00 - 0x8000)]
			}
		}
	}
}

struct TileMap<'a> {
	/// Will always be 32x32 bytes (where each byte is one tile number).
	data: &'a [u8]
}

struct TilePointer {
	number: u8,
	x: u8,
	y: u8
}

const TILES_PER_ROW: u16 = 32;

impl TileMap<'_> {
	fn lookup(&self, x: u8, y: u8) -> TilePointer {
		let offset =
			(((y / TILE_DIMENSION) as u16) * TILES_PER_ROW) +
				((x / TILE_DIMENSION) as u16);

		let tile_x = ((offset % TILES_PER_ROW) as u8) * TILE_DIMENSION;
		let tile_y = ((offset / TILES_PER_ROW) as u8) * TILE_DIMENSION;

		TilePointer {
			number: self.data[offset as usize],
			x: tile_x as u8,
			y: tile_y as u8
		}
	}
}



struct TileData<'a> {
	data: &'a [u8],
	/// Tile number which corresponds to the tile at offset 0 in data.
	offset: u8
}

impl<'a> TileData<'a> {
	fn get(&self, number: u8) -> Tile {
//		if number != 0 {
//			println!("GET {}", number);
//		}
//		let num =
//			if self.offset == 0 {
//				number
//			} else {
//				(((number as i8) as i16) + 128) as u8
//			};

		let off = (number.wrapping_add(self.offset) as usize) * BYTES_PER_TILE;
		Tile {
			data: &self.data[off..(off + BYTES_PER_TILE)]
		}
	}
}

struct Tile<'a> {
	data: &'a [u8]
}

#[derive(Clone, Copy)]
struct ColorNumber(u8);

impl Tile<'_> {
	/// Finds the color in a tile given it's relative position in the tile.
	fn get(&self, x: u8, y: u8) -> ColorNumber {
		assert!(x < TILE_DIMENSION as u8);
		assert!(y < TILE_DIMENSION as u8);

		let off = 2*y as usize; // 2 bytes per line.
		let bit = 7 - x;

		ColorNumber(bitgetv(self.data[off], bit) |
					(bitgetv(self.data[off + 1], bit) << 1))
	}
}


