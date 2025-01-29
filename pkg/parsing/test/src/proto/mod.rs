// TODO: Autogenerate this file.

use common::errors::*;

mod binary_test {
    include!(concat!(env!("OUT_DIR"), "/src/proto/binary_test.rs"));
}

#[cfg(test)]
mod tests {
    use super::binary_test::*;
    use super::*;

    #[test]
    fn compiler_bit_test() -> Result<()> {
        let input: &[u8] = &[0b01011001, 0, 0];

        let (mut v, input_rest) = BitFlags::parse(input)?;
        assert_eq!(input_rest.len(), 2);

        assert_eq!(
            v,
            BitFlags {
                a: false,
                b: true,
                c: 0b0110,
                d: false,
                e: 1
            }
        );

        let mut out = vec![];
        v.serialize(&mut out)?;

        assert_eq!(&out, &input[0..1]);

        // Serializing with a value that is larger than the bit width should fail.
        v.c = 0xf1;
        assert!(v.serialize(&mut out).is_err());

        Ok(())
    }

    #[test]
    fn compiler_bit_large_test() -> Result<()> {
        let input: &[u8] = &[0b10000000, 0xab, 0xcd, 0xef, 0b00000101, 0x01, 0x20, 0x53];

        let (v1, input_rest1) = BitHigh32::parse(input)?;
        assert_eq!(
            v1,
            BitHigh32 {
                flag: true,
                value: 0xabcdef
            }
        );
        assert_eq!(input_rest1.len(), 4);

        let (v2, input_rest2) = BitHigh32::parse(input_rest1)?;
        assert_eq!(
            v2,
            BitHigh32 {
                flag: false,
                value: 0x5012053
            }
        );
        assert_eq!(input_rest2.len(), 0);

        let mut out = vec![];
        v1.serialize(&mut out)?;
        v2.serialize(&mut out)?;
        assert_eq!(&out, input);

        Ok(())
    }

    #[test]
    fn compiler_end_terminated() -> Result<()> {
        let input: &[u8] = &[66, 246, 0, 0, 1, 2, 3, 4, 5, 6, 7, 0, 0xAB, 0xCB, 1];

        let (v, rest) = EndTerminated::parse(input)?;
        assert_eq!(
            v,
            EndTerminated {
                head: 123.0,
                body: vec![1, 2, 3, 4, 5, 6, 7],
                trailer: BitHigh32 {
                    flag: false,
                    value: 0xABCB01
                }
            }
        );
        assert_eq!(rest, &[]);

        Ok(())
    }

    #[test]
    fn compiler_constant_value() -> Result<()> {
        let (v, rest) = ConstantFirstByte::parse(&[12, 5])?;
        assert_eq!(v.first(), 12);
        assert_eq!(v.second, 5);

        let mut out = vec![];
        v.serialize(&mut out);
        assert_eq!(out, &[12, 5]);

        assert!(ConstantFirstByte::parse(&[13, 5]).is_err());

        Ok(())
    }

    // TODO: Test having more regular fields before or after the bit fields.
}
