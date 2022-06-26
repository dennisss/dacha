
use std::str::FromStr;

pub fn demangle_name(name: &str) -> String {
    if let Some(name) = demangle_name_standard(name) {
        return name;
    }    

    name.to_string()
}

fn demangle_name_standard(name: &str) -> Option<String> {
    let name = match name.strip_prefix("_ZN") {
        Some(v) => v,
        None => return None
    };

    let mut name = match name.strip_suffix("E") {
        Some(v) => v,
        None => return None
    };
    
    let mut out = String::new();

    while !name.is_empty() {
        let end_of_length = match name.char_indices().find(|(i, c)| !c.is_numeric()) {
            Some((i, _)) => i,
            None => return None
        };

        let length = match usize::from_str(&name[0..end_of_length]) {
            Ok(v) => v,
            Err(_) => return None
        };

        name = &name[end_of_length..];

        if name.len() < length {
            return None;
        }

        let part = &name[0..length];
        name = &name[length..];

        if !out.is_empty() {
            out.push_str("::");
        }
        out.push_str(part);
    }

    Some(out.replace("$LT$", "<").replace("$GT$", ">"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demangle_name_works() {
        assert_eq!(demangle_name("pthread_attr_getstack"), "pthread_attr_getstack");
        assert_eq!(demangle_name("__rust_panic_cleanup"), "__rust_panic_cleanup");
        assert_eq!(demangle_name("_ZN3std3sys4unix2fs8readlink17h61fc6698afe7e2dbE"), "std::sys::unix::fs::readlink::h61fc6698afe7e2db");
        assert_eq!(demangle_name("_ZN4core5slice4iter13Iter$LT$T$GT$3new17haa7ae5771d10d65aE"), "core::slice::iter::Iter<T>::new::haa7ae5771d10d65a");
    }

}