use common::bytes::{Buf, Bytes};
use common::errors::*;
use parsing::binary::*;
use parsing::*;

use super::handshake::{HandshakeType, ProtocolVersion};
use super::parsing::*;

// List of all extensions: https://www.iana.org/assignments/tls-extensiontype-values/tls-extensiontype-values.xml

// TODO: Should also implement https://tools.ietf.org/html/rfc7627

// TODO: Implement ec_point_formats with just 'uncompressed' being send in
// client hello

/*
struct {
    ExtensionType extension_type;
    opaque extension_data<0..2^16-1>;
} Extension;
*/

#[derive(Debug)]
pub enum Extension {
    ServerName(ServerNameList),
    MaxFragmentLength(MaxFragmentLength),
    SupportedGroups(NamedGroupList),
    SignatureAlgorithms(SignatureSchemeList),

    SupportedVersionsClientHello(SupportedVersionsClientHello),
    SupportedVersionsServerHello(SupportedVersionsServerHello),
    Cookie(Cookie),
    PostHandshakeAuth,
    SignatureAlgorithmsCert(SignatureSchemeList),

    KeyShareClientHello(KeyShareClientHello),
    KeyShareHelloRetryRequest(KeyShareHelloRetryRequest),
    KeyShareServerHello(KeyShareServerHello),

    Unknown { typ: u16, data: Bytes },
}

// TODO: 'There MUST NOT be more than one extension of the same type.'

// TODO: Validate which extensions are allowed to go in which message types.

impl Extension {
    pub fn parse(input: Bytes, msg_type: HandshakeType) -> ParseResult<Self> {
        let parser = seq!(c => {
            let extension_type = c.next(as_bytes(ExtensionType::parse))?;
            if !extension_type.allowed(msg_type) {
                return Err(err_msg("Extension not allowed in this message"));
            }

            let data = c.next(varlen_vector(0, U16_LIMIT))?;

            use ExtensionType::*;
            let res = match extension_type {
                server_name => {
                    map(complete(ServerNameList::parse),
                        |v| Extension::ServerName(v))(data)
                },
                max_fragment_length => {
                    map(complete(MaxFragmentLength::parse),
                        |v| Extension::MaxFragmentLength(v))(data)
                },
                supported_groups => {
                    map(complete(NamedGroupList::parse),
                        |v| Extension::SupportedGroups(v))(data)
                },
                signature_algorithms => {
                    map(complete(SignatureSchemeList::parse),
                        |v| Extension::SignatureAlgorithms(v))(data)
                },
                supported_versions => {
                    complete(|d| parse_supported_versions(d, msg_type))(data)
                },
                cookie => map(Cookie::parse, |v| Extension::Cookie(v))(data),
                post_handshake_auth => {
                    if data.len() != 0 {
                        Err(err_msg("Expected empty data"))
                    } else {
                        Ok((Extension::PostHandshakeAuth, Bytes::new()))
                    }
                },
                signature_algorithms_cert => {
                    map(complete(SignatureSchemeList::parse),
                        |v| Extension::SignatureAlgorithmsCert(v))(data)
                },
                key_share => {
                    complete(|d| parse_key_share(d, msg_type))(data)
                },
                _ => {
                    Ok((Extension::Unknown {
                        typ: extension_type.to_u16(),
                        data
                    }, Bytes::new()))
                }
            };

            let (e, _) = res?;
            Ok(e)
        });
        parser(input)
    }

    pub fn serialize(&self, msg_type: HandshakeType, out: &mut Vec<u8>) -> Result<()> {
        use Extension::*;

        let typ = match self {
            ServerName(e) => ExtensionType::server_name,
            MaxFragmentLength(e) => ExtensionType::max_fragment_length,
            SupportedGroups(e) => ExtensionType::supported_groups,
            SignatureAlgorithms(e) => ExtensionType::signature_algorithms,
            SupportedVersionsClientHello(e) => ExtensionType::supported_versions,
            SupportedVersionsServerHello(e) => ExtensionType::supported_versions,
            Cookie(e) => ExtensionType::cookie,
            PostHandshakeAuth => ExtensionType::post_handshake_auth,
            SignatureAlgorithmsCert(e) => ExtensionType::signature_algorithms_cert,
            KeyShareClientHello(e) => ExtensionType::key_share,
            KeyShareHelloRetryRequest(e) => ExtensionType::key_share,
            KeyShareServerHello(e) => ExtensionType::key_share,
            Unknown { typ, data } => ExtensionType::from_u16(*typ),
        };

        if !typ.allowed(msg_type) {
            return Err(err_msg("Extension not allowed in this message"));
        }

        typ.serialize(out);

        serialize_varlen_vector(0, U16_LIMIT, out, |out| match self {
            ServerName(e) => e.serialize(out),
            MaxFragmentLength(e) => e.serialize(out),
            SupportedGroups(e) => e.serialize(out),
            SignatureAlgorithms(e) => e.serialize(out),
            SupportedVersionsClientHello(e) => e.serialize(out),
            SupportedVersionsServerHello(e) => e.serialize(out),
            Cookie(e) => e.serialize(out),
            PostHandshakeAuth => {}
            SignatureAlgorithmsCert(e) => e.serialize(out),
            KeyShareClientHello(e) => e.serialize(out),
            KeyShareHelloRetryRequest(e) => e.serialize(out),
            KeyShareServerHello(e) => e.serialize(out),
            Unknown { typ, data } => out.extend_from_slice(&data),
        });

        Ok(())
    }
}

#[derive(Debug)]
pub enum ExtensionType {
    server_name,
    max_fragment_length,
    status_request,
    supported_groups,
    signature_algorithms,
    use_srtp,
    heartbeat,
    application_layer_protocol_negotiation,
    signed_certificate_timestamp,
    client_certificate_type,
    server_certificate_type,
    padding,
    pre_shared_key,
    early_data,
    supported_versions,
    cookie,
    psk_key_exchange_modes,
    certificate_authorities,
    oid_filters,
    post_handshake_auth, // < Empty struct
    signature_algorithms_cert,
    key_share,
    unknown(u16),
}

impl ExtensionType {
    fn to_u16(&self) -> u16 {
        use ExtensionType::*;
        match self {
            server_name => 0,
            max_fragment_length => 1,
            status_request => 5,
            supported_groups => 10,
            signature_algorithms => 13,
            use_srtp => 14,
            heartbeat => 15,
            application_layer_protocol_negotiation => 16,
            signed_certificate_timestamp => 18,
            client_certificate_type => 19,
            server_certificate_type => 20,
            padding => 21,
            pre_shared_key => 41,
            early_data => 42,
            supported_versions => 43,
            cookie => 44,
            psk_key_exchange_modes => 45,
            certificate_authorities => 47,
            oid_filters => 48,
            post_handshake_auth => 49,
            signature_algorithms_cert => 50,
            key_share => 51,
            unknown(v) => *v,
        }
    }
    // TODO: This should be allowed to return None so that we can store unknown
    // extensions opaquely?
    fn from_u16(v: u16) -> Self {
        match v {
            0 => Self::server_name,
            1 => Self::max_fragment_length,
            5 => Self::status_request,
            10 => Self::supported_groups,
            13 => Self::signature_algorithms,
            14 => Self::use_srtp,
            15 => Self::heartbeat,
            16 => Self::application_layer_protocol_negotiation,
            18 => Self::signed_certificate_timestamp,
            19 => Self::client_certificate_type,
            20 => Self::server_certificate_type,
            21 => Self::padding,
            41 => Self::pre_shared_key,
            42 => Self::early_data,
            43 => Self::supported_versions,
            44 => Self::cookie,
            45 => Self::psk_key_exchange_modes,
            47 => Self::certificate_authorities,
            48 => Self::oid_filters,
            49 => Self::post_handshake_auth,
            50 => Self::signature_algorithms_cert,
            51 => Self::key_share,
            _ => Self::unknown(v),
        }
    }

    /// See the table on https://tools.ietf.org/html/rfc8446#section-4.2.
    /// TODO: Send 'illegal_parameter' if this happens.
    fn allowed(&self, msg_type: HandshakeType) -> bool {
        use ExtensionType::*;
        use HandshakeType::*;
        match self {
            server_name => (msg_type == client_hello || msg_type == encrypted_extensions),
            max_fragment_length => (msg_type == client_hello || msg_type == encrypted_extensions),
            status_request => {
                (msg_type == client_hello
                    || msg_type == certificate_request
                    || msg_type == certificate)
            }
            supported_groups => (msg_type == client_hello || msg_type == encrypted_extensions),
            signature_algorithms => (msg_type == client_hello || msg_type == certificate_request),
            use_srtp => (msg_type == client_hello || msg_type == encrypted_extensions),
            heartbeat => (msg_type == client_hello || msg_type == encrypted_extensions),
            application_layer_protocol_negotiation => {
                (msg_type == client_hello || msg_type == encrypted_extensions)
            }
            signed_certificate_timestamp => {
                (msg_type == client_hello
                    || msg_type == certificate_request
                    || msg_type == certificate)
            }
            client_certificate_type => {
                (msg_type == client_hello || msg_type == encrypted_extensions)
            }
            server_certificate_type => {
                (msg_type == client_hello || msg_type == encrypted_extensions)
            }
            padding => (msg_type == client_hello),
            key_share => {
                (msg_type == client_hello
                    || msg_type == server_hello
                    || msg_type == hello_retry_request)
            }
            pre_shared_key => (msg_type == client_hello || msg_type == server_hello),
            psk_key_exchange_modes => (msg_type == client_hello),
            early_data => {
                (msg_type == client_hello
                    || msg_type == encrypted_extensions
                    || msg_type == new_session_ticket)
            }
            cookie => (msg_type == client_hello || msg_type == hello_retry_request),
            supported_versions => {
                (msg_type == client_hello
                    || msg_type == server_hello
                    || msg_type == hello_retry_request)
            }
            certificate_authorities => (msg_type == client_hello || msg_type == certificate),
            oid_filters => (msg_type == certificate),
            post_handshake_auth => (msg_type == client_hello),
            signature_algorithms_cert => (msg_type == client_hello || msg_type == certificate),
            _ => true,
        }
    }

    parser!(parse<&[u8], Self> => {
        map(be_u16, |v| Self::from_u16(v))
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_u16().to_be_bytes());
    }
}

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    NameType name_type;
    select (name_type) {
        case host_name: HostName;
    } name;
} ServerName;

opaque HostName<1..2^16-1>;

struct {
    ServerName server_name_list<1..2^16-1>
} ServerNameList;
*/

#[derive(Debug)]
pub struct ServerNameList {
    pub names: Vec<ServerName>,
}

impl ServerNameList {
    parser!(parse<ServerNameList> => {
        seq!(c => {
            let data = c.next(complete(varlen_vector(1, U16_LIMIT)))?;
            let (names, _) = complete(many1(ServerName::parse))(data)?;
            Ok(ServerNameList {
                names
            })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(1, U16_LIMIT, out, |out| {
            for n in self.names.iter() {
                n.serialize(out);
            }
        });
    }
}

#[derive(Debug)]
pub struct ServerName {
    pub typ: NameType,
    pub data: Bytes,
}

impl ServerName {
    parser!(parse<Self> => {
        seq!(c => {
            let typ = NameType::from_u8(c.next(as_bytes(be_u8))?);
            // NOTE: For backwards compatibility all future types must be represented as a u16 number of bytes.
            let data = c.next(varlen_vector(1, U16_LIMIT))?;
            Ok(ServerName { typ, data })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        out.push(self.typ.to_u8());
        serialize_varlen_vector(1, U16_LIMIT, out, |out| {
            out.extend_from_slice(&self.data);
        });
    }
}

tls_enum_u8!(NameType => {
    host_name(0), (255)
});

////////////////////////////////////////////////////////////////////////////////

tls_enum_u8!(MaxFragmentLength => {
    pow2_9(1),
    pow2_10(2),
    pow2_11(3),
    pow2_12(4),
    // Upon seeing this, a client/server must abort
    // with 'illegal_parameter'
    (255)
});

////////////////////////////////////////////////////////////////////////////////

// for 'supported_groups'

// TODO: TLS 1.0 list: https://tools.ietf.org/html/rfc4492#section-5.1.1

/*
enum {
    // Elliptic Curve Groups (ECDHE)
    secp256r1(0x0017), secp384r1(0x0018), secp521r1(0x0019),
    x25519(0x001D), x448(0x001E),

    // Finite Field Groups (DHE)
    ffdhe2048(0x0100), ffdhe3072(0x0101), ffdhe4096(0x0102),
    ffdhe6144(0x0103), ffdhe8192(0x0104),

    // Reserved Code Points
    ffdhe_private_use(0x01FC..0x01FF),
    ecdhe_private_use(0xFE00..0xFEFF),
    (0xFFFF)
} NamedGroup;

struct {
    NamedGroup named_group_list<2..2^16-1>;
} NamedGroupList;
*/

#[derive(Debug)]
pub struct NamedGroupList {
    pub groups: Vec<NamedGroup>,
}

impl NamedGroupList {
    parser!(parse<Self> => {
        seq!(c => {
            let data = c.next(varlen_vector(2, U16_LIMIT))?;
            let groups = c.next(complete(
                many1(NamedGroup::parse)))?;
            Ok(NamedGroupList { groups })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(2, U16_LIMIT, out, |out| {
            for v in self.groups.iter() {
                v.serialize(out);
            }
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum NamedGroup {
    // Elliptic Curve Groups (ECDHE)
    secp256r1,
    secp384r1,
    secp521r1,
    x25519,
    x448,

    // Finite Field Groups (DHE)
    ffdhe2048,
    ffdhe3072,
    ffdhe4096,
    ffdhe6144,
    ffdhe8192,

    // Reserved Code Points
    ffdhe_private_use(u16),
    ecdhe_private_use(u16),

    unknown(u16),
}
impl NamedGroup {
    pub fn to_u16(&self) -> u16 {
        use NamedGroup::*;
        match self {
            secp256r1 => 0x0017,
            secp384r1 => 0x0018,
            secp521r1 => 0x0019,
            x25519 => 0x001D,
            x448 => 0x001E,
            ffdhe2048 => 0x0100,
            ffdhe3072 => 0x0101,
            ffdhe4096 => 0x0102,
            ffdhe6144 => 0x0103,
            ffdhe8192 => 0x0104,
            // TODO: Assert that the values and all other unknowns are in the correct range
            ffdhe_private_use(v) => *v,
            ecdhe_private_use(v) => *v,
            unknown(v) => *v,
        }
    }
    pub fn from_u16(v: u16) -> Self {
        match v {
            0x0017 => Self::secp256r1,
            0x0018 => Self::secp384r1,
            0x0019 => Self::secp521r1,
            0x001D => Self::x25519,
            0x001E => Self::x448,
            0x0100 => Self::ffdhe2048,
            0x0101 => Self::ffdhe3072,
            0x0102 => Self::ffdhe4096,
            0x0103 => Self::ffdhe6144,
            0x0104 => Self::ffdhe8192,
            0x01FC..=0x01FF => Self::ffdhe_private_use(v),
            0xFE00..=0xFEFF => Self::ecdhe_private_use(v),
            _ => Self::unknown(v),
        }
    }

    parser!(parse<Self> => {
        map(as_bytes(be_u16), |v| NamedGroup::from_u16(v))
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_u16().to_be_bytes());
    }
}

////////////////////////////////////////////////////////////////////////////////
// https://tools.ietf.org/html/rfc8446#section-4.2.3
////////////////////////////////////////////////////////////////////////////////

/*
enum {
    // RSASSA-PKCS1-v1_5 algorithms
    rsa_pkcs1_sha256(0x0401),
    rsa_pkcs1_sha384(0x0501),
    rsa_pkcs1_sha512(0x0601),

    // ECDSA algorithms
    ecdsa_secp256r1_sha256(0x0403),
    ecdsa_secp384r1_sha384(0x0503),
    ecdsa_secp521r1_sha512(0x0603),

    // RSASSA-PSS algorithms with public key OID rsaEncryption
    rsa_pss_rsae_sha256(0x0804),
    rsa_pss_rsae_sha384(0x0805),
    rsa_pss_rsae_sha512(0x0806),

    // EdDSA algorithms
    ed25519(0x0807),
    ed448(0x0808),

    // RSASSA-PSS algorithms with public key OID RSASSA-PSS
    rsa_pss_pss_sha256(0x0809),
    rsa_pss_pss_sha384(0x080a),
    rsa_pss_pss_sha512(0x080b),

    // Legacy algorithms
    rsa_pkcs1_sha1(0x0201),
    ecdsa_sha1(0x0203),

    // Reserved Code Points
    private_use(0xFE00..0xFFFF),
    (0xFFFF)
} SignatureScheme;

struct {
    SignatureScheme supported_signature_algorithms<2..2^16-2>;
} SignatureSchemeList;
*/

#[derive(Debug)]
pub struct SignatureSchemeList {
    pub algorithms: Vec<SignatureScheme>,
}

impl SignatureSchemeList {
    parser!(parse<Self> => {
        seq!(c => {
            let data = c.next(varlen_vector(2, exp2(16) - 2))?;
            let (algorithms, _) = complete(many(SignatureScheme::parse))(data)?;
            Ok(Self { algorithms })
        })
    });
    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(2, exp2(16) - 2, out, |out| {
            for a in self.algorithms.iter() {
                a.serialize(out);
            }
        });
    }
}

#[derive(Debug)]
pub enum SignatureScheme {
    // RSASSA-PKCS1-v1_5 algorithms
    rsa_pkcs1_sha256,
    rsa_pkcs1_sha384,
    rsa_pkcs1_sha512,

    // ECDSA algorithms
    ecdsa_secp256r1_sha256,
    ecdsa_secp384r1_sha384,
    ecdsa_secp521r1_sha512,

    // RSASSA-PSS algorithms with public key OID rsaEncryption
    rsa_pss_rsae_sha256,
    rsa_pss_rsae_sha384,
    rsa_pss_rsae_sha512,

    // EdDSA algorithms
    ed25519,
    ed448,

    // RSASSA-PSS algorithms with public key OID RSASSA-PSS
    rsa_pss_pss_sha256,
    rsa_pss_pss_sha384,
    rsa_pss_pss_sha512,

    // Legacy algorithms
    rsa_pkcs1_sha1,
    ecdsa_sha1,

    // Reserved Code Points
    private_use(u16),

    unknown(u16),
}

impl SignatureScheme {
    fn to_u16(&self) -> u16 {
        use SignatureScheme::*;
        match self {
            rsa_pkcs1_sha256 => 0x0401,
            rsa_pkcs1_sha384 => 0x0501,
            rsa_pkcs1_sha512 => 0x0601,
            ecdsa_secp256r1_sha256 => 0x0403,
            ecdsa_secp384r1_sha384 => 0x0503,
            ecdsa_secp521r1_sha512 => 0x0603,
            rsa_pss_rsae_sha256 => 0x0804,
            rsa_pss_rsae_sha384 => 0x0805,
            rsa_pss_rsae_sha512 => 0x0806,
            ed25519 => 0x0807,
            ed448 => 0x0808,
            rsa_pss_pss_sha256 => 0x0809,
            rsa_pss_pss_sha384 => 0x080a,
            rsa_pss_pss_sha512 => 0x080b,
            rsa_pkcs1_sha1 => 0x0201,
            ecdsa_sha1 => 0x0203,
            private_use(v) => *v,
            unknown(v) => *v,
        }
    }
    fn from_u16(v: u16) -> Self {
        use SignatureScheme::*;
        match v {
            0x0401 => rsa_pkcs1_sha256,
            0x0501 => rsa_pkcs1_sha384,
            0x0601 => rsa_pkcs1_sha512,
            0x0403 => ecdsa_secp256r1_sha256,
            0x0503 => ecdsa_secp384r1_sha384,
            0x0603 => ecdsa_secp521r1_sha512,
            0x0804 => rsa_pss_rsae_sha256,
            0x0805 => rsa_pss_rsae_sha384,
            0x0806 => rsa_pss_rsae_sha512,
            0x0807 => ed25519,
            0x0808 => ed448,
            0x0809 => rsa_pss_pss_sha256,
            0x080a => rsa_pss_pss_sha384,
            0x080b => rsa_pss_pss_sha512,
            0x0201 => rsa_pkcs1_sha1,
            0x0203 => ecdsa_sha1,
            0xFE00..=0xFFFF => private_use(v),
            _ => unknown(v),
        }
    }
    parser!(pub parse<Self> => {
        map(as_bytes(be_u16), |v| Self::from_u16(v))
    });

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.to_u16().to_be_bytes());
    }
}

////////////////////////////////////////////////////////////////////////////////
// RFC 8446 4.2.1. Supported Versions
// https://tools.ietf.org/html/rfc8446#section-4.2.1
////////////////////////////////////////////////////////////////////////////////

/*
struct {
    select (Handshake.msg_type) {
        case client_hello:
            ProtocolVersion versions<2..254>;

        case server_hello: // and HelloRetryRequest
            ProtocolVersion selected_version;
    };
} SupportedVersions;
*/

#[derive(Debug)]
pub struct SupportedVersionsClientHello {
    /// At least one version supported by the client.
    pub versions: Vec<ProtocolVersion>,
}

impl SupportedVersionsClientHello {
    parser!(parse<Self> => {
        seq!(c => {
            let data = c.next(varlen_vector(2, 254))?;
            let versions = c.next(complete(many1(as_bytes(be_u16))))?;
            Ok(Self { versions })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(2, 254, out, |out| {
            for v in self.versions.iter() {
                out.extend_from_slice(&v.to_be_bytes());
            }
        });
    }
}

#[derive(Debug)]
pub struct SupportedVersionsServerHello {
    pub selected_version: ProtocolVersion,
}

impl SupportedVersionsServerHello {
    parser!(parse<Self> => {
        map(as_bytes(be_u16), |v| Self { selected_version: v })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.selected_version.to_be_bytes());
    }
}

fn parse_supported_versions(input: Bytes, msg_type: HandshakeType) -> ParseResult<Extension> {
    if msg_type == HandshakeType::client_hello {
        map(SupportedVersionsClientHello::parse, |v| {
            Extension::SupportedVersionsClientHello(v)
        })(input)
    } else if msg_type == HandshakeType::server_hello {
        map(SupportedVersionsServerHello::parse, |v| {
            Extension::SupportedVersionsServerHello(v)
        })(input)
    } else {
        Err(err_msg("Unsupported msg_type"))
    }
}

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    opaque cookie<1..2^16-1>;
} Cookie;
*/

#[derive(Debug)]
pub struct Cookie {
    pub data: Bytes,
}
impl Cookie {
    parser!(parse<Self> => {
        map(varlen_vector(1, U16_LIMIT),
            |data| Cookie { data })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(1, U16_LIMIT, out, |out| {
            out.extend_from_slice(&self.data);
        });
    }
}

////////////////////////////////////////////////////////////////////////////////
// RFC 8446 4.2.8. Key Share
// https://tools.ietf.org/html/rfc8446#section-4.2.8
////////////////////////////////////////////////////////////////////////////////

/*
struct {
    NamedGroup group;
    opaque key_exchange<1..2^16-1>;
} KeyShareEntry;

struct {
    KeyShareEntry client_shares<0..2^16-1>;
} KeyShareClientHello;
*/

#[derive(Debug)]
pub struct KeyShareClientHello {
    pub client_shares: Vec<KeyShareEntry>,
}

impl KeyShareClientHello {
    parser!(parse<Self> => { seq!(c => {
		let data = c.next(varlen_vector(1, U16_LIMIT))?;

		let (out, _) = map(complete(many1(KeyShareEntry::parse)),
			|client_shares| {
				KeyShareClientHello { client_shares }
			})(data)?;

		Ok(out)
	}) });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(0, U16_LIMIT, out, |out| {
            for e in self.client_shares.iter() {
                e.serialize(out);
            }
        });
    }
}

tls_struct!(KeyShareHelloRetryRequest => {
    NamedGroup selected_group;
});

tls_struct!(KeyShareServerHello => {
    KeyShareEntry server_share;
});

#[derive(Debug)]
pub struct KeyShareEntry {
    pub group: NamedGroup,
    pub key_exchange: Bytes,
}

impl KeyShareEntry {
    // TODO: Check the size of the key_exchange?

    parser!(parse<Self> => {
        seq!(c => {
            let group = c.next(NamedGroup::parse)?;
            let key_exchange = c.next(varlen_vector(1, U16_LIMIT))?;
            Ok(KeyShareEntry { group, key_exchange })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        self.group.serialize(out);
        serialize_varlen_vector(1, U16_LIMIT, out, |out| {
            out.extend_from_slice(&self.key_exchange);
        });
    }
}

fn parse_key_share(input: Bytes, msg_type: HandshakeType) -> ParseResult<Extension> {
    match msg_type {
        HandshakeType::client_hello => map(KeyShareClientHello::parse, |v| {
            Extension::KeyShareClientHello(v)
        })(input),
        HandshakeType::hello_retry_request => map(KeyShareHelloRetryRequest::parse, |v| {
            Extension::KeyShareHelloRetryRequest(v)
        })(input),
        HandshakeType::server_hello => map(KeyShareServerHello::parse, |v| {
            Extension::KeyShareServerHello(v)
        })(input),
        _ => Err(err_msg("Unsupported msg_type")),
    }
}

/*
struct {
    uint8 legacy_form = 4;
    opaque X[coordinate_length];
    opaque Y[coordinate_length];
} UncompressedPointRepresentation;
*/

#[derive(Debug)]
pub struct UncompressedPointRepresentation {
    pub legacy_form: u8,
    pub x: Bytes,
    pub y: Bytes,
}

impl UncompressedPointRepresentation {
    fn coordinate_size(group: NamedGroup) -> Result<usize> {
        Ok(match group {
            NamedGroup::secp256r1 => 32,
            NamedGroup::secp384r1 => 48,
            NamedGroup::secp521r1 => 66,
            _ => {
                return Err(err_msg("Unsupported group"));
            }
        })
    }

    fn parse(input: Bytes, group: NamedGroup) -> ParseResult<Self> {
        let size = Self::coordinate_size(group)?;
        let parser = seq!(c => {
            let legacy_form = c.next(as_bytes(be_u8))?;
            let x = c.next(take_exact(size))?;
            let y = c.next(take_exact(size))?;
            Ok(Self { legacy_form, x, y })
        });

        parser(input)
    }

    fn serialize(&self, group: NamedGroup, out: &mut Vec<u8>) -> Result<()> {
        let size = Self::coordinate_size(group)?;
        if size != self.x.len() || size != self.y.len() {
            return Err(err_msg("Coordinates incorrect size"));
        }

        out.push(self.legacy_form);
        out.extend_from_slice(&self.x);
        out.extend_from_slice(&self.y);

        Ok(())
    }
}
