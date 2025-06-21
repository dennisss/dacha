use std::vec::Vec;

use common::errors::*;

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

pub fn find_server_name_from_client(extensions: &[Extension]) -> Result<Option<&str>> {
    for e in extensions {
        if let Extension::ServerName(v) = e {
            let server_name = match v {
                Some(v) => v,
                None => {
                    return Err(err_msg(
                        "Empty server_name only allowed to be sent from servers",
                    ));
                }
            };

            if server_name.names.len() != 1 {
                return Err(err_msg("Expected request to have exactly one name"));
            }

            if server_name.names[0].typ != NameType::host_name {
                return Err(format_err!(
                    "Only host_name type server names are supported. Instead got: {:?}",
                    server_name.names[0].typ));
            }

            let name = std::str::from_utf8(&server_name.names[0].data)?;

            return Ok(Some(name));
        }
    }

    Ok(None)
}

pub fn find_alpn_extension(extensions: &[Extension]) -> Option<&ProtocolNameList> {
    for e in extensions {
        if let Extension::ALPN(v) = e {
            return Some(v);
        }
    }

    None
}
