use crate::tls::extensions::*;

pub fn find_supported_versions_sh(
    extensions: &Vec<Extension>,
) -> Option<&SupportedVersionsServerHello> {
    for e in extensions {
        if let Extension::SupportedVersionsServerHello(v) = e {
            return Some(v);
        }
    }

    None
}

pub fn find_key_share_ch(extensions: &[Extension]) -> Option<&KeyShareClientHello> {
    for e in extensions {
        if let Extension::KeyShareClientHello(v) = e {
            return Some(v);
        }
    }

    None
}

pub fn find_key_share_sh(extensions: &Vec<Extension>) -> Option<&KeyShareServerHello> {
    for e in extensions {
        if let Extension::KeyShareServerHello(v) = e {
            return Some(v);
        }
    }

    None
}

pub fn find_key_share_retry(extensions: &[Extension]) -> Option<&KeyShareHelloRetryRequest> {
    for e in extensions {
        if let Extension::KeyShareHelloRetryRequest(v) = e {
            return Some(v);
        }
    }

    None
}

pub fn find_supported_versions_ch(
    extensions: &[Extension],
) -> Option<&SupportedVersionsClientHello> {
    for e in extensions {
        if let Extension::SupportedVersionsClientHello(v) = e {
            return Some(v);
        }
    }

    None
}

pub fn find_signature_algorithms(extensions: &[Extension]) -> Option<&SignatureSchemeList> {
    for e in extensions {
        if let Extension::SignatureAlgorithms(v) = e {
            return Some(v);
        }
    }

    None
}
