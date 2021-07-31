use common::base64;
use common::errors::*;

use super::types::*;

pub fn parse_urlbase64(s: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
    base64::decode_config(s, base64::URL_SAFE)
}

pub fn serialize_urlbase64(s: &[u8]) -> String {
    base64::encode_config(s, base64::URL_SAFE)
}

pub fn split_path_segments(path: &str) -> Option<Vec<String>> {
    let mut segs: Vec<String> = path
        .split('/')
        .into_iter()
        .map(|s| String::from(s))
        .collect();

    if segs.len() < 2 || &segs[0] != "" {
        return None;
    }

    // Special case of the index route that takes up no segments
    if &segs[1] == "" {
        return Some(vec![]);
    }

    // Remove the trivial '' before the first slash and return it
    segs.remove(0);

    Some(segs)
}

#[derive(PartialEq)]
pub enum Host {
    Store(MachineId),
    Cache(MachineId),
}

impl Host {
    pub fn to_string(&self) -> String {
        match self {
            Host::Store(m) => format!("{}.store.hay", m),
            Host::Cache(m) => format!("{}.cache.hay", m),
        }
    }

    pub fn check_against(&self, request_head: &http::RequestHead) -> bool {
        // TODO: Consider returning a finer grained error to the client if multiple
        // headers are presetn.
        let v = match request_head.headers.get_one("Host") {
            Ok(Some(v)) => v,
            _ => return false,
        };

        match Host::from_header(v.value.as_bytes()) {
            Ok(h) => h == *self,
            Err(_) => false,
        }
    }

    pub fn from_header(v: &[u8]) -> Result<Host> {
        // TODO: Instead assert that they are ASCII (we should make it difficult to
        // write )
        let s: &str = match std::str::from_utf8(v) {
            Ok(s) => s,
            Err(_) => {
                return Err(err_msg("Invalid header value string"));
            }
        };

        let s = s.to_lowercase();

        let segs = s.split('.').collect::<Vec<_>>();
        if segs.len() < 3 {
            return Err(err_msg("Not enough segments in host"));
        }

        if segs[2] != "hay" {
            return Err(err_msg("Missing hay domain"));
        }

        let id = match segs[0].parse::<MachineId>() {
            Ok(v) => v,
            Err(_) => return Err(err_msg("Invalid machine id")),
        };

        match segs[1] {
            "store" => Ok(Host::Store(id)),
            "cache" => Ok(Host::Cache(id)),
            _ => Err(err_msg("Unknown domain type")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_to_string() {
        assert_eq!(&Host::Store(12).to_string(), "12.store.hay");
    }

    #[test]
    fn host_from_header() {
        let v = hyper::header::HeaderValue::from_str("5454.SToRE.hay.localhost").unwrap();
        match Host::from_header(&v) {
            Ok(s) => {
                match s {
                    Host::Store(5454) => {}
                    _ => panic!("Wrong parsing result"),
                };
            }
            _ => panic!("Should have been parseable"),
        };
    }
}
