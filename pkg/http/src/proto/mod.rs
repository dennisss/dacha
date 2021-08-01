pub mod v2 {
    #![allow(dead_code, non_snake_case, non_camel_case_types, unused_parens)]

    include!(concat!(env!("OUT_DIR"), "/src/proto/v2.rs"));
}

#[cfg(test)]
mod tests {
    use super::v2;

    #[test]
    fn settings_parse_test() {
        let input: &[u8] = &[1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 16];

        let setting1 = v2::SettingsParameter::parse(input).unwrap();
        println!("{:?}", setting1);

        let settings = v2::SettingsFramePayload::parse_complete(input).unwrap();
        println!("{:?}", settings);
    }
}
