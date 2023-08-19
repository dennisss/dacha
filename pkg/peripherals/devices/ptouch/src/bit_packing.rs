/*
- With bit-packing, we must always compress all 16 bytes
- Compression words as follows:
    - If we see that the next 'n' bytes have a value of 'k'
        - Emit 2 bytes: [-(n - 1), k]
    - If we see that the next series of 'm' bytes are all different values:
        - Emit m+1 bytes: [m - 1, ...]
- If the above results in more than 16 bytes, revert to encoding everything as 17 bytes
*/

#[derive(Clone, Copy)]
enum RunType {
    None,
    AllSame,
    AllDifferent,
}

/// Given an entire raster line of pixel values (16 bytes for PT-P700),
/// compresses the data using the TIFF (Pack Bits) mode.
pub fn pack_line_bits(line: &[u8]) -> Vec<u8> {
    let mut out = vec![];

    let mut i = 0;
    while i < line.len() {
        let mut last_value = line[i];

        let mut run_type = RunType::None;
        let mut run_length = 1;

        for j in (i + 1)..line.len() {
            let cur_value = line[j];

            match run_type {
                RunType::None => {
                    run_type = if cur_value == last_value {
                        RunType::AllSame
                    } else {
                        RunType::AllDifferent
                    };
                }
                RunType::AllDifferent => {
                    if cur_value == last_value {
                        run_length -= 1;
                        break;
                    }
                }
                RunType::AllSame => {
                    if cur_value != last_value {
                        break;
                    }
                }
            }

            run_length += 1;
            last_value = cur_value;
        }

        match run_type {
            RunType::AllDifferent | RunType::None => {
                out.push((run_length - 1) as u8);
                out.extend_from_slice(&line[i..(i + run_length)]);
            }
            RunType::AllSame => {
                out.push((((run_length - 1) as i8) * -1) as u8);
                out.push(line[i]);
            }
        }

        i += run_length;
    }

    // Fall back to encoding everything usign the 'all different' run type.
    if out.len() > line.len() {
        out.clear();
        out.push((line.len() - 1) as u8);
        out.extend_from_slice(line);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_line_bits_test() {
        // Example for the Brother command reference
        assert_eq!(
            pack_line_bits(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x22, 0x22, 0x23, 0xBA,
                0xBF, 0xA2, 0x22, 0x2B
            ]),
            &[0xED, 0x00, 0xFF, 0x22, 0x05, 0x23, 0xBA, 0xBF, 0xA2, 0x22, 0x2B]
        );

        assert_eq!(
            pack_line_bits(&[0x00, 0x01, 0x01, 0x01]),
            &[0, 0x00, 254, 0x01]
        );

        assert_eq!(
            pack_line_bits(&[0x00, 0x01, 0x01, 0x01, 0x01]),
            &[0, 0x00, 253, 0x01]
        );

        // Not compressable. Use fallback.
        assert_eq!(pack_line_bits(&[0x00, 0x01, 0x01]), &[2, 0x00, 0x01, 0x01]);

        // All the same
        assert_eq!(
            pack_line_bits(&[0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB]),
            &[251, 0xAB]
        );
    }
}
