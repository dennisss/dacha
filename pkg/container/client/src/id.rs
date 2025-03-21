// Utilities with dealing with entity ids.
//
// - Currently u64 ids are using to unique identify nodes and workers.
// - Ids are generated pseudorandomly but must start with a alphabetic character
//   since they are used as DNS components.

pub fn normalize_entity_id(raw: u64) -> u64 {
    let s = base_radix::base32_encode_cl64(raw);

    // All encoded characters can either be alphabetic or numeric but the first
    // character must be numeric to
    let first_char = s.chars().next().unwrap();
    if first_char.is_ascii_digit() {
        let new_char = (((first_char as u8) - b'0') + b'a') as char;

        let mut new_s = String::new();
        new_s.push(new_char);
        new_s.push_str(s.split_at(1).1);

        return base_radix::base32_decode_cl64(new_s).unwrap();
    }

    raw
}

pub fn entity_id_to_string(id: u64) -> Option<String> {
    let s = base_radix::base32_encode_cl64(id);
    if !s.chars().next().unwrap().is_alphabetic() {
        return None;
    }

    Some(s)
}

pub fn entity_id_from_string(id: &str) -> Option<u64> {
    base_radix::base32_decode_cl64(id)
}

pub fn is_valid_entity_id(id: u64) -> bool {
    normalize_entity_id(id) == id
}
