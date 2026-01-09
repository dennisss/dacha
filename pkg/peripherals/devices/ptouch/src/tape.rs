/*
length:
    min: 4.4mm / 31 dots
    max: (for non-heatshrink) 1000mm / 7068 dots


feed amount
- min: 2mm (14 dots)
- max: 127mm (900 dots)

- print head is 128 pixels
- max prin area is


*/

use crate::{MediaType, Status};

// Applicable to PT-H500/P700/E500
const DPI: usize = 180;

const MM_PER_INCH: f32 = 25.4;

/// Metadata described the dimensions of a loaded tape for printing.
#[derive(Clone, Debug)]
pub struct Tape {
    /// Human readable name of this tape.
    pub name: String,

    /// Overall width of the tape in dot units.
    pub width: usize,

    /// Width of the printable area of the tape in dot units.
    pub print_area: usize,

    /// The minimum margin (before and after each page) added when printing.
    /// This is in dot units.
    pub margin: usize,

    /// Dots per inch.
    pub dpi: usize,
}

impl Tape {
    pub(crate) fn from_status(status: &Status) -> Option<Self> {
        match status.media_type {
            MediaType::LAMINATED_TAPE => {
                // for TZe tapes
                let (width, print_area) = match status.media_width {
                    3 => (3.5, 24),
                    6 => (6., 32),
                    9 => (9., 50),
                    12 => (12., 70),
                    18 => (18., 112),
                    24 => (24., 128),
                    _ => return None,
                };

                let name = format!(
                    "TZe Laminated {}mm ({:?} on {:?})",
                    width, status.text_color, status.tape_color
                );

                Some(Self {
                    name,
                    width: Self::mm_to_dots_impl(width, DPI as f32).round() as usize,
                    print_area,
                    margin: 14,
                    dpi: DPI,
                })
            }
            MediaType::NON_LAMINATED_TAPE => {
                let (width, print_area) = match status.media_width {
                    // For https://www.amazon.com/dp/B08748QQNX
                    // "IDIK HSe-221 Heat Shrink Tubing Label Tape for Brother 8.8mm Black on White for Brother Printer Ribbon Label Tape HS2-221"
                    9 => (8.8, 48),
                    _ => return None,
                };

                let name = format!(
                    "Non-Laminated {}mm ({:?} on {:?})",
                    width, status.text_color, status.tape_color
                );

                Some(Self {
                    name,
                    width: Self::mm_to_dots_impl(width, DPI as f32).round() as usize,
                    print_area,
                    margin: 14,
                    dpi: DPI,
                })

            }

            MediaType::HEAT_SHRINK_TUBE_3_TO_1 => {
                let (width, print_area) = match status.media_width {
                    // For https://www.amazon.com/dp/B0C138HXLR
                    // "Brother HSe251E 21mm (0.82in) Black on White 3:1 Heat Shrink Tube"
                    21 => (21., 120),
                    _ => return None 
                };

                let name = format!(
                    "HSe 3:1 {}mm ({:?} on {:?})",
                    width, status.text_color, status.tape_color
                );

                Some(Self {
                    name,
                    width: Self::mm_to_dots_impl(width, DPI as f32).round() as usize,
                    print_area,
                    margin: 14,
                    dpi: DPI,
                })
            }

            _ => {
                return None;
            }
        }
    }

    pub fn mm_to_dots(&self, mm: f32) -> f32 {
        Self::mm_to_dots_impl(mm, self.dpi as f32)
    }

    fn mm_to_dots_impl(mm: f32, dpi: f32) -> f32 {
        mm * (dpi / MM_PER_INCH)
    }
}
