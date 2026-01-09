
use crate::management::*;

pub const FAN_NAMES: &'static [&'static str; 6] = &[
    "front_left",
    "back_left",
    "front_middle",
    "back_middle",
    "front_right",
    "back_right",
];

const BLADE_ORDER: &'static [(EnclosureSide, usize); 12] = &[
    (EnclosureSide::Right, 1),
    (EnclosureSide::Left, 5),
    (EnclosureSide::Left, 6),
    (EnclosureSide::Right, 4),
    (EnclosureSide::Left, 1),
    (EnclosureSide::Left, 4),
    (EnclosureSide::Right, 3),
    (EnclosureSide::Right, 5),
    (EnclosureSide::Left, 2),
    (EnclosureSide::Left, 3),
    (EnclosureSide::Right, 2),
    (EnclosureSide::Right, 6),
];

/// Ordering of the blades in the expander ports
/// (blade 6 takes up phys 0-3, blade 5 takes up phys 4-7, etc.)
const EXPANDER_ORDER: &'static [usize; 6] = &[
    6, 5, 4, 3, 2, 1
];

pub fn expander_phy_to_bay_number(side: EnclosureSide, phy_num: usize) -> Option<usize> {
    // TODO: Check for overflow.
    let blade_num = EXPANDER_ORDER[phy_num / 4];

    let mut i = 0;

    for (blade_i, (current_side, current_blade)) in BLADE_ORDER.iter().cloned().enumerate() {

        let blade_disk_count = {
            if blade_i % 4 == 0 {
                3
            } else {
                4
            }
        };

        if (current_side, current_blade) == (side, blade_num) {
            return Some(i + ((blade_disk_count - 1) - (phy_num % 4)));
        } 

        i += blade_disk_count;
    }

    None
}


/// For every LED in grid order, returns the index of the bay below it and above it.
pub fn led_grid_ordering() -> Vec<(Option<usize>, Option<usize>)> {
    fn get_bay_below(row: isize, col: isize) -> Option<usize> {
        let i = row * 15 + col;
        if i < 0 || i >= 15*3 {
            return None;
        }

        Some(i as usize)
    }
    
    let mut out = vec![];

    for row in 0..4 {
        for col in 0..15 {
            out.push((get_bay_below(row, col), get_bay_below(row - 1, col)))
        }
    }

    out
}

pub fn leds_from_grid_order(data: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    out.reserve_exact(data.len());

    let bytes_per_col = 3;
    let bytes_per_row = 15 * bytes_per_col;

    for row in 0..4 {
        let mut col_order = (0..15).collect::<Vec<_>>();
        if row % 2 == 0 {
            col_order.reverse();
        }
        
        for col in col_order {
            let i = row*bytes_per_row + col*bytes_per_col;
            let j = i + bytes_per_col;
            out.extend_from_slice(&data[i..j]);
        }
    }

    out
}


#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn bay_nums() {
        assert_eq!(expander_phy_to_bay_number(EnclosureSide::Right, 22).unwrap(), 0);
        assert_eq!(expander_phy_to_bay_number(EnclosureSide::Right, 21).unwrap(), 1);
        assert_eq!(expander_phy_to_bay_number(EnclosureSide::Right, 20).unwrap(), 2);
        assert_eq!(expander_phy_to_bay_number(EnclosureSide::Right, 19).unwrap(), 37);
        assert_eq!(expander_phy_to_bay_number(EnclosureSide::Right, 18).unwrap(), 38);
        assert_eq!(expander_phy_to_bay_number(EnclosureSide::Right, 17).unwrap(), 39);
        assert_eq!(expander_phy_to_bay_number(EnclosureSide::Right, 16).unwrap(), 40);
    }

    #[test]
    fn leds() {
        // let order = led_grid_ordering();
        // println!("{:?}", order);

        let mut data = vec![0u8; 4*15*3];
        data[0] = 1;
        data[14*3] = 2;


        let out = leds_from_grid_order(&data);
        println!("{:?}", out);



    }

}