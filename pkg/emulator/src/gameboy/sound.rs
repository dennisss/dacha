
/*
Mapped to memory:
Channel 1: FF10 - FF14
Channel 2: FF16 - FF19
Channel 3: FF1A - FF1E
Channel 4: FF20 - FF23
Registers: FF24 - FF26
*/

use std::sync::{Mutex, Arc};
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use common::bits::{bitget, bitset};
use crate::gameboy::memory::MemoryInterface;
use crate::errors::*;
use crate::gameboy::clock::Clock;

const NUM_CHANNELS: usize = 4;

const APP_VOLUME: f32 = 0.2;

pub struct SoundController {
	host: cpal::Host,
	device: cpal::Device,
	stream: cpal::Stream,

	pub state: Arc<Mutex<SoundControllerState>>
}

impl SoundController {
	pub fn new() -> Result<Self> {
		let state = Arc::new(Mutex::new(SoundControllerState::default()));

		let host = cpal::default_host();
		let device = host.default_output_device()
			.ok_or(err_msg("No default sound device"))?;
		let config = device.default_output_config()?;

		println!("Format: {:?}", config);

		if config.channels() != 2 {
			return Err(err_msg("Only dual channel audio is supported."));
		}

		// TODO: We need to determine how far forward we buffer sound so that
		// we can respond to changes in memory quick enough for all effects.

		let mut sample_index = 0;
		let sample_rate = config.sample_rate().0 as u64;
		let state2 = state.clone();

		let stream = device.build_output_stream(
			&config.into(),
			// TODO: Check if this format is supported?
			move |data: &mut [f32]| {
				let mut guard = state2.lock().unwrap();
				guard.compute(data, sample_index, sample_rate);
				sample_index += data.len() as u64;
			},
			|err| eprintln!("an error occurred on stream: {}", err),
			)?;
		stream.play()?;

		Ok(Self {
			host, device, stream, state
		})
	}

	/// NOTE: We expect that this will be called at 512Hz
	pub fn step(&self, clock: &Clock) -> Result<()> {
		let mut state = self.state.lock().unwrap();
		state.step_512hz(clock);
		Ok(())
	}
}


#[derive(Default)]
pub struct SoundControllerState {
	/// Step from 0-7. Incremented at 512Hz. Reset whenever the sound controller
	/// is turned on (like everything else)
	frame_seq: u8,

	channel1: Channel1,
	channel2: Channel2,
	channel3: Channel3,
	channel4: Channel4,

	// FF24
	channel_control: u8,

	/// For each output terminal, selects which channels will contribute to
	/// that terminal's final output.
	/// FF25
	output_select: u8,

	// FF26
	sound_on_off: SoundOnOffRegister
}

impl SoundControllerState {
	pub fn step_512hz(&mut self, clock: &Clock) {
		// 256Hz for length control
		if self.frame_seq % 2 == 0 {
			if self.channel1.counting &&
				self.sound_on_off.channel_enabled(Channel::Sound1) {
				if self.channel1.length_remaining == 0 {
					// NOTE: We disable the channel at the beginning of the
					// cycle after the last cycle so that the background audio
					// thread still runs the last cycle.
					self.sound_on_off.disable_channel(Channel::Sound1);
				} else {
					self.channel1.length_remaining -= 1;
				}
			}

			// TODO: Implement consistently for all of the channels.
		}

		// 128Hz for sweep control
		if (self.frame_seq % 2 == 0) && (self.frame_seq % 4 != 0) {

		}

		// 64Hz for volume control
		if self.frame_seq == 7 {
			self.channel1.volume.step_64hz();
			self.channel2.volume.step_64hz();
			self.channel4.volume.step_64hz();
		}

		let cycles = clock.now().cycles_512hz();
		self.frame_seq = (self.frame_seq + 1) % 8;
	}

	/// Returns whether or not the sound controller is aware of the given memory
	/// address. If it returns false, then memory i/o to that address will
	/// always fail because it is handled by another device..
	pub fn addr_mapped(addr: u16) -> bool {
		match addr {
			0xFF10..=0xFF14 | 0xFF16..=0xFF19 | 0xFF1A..=0xFF1E |
			0xFF20..=0xFF23 | 0xFF24..=0xFF26 | 0xFF30..=0xFF3F => true,
			_ => false
		}
	}

	// We will have a sample clock.
	//

	//
	fn compute(&self, data: &mut [f32], sample_index: u64, sample_rate: u64) {
		if !self.sound_on_off.global_on() {
			return;
		}

//		println!("{} {} {}", self.channel1.length_duty.duty_cycle(),
//				 self.channel1.frequency.period(), self.channel1.volume.volume());

		for (i, sample) in data.chunks_mut(2).enumerate() {
			// TODO: This can be done at a better precision by splitting up the
			// decimal and integer calculation?
			let time =
				((sample_index + (i as u64)) as f64) / (sample_rate as f64);

			let vals = self.compute_sample(time);
			sample[0] = APP_VOLUME * vals.0;
			sample[1] = APP_VOLUME * vals.1;
		}
	}

	/// time: Number of seconds elapsed since start of audio playback.
	fn compute_sample(&self, time: f64) -> (f32, f32) {
		let mut channels = [0.0f32; NUM_CHANNELS];

		// TODO: Optimize most of this logic out if the volumes are 0.

		if self.sound_on_off.channel_enabled(Channel::Sound1) {
			let mut value = -1.0;

			let duty = self.channel1.length_duty.duty_cycle();
			// TODO: Must be a dynamic frequency.
			let period = self.channel1.frequency.period();

			let pos = (time % period) / period;
			if pos < duty {
				value = 1.0;
			}

			value *= self.channel1.volume.volume();

			channels[0] = value;
		}

		if self.sound_on_off.channel_enabled(Channel::Sound2) {
			let mut value = -1.0;

			let duty = self.channel2.length_duty.duty_cycle();
			let period = self.channel2.frequency.period();

			let pos = (time % period) / period;
			if pos < duty {
				value = 1.0;
			}

			value *= self.channel2.volume.volume();

			channels[1] = value;
		}

		if self.sound_on_off.channel_enabled(Channel::Sound3) {

		}

		if self.sound_on_off.channel_enabled(Channel::Sound4) {

		}

		let so1 = self.mix_terminal(SoundOutputTerminal::S01, &channels);
		let so2 = self.mix_terminal(SoundOutputTerminal::SO2, &channels);

		(so1, so2)
	}


	/// Given the current values of all channels, computes the
	// Output can be 0 for SO1 and 1 for SO2
	fn mix_terminal(&self, terminal: SoundOutputTerminal,
					channels: &[f32; 4]) -> f32 {
		let mut value = 0.0;

		let terminal_offset = 4*(terminal as u8);
		for i in 0..channels.len() {
			if bitget(self.output_select, (i as u8) + terminal_offset) {
				value += channels[i];
			}
		}

		if value > 1.0 {
			value = 1.0;
		} else if value < -1.0 {
			value = -1.0;
		}

		let terminal_offset = 3*(terminal as u8);
		// Volume from 0-7
		let volume = (self.channel_control >> terminal_offset) & 0b11;

		value *= (volume as f32) / 7.0;

		value
	}
}

// FF11 -> 80
// ff26 -> 80


// Starting sound 1:
// 0xFF13  <- 0x83
// 0xFF14  <- 0x87

impl MemoryInterface for SoundControllerState {
	fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
		if !self.sound_on_off.global_on() && addr != 0xFF26 {
			return Err(err_msg("Writing sound registers while off"));
		}

		match addr {
			0xFF10 => self.channel1.sweep.set(value),
			0xFF11 => self.channel1.length_duty.set(value),
			0xFF12 => self.channel1.volume.set(value),
			0xFF13 => self.channel1.frequency.set_lower(value),
			0xFF14 => {
				self.channel1.frequency.set_upper(value);
				// Bits 3-5 do nothing
				self.channel1.counting = bitget(value, 6);
				if bitget(value, 7) {
					self.channel1.volume.restart();
					self.channel1.length_remaining =
						self.channel1.length_duty.length();
				}
			},

			0xFF16 => self.channel2.length_duty.set(value),
			0xFF17 => self.channel2.volume.set(value),
			0xFF18 => self.channel2.frequency.set_lower(value),
			0xFF19 => {
				self.channel2.frequency.set_upper(value);
				// Bits 3-5 do nothing
				self.channel2.counting = bitget(value, 6);
				if bitget(value, 7) {
					self.channel2.volume.restart();
					self.channel2.length_remaining =
						self.channel2.length_duty.length();
				}
			},

			0xFF1A => { self.channel3.playing = bitget(value, 7); },
			0xFF1B => { self.channel3.length = value; },
			0xFF1C => self.channel3.output_level.set(value),
			0xFF1D => self.channel3.frequency.set_lower(value),
			0xFF1E => {
				self.channel3.frequency.set_upper(value);
				self.channel3.length_remaining = self.channel3.length();
			},

			0xFF20 => { self.channel4.length.set(value & 0b111111); },
			0xFF21 => self.channel4.volume.set(value),
			0xFF22 => { self.channel4.polynomial_counter = value; },
			0xFF23 => {
				self.channel4.counting = bitget(value, 6);
				if bitget(value, 7) {
					// Reset channel
					self.channel4.volume.restart();
					self.channel4.length_remaining =
						self.channel4.length.length();
				}
			},

			0xFF24 => {
				if bitget(value, 3) || bitget(value, 7) {
					return Err(err_msg("Vin mixing is not supported"));
				}

				self.channel_control = value;
			},
			0xFF25 => { self.output_select = value; },
			0xFF26 => {
				// Just the top bit is writeable
				let on = bitget(value, 7);
				if on {
					// TODO: What if sound was already on?
					// Mark all four channels as on.
					self.sound_on_off.value = 0b10001111;
				} else {
					// Clear all sound control registers on disabling sound.
					*self = SoundControllerState::default();
				}
			},

			0xFF30..=0xFF3F => {
				self.channel3.pattern[(addr - 0xFF30) as usize] = value; }
			_ => { return Err(err_msg("Unimplemented sound addr")) }
		}

		Ok(())
	}

	fn load8(&mut self, addr: u16) -> Result<u8> {
		if !self.sound_on_off.global_on() && addr != 0xFF26 {
			return Err(err_msg("Reading sound registers while off"));
		}

		Ok(match addr {
			0xFF10 => self.channel1.sweep.get(),
			0xFF11 => self.channel1.length_duty.get(),
			0xFF12 => self.channel1.volume.get(),
			0xFF13 => { return Err(err_msg("Write only")); },
			0xFF14 => {
				// Based on memory values observed after boot rom is done.
				let mut v = 0xff;
				bitset(&mut v, self.channel1.counting, 6);
				v
			},

			0xFF16 => self.channel2.length_duty.get(),
			0xFF17 => self.channel2.volume.get(),
			0xFF18 => { return Err(err_msg("Write only")); },
			0xFF19 => {
				// Based on memory values observed after boot rom is done.
				let mut v = 0xff;
				bitset(&mut v, self.channel2.counting, 6);
				v
			},

			0xFF1A => {
				// Based on memory values observed after boot rom is done.
				let mut v = 0xff;
				bitset(&mut v, self.channel3.playing, 7);
				v
			},
			0xFF1B => { return Err(err_msg("Write only")); },
			0xFF1C => self.channel3.output_level.get(),
			0xFF1D => { return Err(err_msg("Write only")); },
			0xFF1E => {
				// Based on memory values observed after boot rom is done.
				let mut v = 0xff;
				bitset(&mut v, self.channel3.counting, 6);
				v
			},

			0xFF20 => { self.channel4.length.get() | 0b11000000 },
			0xFF21 => self.channel4.volume.get(),
			0xFF22 => self.channel4.polynomial_counter,
			0xFF23 => {
				// Based on memory values observed after boot rom is done.
				let mut v = 0xff;
				bitset(&mut v, self.channel3.counting, 6);
				v
			},

			0xFF24 => { self.channel_control },
			0xFF25 => { self.output_select },
			0xFF26 => { self.sound_on_off.value },
			0xFF30..=0xFF3F => self.channel3.pattern[(addr - 0xFF30) as usize],
			_ => { return Err(err_msg("Unimplemented sound addr")) }
		})
	}
}

#[derive(Default)]
struct Channel1 {
	sweep: SweepEnvelope,
	length_duty: SoundLengthDutyCycleRegister,
	volume: VolumeEnvelope,
	frequency: Frequency,

	/// Whether or not this sound is being counted.
	counting: bool,

	/// NOTE: Will be up to 64 for all channels but channel 3 which can be up to
	/// 256.
	/// TODO: Use a tuple to ensure that is always represented in 256hz periods.
	length_remaining: usize,

	/// When counting is on, this will be the number of cycles remaining in the
	/// sound.
	length: usize,
}

#[derive(Default)]
struct Channel2 {
	length_duty: SoundLengthDutyCycleRegister,
	volume: VolumeEnvelope,
	frequency: Frequency,
	/// If in counter mode, how many

	length_remaining: usize,
	counting: bool
}

#[derive(Default)]
struct Channel3 {
	// Bit 7 of 0xFF1A
	playing: bool,
	length: u8,
	output_level: WaveOutputLevel,
	frequency: Frequency,
	pattern: [u8; 16],

	/// Number of
	length_remaining: usize,
	counting: bool,
}

impl Channel3 {
	pub fn length(&self) -> usize {
		256 - (self.length as usize)
	}
}

#[derive(Default)]
struct Channel4 {
	/// NOTE: Only the length part of this is used.
	length: SoundLengthDutyCycleRegister,

	volume: VolumeEnvelope,
	polynomial_counter: u8,

	control: u8,

	length_remaining: usize,
	counting: bool
}


#[derive(Default)]
pub struct SweepEnvelope {
	register: u8,
}

impl SweepEnvelope {
	fn set(&mut self, value: u8) {
		self.register = value;
	}
	fn get(&self) -> u8 {
		self.register | 0x80
	}
}


/// Register that specifies the duty cycle and time length of sounds.
/// For NR11, NR21, NR31
#[derive(Default)]
pub struct SoundLengthDutyCycleRegister {
	register: u8
}

impl SoundLengthDutyCycleRegister {
	fn set(&mut self, value: u8) {
		self.register = value;
	}

	fn get(&self) -> u8 {
		self.register & 0b11111
	}

	/// Gets the tone's duty cycle as a fractional percentage (0-1)
	fn duty_cycle(&self) -> f64 {
		let val = self.register >> 6;
		match val {
			0b00 => 0.125,
			0b01 => 0.25,
			0b10 => 0.5,
			0b11 => 0.75,
			// 'val' should only be 2 bits (and all of those cases are
			// handled above).
			_ => panic!("Should not happen")
		}
	}

	/// Gets the sound length in 256Hz period.
	fn length(&self) -> usize {
		let t1 = (self.register & 0b111111) as usize;
		64 - t1
	}
}


#[derive(Default)]
struct VolumeEnvelope {
	/// The configurable register NRx2
	register: u8,

	/// Current value of the above register being used.
	latched_register: u8,

	/// Current value of the volume from 0-15
	volume: u8,

	/// Number of cycles that have passed since the last reset or volume change.
	cycles: u8,
}

impl VolumeEnvelope {
	fn set(&mut self, value: u8) {
		self.register = value;
	}

	fn get(&self) -> u8 {
		self.register
	}

	fn volume(&self) -> f32 {
		(self.volume as f32) / 15.0
	}

	/// Updates the volume envelope. To be called at 64Hz
	fn step_64hz(&mut self) {
		let increasing = bitget(self.latched_register, 3);
		let period = self.latched_register & 0b111;

		if period == 0 {
			return;
		}

		self.cycles += 1;
		if self.cycles == period {
			self.cycles = 0;
			if increasing {
				if self.volume < 0xf {
					self.volume += 1;
				}
			} else {
				if self.volume > 0 {
					self.volume -= 1;
				}
			}
		}
	}

	/// To be called when sound is restarted (bit 7 of NRx4)
	fn restart(&mut self) {
		// Reset to initial volume.
		self.volume = self.register >> 4;

		self.latched_register = self.register;
		self.cycles = 0;
	}
}

/// For NRx3/NRx4 registers in channels 1-3
#[derive(Default)]
struct Frequency {
	value: u16,
}

impl Frequency {
	fn set_lower(&mut self, value: u8) {
		self.value = (self.value & 0xf00) | (value as u16);
	}

	fn set_upper(&mut self, value: u8) {
		self.value = (self.value & 0xff) | (((value as u16) & 0b111) << 8);
	}

	/// Gets the period in seconds
	fn period(&self) -> f64 {
		let hz = 131072.0 / (2048.0 - (self.value as f64));
		assert!(hz >= 0.0);
		1.0 / hz
	}
}

#[derive(Default)]
struct WaveOutputLevel {
	register: u8
}

impl WaveOutputLevel {
	fn volume(&self) -> f32 {
		match (self.register >> 5) & 0b11 {
			0 => 0.0,
			1 => 1.0,  // No change to wave
			2 => 0.5,  // '>> 1' to wave contents
			3 => 0.2,  // '>> 2' to wave contents
			_ => panic!("Should not happen")
		}
	}

	fn set(&mut self, value: u8) { self.register = value & 0b01100000; }
	fn get(&self) -> u8 { self.register }
}


// Length, volume, frequency will be driven by the main clock.
//

#[derive(Clone, Copy, Debug, PartialEq)]
enum SoundOutputTerminal {
	S01 = 0,
	SO2 = 1
}


#[derive(Default)]
pub struct SoundOnOffRegister {
	value: u8
}

enum Channel {
	Sound1 = 0,
	Sound2 = 1,
	Sound3 = 2,
	Sound4 = 3
}

impl SoundOnOffRegister {
	fn global_on(&self) -> bool { bitget(self.value, 7) }

	fn channel_enabled(&self, channel: Channel) -> bool {
		bitget(self.value, channel as u8)
	}

	fn disable_channel(&mut self, channel: Channel) {
		bitset(&mut self.value, false, channel as u8)
	}
}