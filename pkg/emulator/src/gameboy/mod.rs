use std::cell::RefCell;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::rc::Rc;

use common::errors::*;
use graphics::raster::canvas::RasterCanvas;
use graphics::raster::canvas_render_loop::WindowOptions;

use crate::gameboy::clock::CYCLES_PER_SECOND;
use crate::gameboy::joypad::Joypad;
use crate::gameboy::memory::MemoryInterface;

pub mod clock;
pub mod cpu;
pub mod joypad;
pub mod memory;
pub mod sound;
pub mod timer;
pub mod video;

const SLOW_DOWN: u64 = 1;

// ext/gameboy/pokemon_blue.gbc
// ^ MBC3+RAM+BATTERY

pub fn diff_logs() -> Result<()> {
    let f1 = File::open("golden-2")?;
    let mut buf1 = BufReader::new(f1);
    let f2 = File::open("test-2")?;
    let mut buf2 = BufReader::new(f2);

    let mut n = 0;
    loop {
        if n % 1000 == 0 {
            println!("DIFF {}", n);
        }
        n += 1;

        let mut l1 = String::new();
        let mut l2 = String::new();
        //		while l1.chars().next() != Some('A') {
        buf1.read_line(&mut l1)?;
        //		}
        //		while l2.chars().next() != Some('A') {
        buf2.read_line(&mut l2)?;
        //		}

        if l1[0..51].to_ascii_lowercase() != l2[0..51].to_ascii_lowercase() {
            println!("{}", n);
            println!("{}", l1);
            println!("{}", l2);
            panic!("DIFF");
        }
    }

    Ok(())
}

// '01-special.gb'             - passed
// '02-interrupts.gb'          - passed
// '03-op sp,hl.gb'            - passed
// '04-op r,imm.gb'            - passed
// '05-op rp.gb'               - passed
// '06-ld r,r.gb'              - passed
// '07-jr,jp,call,ret,rst.gb'  - passed
// '08-misc instrs.gb'         - passed
// '09-op r,r.gb'              - passed
// '10-bit ops.gb'             - passed
// '11-op a,(hl).gb'           - passed

const CARTRIDGE_FILE: &'static str = "ext/gameboy/pokemon_blue.gbc";

pub async fn run() -> Result<()> {
    //	diff_logs()?;

    let mut boot_rom = vec![];
    let mut boot_rom_file = File::open(project_path!("ext/gameboy/dmg_rom.bin"))?;
    boot_rom_file.read_to_end(&mut boot_rom)?;

    let mut rom = vec![];
    let mut rom_file = File::open(project_path!(CARTRIDGE_FILE))?;
    rom_file.read_to_end(&mut rom)?;

    let clock = Rc::new(RefCell::new(clock::Clock::new()));
    let interrupts = Rc::new(RefCell::new(memory::InterruptState::default()));

    let video = Rc::new(RefCell::new(video::VideoController::new(
        clock.clone(),
        interrupts.clone(),
    )));

    let sound = sound::SoundController::new()?;

    let cartridge = memory::MBC3::new(rom)?;

    let mut joypad = Rc::new(RefCell::new(Joypad::default()));

    let timer = Rc::new(RefCell::new(timer::Timer::new(
        clock.clone(),
        interrupts.clone(),
    )));

    let mut mem = memory::Memory::new(
        &boot_rom,
        cartridge,
        video.clone(),
        sound.state.clone(),
        joypad.clone(),
        interrupts.clone(),
        timer.clone(),
    );

    let mut cpu = cpu::CPU::default();

    /*
        mem.store8(0xff50, 1).unwrap(); // Disable boot rom.
        cpu.registers.PC = 0x100;

        cpu.registers.AF = 0x01B0;
        cpu.registers.BC = 0x0013;
        cpu.registers.DE = 0x00D8;
        cpu.registers.HL = 0x014D;
        cpu.registers.SP = 0xFFFE;
        mem.store8(0xFF05, 0x00).unwrap();
        mem.store8(0xFF06, 0x00).unwrap();
        mem.store8(0xFF07, 0x00).unwrap();
        mem.store8(0xFF10, 0x80).unwrap();
        mem.store8(0xFF11, 0xBF).unwrap();
        mem.store8(0xFF12, 0xF3).unwrap();
        mem.store8(0xFF14, 0xBF).unwrap();
        mem.store8(0xFF16, 0x3F).unwrap();
        mem.store8(0xFF17, 0x00).unwrap();
        mem.store8(0xFF19, 0xBF).unwrap();
        mem.store8(0xFF1A, 0x7F).unwrap();
        mem.store8(0xFF1B, 0xFF).unwrap();
        mem.store8(0xFF1C, 0x9F).unwrap();
        mem.store8(0xFF1E, 0xBF).unwrap();
        mem.store8(0xFF20, 0xFF).unwrap();
        mem.store8(0xFF21, 0x00).unwrap();
        mem.store8(0xFF22, 0x00).unwrap();
        mem.store8(0xFF23, 0xBF).unwrap();
        mem.store8(0xFF24, 0x77).unwrap();
        mem.store8(0xFF25, 0xF3).unwrap();
        mem.store8(0xFF26, 0xF1).unwrap();
        mem.store8(0xFF40, 0x91).unwrap();
        mem.store8(0xFF42, 0x00).unwrap();
        mem.store8(0xFF43, 0x00).unwrap();
    //	mem.store8(0xFF45, 0x00).unwrap();
        mem.store8(0xFF47, 0xFC).unwrap();
        mem.store8(0xFF48, 0xFF).unwrap();
        mem.store8(0xFF49, 0xFF).unwrap();
        mem.store8(0xFF4A, 0x00).unwrap();
        mem.store8(0xFF4B, 0x00).unwrap();
        mem.store8(0xFFFF, 0x00).unwrap();
        */

    let mut canvas = RasterCanvas::create(video::SCREEN_HEIGHT, video::SCREEN_WIDTH);

    let window_options = WindowOptions {
        name: "Gameboy".to_string(),
        width: video::SCREEN_WIDTH * 4,
        height: video::SCREEN_HEIGHT * 4,
        samples: 1,
    };

    let mut paused = false;
    let mut unpaused = false;

    const BREAK_POINT: u16 = 0xffff; // 0x0100;

    canvas
        .render_loop(window_options, |canvas, window, _| {
            {
                let mut jp = joypad.borrow_mut();
                jp.select_pressed =
                    window.raw().get_key(graphics::glfw::Key::Q) == graphics::glfw::Action::Press;
                jp.start_pressed =
                    window.raw().get_key(graphics::glfw::Key::W) == graphics::glfw::Action::Press;
                jp.b_pressed =
                    window.raw().get_key(graphics::glfw::Key::E) == graphics::glfw::Action::Press;
                jp.a_pressed =
                    window.raw().get_key(graphics::glfw::Key::R) == graphics::glfw::Action::Press;

                jp.up_pressed =
                    window.raw().get_key(graphics::glfw::Key::Up) == graphics::glfw::Action::Press;
                jp.down_pressed = window.raw().get_key(graphics::glfw::Key::Down)
                    == graphics::glfw::Action::Press;
                jp.left_pressed = window.raw().get_key(graphics::glfw::Key::Left)
                    == graphics::glfw::Action::Press;
                jp.right_pressed = window.raw().get_key(graphics::glfw::Key::Right)
                    == graphics::glfw::Action::Press;

                // TODO: Trigger joypad interrupt if needed.
            }

            let mut speed_up = 1;

            if paused {
                // TODO: This should only be triggered if we just pressed 'C' once (check
                // events).
                if window.raw().get_key(graphics::glfw::Key::C) == graphics::glfw::Action::Press {
                    clock.borrow_mut().reset_start();
                    paused = false;
                    unpaused = true;

                    speed_up = 1;
                }
            } else {
                let target_cycles = clock.borrow().target() / SLOW_DOWN * speed_up;

                loop {
                    // TODO: Should wait until the previous instruction is complete?
                    if cpu.registers.PC == BREAK_POINT && cpu.remaining_cycles == 0 && !paused {
                        if unpaused {
                            unpaused = false;
                        } else {
                            println!("BREAKING");
                            println!("{:?}", cpu.registers);
                            paused = true;
                            break;
                        }
                    }

                    let mut cycles = {
                        let c = clock.borrow_mut();
                        if c.cycles >= target_cycles {
                            break;
                        } else {
                            c.cycles
                        }
                    };

                    //				if cycles >= 23454500 {
                    //					cycles -= 23454500;
                    //				}

                    if cycles % CYCLES_PER_SECOND == 0 {
                        //					println!("TICK {}", cycles);
                    }

                    // Only call at 1MHz.
                    if cycles % 4 == 0 {
                        video.borrow_mut().step()?;
                    }

                    // TODO: Move this timing logic into the sound controller?
                    if cycles % 8192 == 0 {
                        sound.step(&clock.borrow())?;
                    }

                    // TODO: Sort by dependencies.
                    // Always run the highest up components first.

                    cpu.step(&mut mem, &interrupts, cycles)?;

                    timer.borrow_mut().step()?;

                    mem.step()?;

                    // TODO: Eventually we will need to cycle the memory too to support
                    // DMAs

                    clock.borrow_mut().cycles += 1;
                }
            }

            {
                let screen_buffer = &video.borrow().screen_buffer;

                for y in 0..video::SCREEN_HEIGHT {
                    for x in 0..video::SCREEN_WIDTH {
                        let c = screen_buffer[y * video::SCREEN_WIDTH + x].to_rgb() as u8;
                        canvas.drawing_buffer[(y, x, 0)] = c;
                        canvas.drawing_buffer[(y, x, 1)] = c;
                        canvas.drawing_buffer[(y, x, 2)] = c;
                    }
                }
            }

            Ok(())
        })
        .await?;

    // while window.is_open() {

    // }

    Ok(())
}
