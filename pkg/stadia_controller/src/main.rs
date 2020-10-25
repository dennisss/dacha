extern crate libusb;
extern crate uinput;

#[macro_use]
extern crate common;

use common::errors::*;
use std::time::Duration;
use uinput::event::absolute::{Absolute, Position};
use uinput::event::controller::Misc;
use uinput::event::controller::{DPad, GamePad};

type Result<T> = std::result::Result<T, Error>;

// TODO: Dedup with common package.
macro_rules! enum_def {
    ($name:ident $t:ty => $( $case:ident = $val:expr ),*) => {
    	#[derive(Clone, Copy, PartialEq, Eq, Debug)]
		pub enum $name {
			$(
				$case = $val
			),*
		}

		impl $name {
			pub fn from_value(v: $t) -> Result<Self> {
				Ok(match v {
					$(
						$val => $name::$case,
					)*
					_ => {
						return Err(
							format_err!("Unknown value for '$name': {}", v));
					}
				})
			}

			pub fn to_value(&self) -> $t {
				match self {
					$(
						$name::$case => $val,
					)*
				}
			}
		}

    };
}

macro_rules! send_button_change {
    ($controller:ident, $state:ident, $last_state:ident, $prop:tt, $event:expr) => {
        if $state.$prop != $last_state.$prop {
            $controller.send($event, if $state.$prop { 1 } else { 0 })?;
        }
    };
}

const PROTOCOL_VERSION: u8 = 3;
const AXIS_CENTER: u8 = 0x80;

#[derive(Debug)]
struct StadiaStickState {
    /// Left-right stick position. 0x80 is centered. Right is positive.
    x: u8,
    /// Up-down stick position. 0x80 is centered. Down is positive.
    y: u8,

    pressed: bool,
}

impl Default for StadiaStickState {
    fn default() -> Self {
        Self {
            x: AXIS_CENTER,
            y: AXIS_CENTER,
            pressed: false,
        }
    }
}

enum_def!(StadiaDPadDirection u8 =>
    UP = 0,
    UP_RIGHT = 1,
    RIGHT = 2,
    DOWN_RIGHT = 3,
    DOWN = 4,
    DOWN_LEFT = 5,
    LEFT = 6,
    UP_LEFT = 7,
    CENTER = 8
);

// TODO: Refactor to use match {}
impl StadiaDPadDirection {
    pub fn up_pressed(&self) -> bool {
        *self == Self::UP || *self == Self::UP_RIGHT || *self == Self::UP_LEFT
    }
    pub fn right_pressed(&self) -> bool {
        *self == Self::RIGHT || *self == Self::UP_RIGHT || *self == Self::DOWN_RIGHT
    }
    pub fn down_pressed(&self) -> bool {
        *self == Self::DOWN || *self == Self::DOWN_RIGHT || *self == Self::DOWN_LEFT
    }
    pub fn left_pressed(&self) -> bool {
        *self == Self::LEFT || *self == Self::UP_LEFT || *self == Self::DOWN_LEFT
    }
}

impl Default for StadiaDPadDirection {
    fn default() -> Self {
        Self::CENTER
    }
}

#[derive(Debug, Default)]
struct StadiaControllerState {
    // Binary buckets. True is pressed down.
    a: bool,
    b: bool,
    y: bool,
    x: bool,
    l1: bool,
    r1: bool,

    dpad: StadiaDPadDirection,

    // The middle top-right button with three horizontal bars.
    menu: bool,
    // The middle top-left button with three dots.
    options: bool,

    assistant: bool,
    capture: bool,
    stadia: bool,

    // From 0-255 where 0 is not pressed, and 255 is fully pressed.
    l2: u8,
    r2: u8,

    l3: StadiaStickState,
    r3: StadiaStickState,
}

//impl std::fmt::Debug for StadiaControllerState {
//	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//		write!(f, "Point {{ x: {}, y: {} }}", self.x, self.y)
//	}
//}

impl StadiaControllerState {
    pub fn parse_usb_packet(buf: &[u8]) -> Result<Self> {
        if buf.len() < 1 || buf[0] != PROTOCOL_VERSION {
            return Err(err_msg("Unsupported controller USB protocol version"));
        }

        // TODO: Why do packagts have 11 bytes now? (used to only have 10)
        if buf.len() < 10 {
            return Err(format_err!("Invalid USB data length: {}", buf.len()));
        }

        let dpad = buf[1]; // 0x08 when nothing is pressed
        let mid = buf[2];
        let abxy = buf[3];

        Ok(StadiaControllerState {
            dpad: StadiaDPadDirection::from_value(dpad).unwrap(),

            menu: mid & 0x20 != 0,
            capture: mid & 0x01 != 0,
            assistant: mid & 0x02 != 0,
            stadia: mid & 0x10 != 0,
            options: mid & 0x40 != 0,

            a: abxy & 0x40 != 0,
            b: abxy & 0x20 != 0,
            x: abxy & 0x10 != 0,
            y: abxy & 0x08 != 0,
            l1: abxy & 0x04 != 0,
            r1: abxy & 0x02 != 0,

            l3: StadiaStickState {
                x: buf[4],
                y: buf[5],
                pressed: abxy & 0x01 != 0,
            },
            r3: StadiaStickState {
                x: buf[6],
                y: buf[7],
                pressed: mid & 0x80 != 0,
            },

            l2: buf[8],
            r2: buf[9],
        })
    }
}

const USB_CONFIG: u8 = 1;
const USB_IFACE: u8 = 1;

// sudo cp pkg/stadia_controller/80-stadia-controller.rules /etc/udev/rules.d/
// sudo udevadm control --reload-rules

// TODO: Support the mic input

fn read_controller() -> Result<()> {
    let mut context = libusb::Context::new()?;

    let (mut device_handle, device_desc) = {
        let mut handle = None;

        for mut device in context.devices()?.iter() {
            let desc = device.device_descriptor()?;
            if desc.vendor_id() == 0x18d1 && desc.product_id() == 0x9400 {
                handle = Some((device.open()?, desc));
                break;
            }
        }

        handle.ok_or(err_msg("No device found"))?
    };

    let languages = device_handle.read_languages(Duration::from_secs(1))?;
    if languages.len() != 1 {
        return Err(err_msg("Expected only a single language"));
    }

    let product_name =
        device_handle.read_product_string(languages[0], &device_desc, Duration::from_secs(1))?;

    println!("Product name: {}", product_name);

    device_handle.reset()?;

    if device_handle.kernel_driver_active(USB_IFACE)? {
        println!("Detaching kernel driver.");
        device_handle.detach_kernel_driver(USB_IFACE)?;
    }

    device_handle.set_active_configuration(USB_CONFIG)?;
    device_handle.claim_interface(USB_IFACE)?;
    device_handle.set_alternate_setting(USB_IFACE, 0)?;

    println!("Opened!");

    let mut last_state = StadiaControllerState::default();

    let abs_max = std::u16::MAX as i32;
    let abs_min = abs_max * -1;

    // TODO: Need a constant GUID

    let mut controller = uinput::default()?
        .name(&product_name)?
        .event(DPad::Up)?
        .event(DPad::Right)?
        .event(DPad::Down)?
        .event(DPad::Left)?
        .event(GamePad::A)?
        .event(GamePad::B)?
        .event(GamePad::X)?
        .event(GamePad::Y)?
        .event(GamePad::TL)?
        .event(GamePad::TR)?
        .event(GamePad::TL2)?
        .event(GamePad::TR2)?
        .event(GamePad::ThumbL)?
        .event(GamePad::ThumbR)?
        .event(GamePad::Start)?
        .event(GamePad::Select)?
        .event(Misc::_0)?
        .event(Misc::_1)?
        .event(Misc::_2)?
        .event(Absolute::Position(Position::X))?
        .min(abs_min)
        .max(abs_max)
        .event(Absolute::Position(Position::Y))?
        .min(abs_min)
        .max(abs_max)
        .event(Absolute::Position(Position::RX))?
        .min(abs_min)
        .max(abs_max)
        .event(Absolute::Position(Position::RY))?
        .min(abs_min)
        .max(abs_max)
        .create()?;

    let mut buf = [0u8; 512];
    loop {
        let nread = match device_handle.read_interrupt(0x83, &mut buf, Duration::new(1, 0)) {
            Err(libusb::Error::Timeout) => {
                // println!("Timed out");
                continue;
            }
            result @ _ => result?,
        };

        // TODO: Remove this as it is in parse_usb_packet?

        let state = StadiaControllerState::parse_usb_packet(&buf[0..nread])?;

        send_button_change!(controller, state, last_state, a, GamePad::A);
        send_button_change!(controller, state, last_state, b, GamePad::B);
        send_button_change!(controller, state, last_state, x, GamePad::X);
        send_button_change!(controller, state, last_state, y, GamePad::Y);
        send_button_change!(controller, state, last_state, l1, GamePad::TL);
        send_button_change!(controller, state, last_state, r1, GamePad::TR);
        send_button_change!(controller, state, last_state, menu, GamePad::Start);
        send_button_change!(controller, state, last_state, options, GamePad::Select);
        // This can't be GamePad::Mode because that will mess up the web gamepad
        // mapping.
        send_button_change!(controller, state, last_state, stadia, Misc::_0);
        send_button_change!(controller, state, last_state, assistant, Misc::_1);
        send_button_change!(controller, state, last_state, capture, Misc::_2);

        if state.dpad.up_pressed() != last_state.dpad.up_pressed() {
            controller.send(DPad::Up, if state.dpad.up_pressed() { 1 } else { 0 })?;
        }
        if state.dpad.right_pressed() != last_state.dpad.right_pressed() {
            controller.send(DPad::Right, if state.dpad.right_pressed() { 1 } else { 0 })?;
        }
        if state.dpad.down_pressed() != last_state.dpad.down_pressed() {
            controller.send(DPad::Down, if state.dpad.down_pressed() { 1 } else { 0 })?;
        }
        if state.dpad.left_pressed() != last_state.dpad.left_pressed() {
            controller.send(DPad::Left, if state.dpad.left_pressed() { 1 } else { 0 })?;
        }

        if state.l3.pressed != last_state.l3.pressed {
            controller.send(GamePad::ThumbL, if state.l3.pressed { 1 } else { 0 })?;
        }
        if state.r3.pressed != last_state.r3.pressed {
            controller.send(GamePad::ThumbR, if state.r3.pressed { 1 } else { 0 })?;
        }

        let convert_axis = |v: u8| {
            ((v as i32) - (AXIS_CENTER as i32)) * ((std::u16::MAX as i32) / (AXIS_CENTER as i32))
        };

        let axis_btn = |v: u8| v > 0;

        if axis_btn(state.l2) != axis_btn(last_state.l2) {
            controller.send(GamePad::TL2, if axis_btn(state.l2) { 1 } else { 0 })?;
        }
        if axis_btn(state.r2) != axis_btn(last_state.r2) {
            controller.send(GamePad::TR2, if axis_btn(state.r2) { 1 } else { 0 })?;
        }

        controller.send(Position::X, convert_axis(state.l3.x))?;
        controller.send(Position::Y, convert_axis(state.l3.y))?;
        controller.send(Position::RX, convert_axis(state.r3.x))?;
        controller.send(Position::RY, convert_axis(state.r3.y))?;

        controller.synchronize()?;
        //		println!("{:?}", state);

        last_state = state;
    }

    Ok(())
}

fn main() -> Result<()> {
    println!("Hello!");

    read_controller()
}
