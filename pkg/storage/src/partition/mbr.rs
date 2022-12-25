use common::errors::*;

mod proto {
    #![allow(dead_code, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/src/partition/mbr.rs"));
}

pub use proto::*;

pub fn parse_mbr(data: &[u8]) -> Result<MBR> {
    let (mbr, rest) = MBR::parse(data)?;
    if rest.len() != 0 {
        return Err(err_msg("Too many bytes provided"));
    }

    if &mbr.boot_signature != &[0x55, 0xAA] {
        return Err(err_msg("Incorrect boot signature in MBR"));
    }

    Ok(mbr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mbr_size() {
        assert_eq!(MBR::size_of(), 512);
    }
}
