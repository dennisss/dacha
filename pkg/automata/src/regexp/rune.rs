use common::errors::*;
use common::InRange;

/// First to last code point in for each byte length of a UTF-8 rune.
/// UTF-8 characters are encoded in up to 4 bytes with up to 21-bits being
/// use-able to represent the code itself.
const CODE_POINT_RANGES: &'static [(u32, u32)] = &[
    // Range of character codes of encoded length 1.
    (0, 0x7F),
    // ... of length 2
    (0x80, 0x7FF),
    // ... of length 3
    (0x800, 0xFFFF),
    // ... of length 4
    (0x10000, 0x10FFFF),
];

/*
Match all byte patterns from:
01 02 03 04
to
05 06 07 08


01 02 03 04 ->
01 FF FF FF

01 FF 00 00
01 FF FF FF

01 FF 03 04 ->
01 FF FF FF

01 FF FF 04 ->
01 FF FF FF


01 FF


01 02




Need '01 02 03 [04 - FF]'
Need '01 02 [04 - FF] [0 - FF]'
Need '01 [03 - FF] [00 - FF] [00 - FF]'
Need '[02 - 04] [0 - FF] [0 - FF] [0 - FF]'

Now need to go from
05 00 00 00
to
05 06 07 08

which is:
05 [0-5] [0 - FF] [0 - FF]
05 06 [0 - 6] [0 - FF]



01 02 03 04
01 FF FF FF





then we can get it higher, and


*/

/// Expands a range of unicode characters to a regular expression which matches
/// only on single bytes at a time.
fn expand_rune_range(start: char, end: char) -> Result<Vec<Vec<(u8, u8)>>> {
    assert!(end >= start);

    // Buffers for storing encoded UTF-8 characters.
    let mut buf = [0u8; 4];
    let mut buf2 = [0u8; 4];

    let mut out = vec![];
    for (lower_code, upper_code) in CODE_POINT_RANGES.iter().cloned() {
        // Skip ranges that don't overlap at all with the requested character range.
        if ((start as u32) < lower_code && (end as u32) < upper_code)
            || ((start as u32) > upper_code && (end as u32) > upper_code)
        {
            continue;
        }

        // For this byte length, these are the start/end codes across which we will
        // match.
        let lower = std::cmp::max(lower_code, start as u32);
        let upper = std::cmp::min(upper_code, end as u32);

        let lower_str = std::char::from_u32(lower).unwrap().encode_utf8(&mut buf);
        let upper_str = std::char::from_u32(upper).unwrap().encode_utf8(&mut buf2);
        assert_eq!(lower_str.len(), upper_str.len());

        let mut chain = vec![];
        for i in 0..lower_str.len() {
            chain.push((lower_str.as_bytes()[i], upper_str.as_bytes()[i]));
        }

        out.push(chain);
    }

    Ok(out)
}

/*
fn ranges_between(start: &[u8], end: &[u8], out: &mut Vec<Vec<(u8, u8)>>) {

    let mut cur_start = start.to_vec();

    for i in 0..(cur_start.len() - 1) {
        if cur_start[i] != end[i] {
            // Break into three subproblems. Beginning, middle and end.

            // Beginning
            {
                let mut new_end = cur_start.clone();
                new_end[i] = cur_start[i];
                for j in (i + 1)..new_end.len() {
                    new_end[j] = 0xff;
                }
                ranges_between(&cur_start, &new_end, out);
            }

            // Middle
            if end[i] - start[i] > 1 {
                let mut chain = vec![];
                //  Generate all of them.
            }

            // End
            {
                cur_start[i] = end[i];
                for j in (i + 1)..cur_start.len() {
                    cur_start[i] = 0;
                }
            }
        }
    }

    // Handle the final index (which should be trivial now).

    // TODO: If they are all equal, then we should push a trivial pattern?


}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rune_test() {
        assert_eq!(
            expand_rune_range('A', 'C').unwrap(),
            vec![vec![(0x41, 0x43)]]
        );

        assert_eq!(
            expand_rune_range('\u{00B5}', '\u{00B5}').unwrap(),
            vec![vec![(0o0302, 0o0302), (0o0265, 0o0265)]]
        );

        assert_eq!(
            expand_rune_range('\u{00B5}', '\u{00D7}').unwrap(),
            vec![
                vec![(0o0302, 0o0302), (0o0265, 0xFF)],
                vec![(0o0303, 0o0303), (0, 0o0227)]
            ]
        );

        // 0303 0227
    }
}
