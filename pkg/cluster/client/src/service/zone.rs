

pub const LOCAL_ZONE: &'static str = "local";

pub const GLOBAL_ZONE: &'static str = "global";

// This needs to be stricter than 'parse_reg_name' since it needs to be part of a hostname.
// TODO: Dedup with sanitizing job names.
// TODO: Perform validation on the zone name with this whenever reading in a zone name from the user.
regexp!(ZONE_NAME_PATTERN => "^[a-z]([a-z0-9\\-_]*[a-z0-9])?$");

pub fn is_valid_zone(zone: &str) -> bool {
    if zone == LOCAL_ZONE || zone == GLOBAL_ZONE {
        return false;
    }

    if zone.len() > 32 {
        return false;
    }

    ZONE_NAME_PATTERN.exec(zone).is_some()
}

pub fn is_valid_user_name(name: &str) -> bool {
    is_valid_zone(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_zone_test() {
        let valid: &'static [&'static str] = &[
            "hello",
            "a-b",
            "us-east-12",
            "apple",
            "a_super_long_name_this_is_way_to"
        ];

        let invalid: &'static [&'static str] = &[
            "HELLO",
            "-",
            "",
            "-apple",
            "a-",
            "local",
            "global",
            "a_super_long_name_this_is_way_to_big_to_fit_in_a_url"
        ];

        for name in valid {
            assert_eq!(is_valid_zone(name), true);
        }

        for name in invalid {
            assert_eq!(is_valid_zone(name), false);
        }
    }
}