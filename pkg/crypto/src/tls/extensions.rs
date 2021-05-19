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

    ALPN(ProtocolNameList),

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

            let res = match extension_type {
                ExtensionType::ServerName => {
                    map(complete(ServerNameList::parse),
                        |v| Extension::ServerName(v))(data)
                },
                ExtensionType::MaxFragmentLength => {
                    map(complete(MaxFragmentLength::parse),
                        |v| Extension::MaxFragmentLength(v))(data)
                },
                ExtensionType::SupportedGroups => {
                    map(complete(NamedGroupList::parse),
                        |v| Extension::SupportedGroups(v))(data)
                },
                ExtensionType::SignatureAlgorithms => {
                    map(complete(SignatureSchemeList::parse),
                        |v| Extension::SignatureAlgorithms(v))(data)
                },
                ExtensionType::SupportedVersions => {
                    complete(|d| parse_supported_versions(d, msg_type))(data)
                },
                ExtensionType::Cookie => map(Cookie::parse, |v| Extension::Cookie(v))(data),
                ExtensionType::PostHandshakeAuth => {
                    if data.len() != 0 {
                        Err(err_msg("Expected empty data"))
                    } else {
                        Ok((Extension::PostHandshakeAuth, Bytes::new()))
                    }
                },
                ExtensionType::SignatureAlgorithmsCert => {
                    map(complete(SignatureSchemeList::parse),
                        |v| Extension::SignatureAlgorithmsCert(v))(data)
                },
                ExtensionType::KeyShare => {
                    complete(|d| parse_key_share(d, msg_type))(data)
                },
                ExtensionType::ApplicationLayerProtocolNegotiation => {
                    map(complete(ProtocolNameList::parse),
                        |l| Extension::ALPN(l))(data)
                }
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
            ServerName(_) => ExtensionType::ServerName,
            MaxFragmentLength(_) => ExtensionType::MaxFragmentLength,
            SupportedGroups(_) => ExtensionType::SupportedGroups,
            SignatureAlgorithms(_) => ExtensionType::SignatureAlgorithms,
            SupportedVersionsClientHello(_) => ExtensionType::SupportedVersions,
            SupportedVersionsServerHello(_) => ExtensionType::SupportedVersions,
            Cookie(_) => ExtensionType::Cookie,
            PostHandshakeAuth => ExtensionType::PostHandshakeAuth,
            SignatureAlgorithmsCert(_) => ExtensionType::SignatureAlgorithmsCert,
            KeyShareClientHello(_) => ExtensionType::KeyShare,
            KeyShareHelloRetryRequest(_) => ExtensionType::KeyShare,
            KeyShareServerHello(_) => ExtensionType::KeyShare,
            ALPN(_) => ExtensionType::ApplicationLayerProtocolNegotiation,
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
            ALPN(e) => e.serialize(out),
            Unknown { typ, data } => out.extend_from_slice(&data),
        });

        Ok(())
    }
}

#[derive(Debug)]
pub enum ExtensionType {
    ServerName,
    MaxFragmentLength,
    StatusRequest,
    SupportedGroups,
    SignatureAlgorithms,
    UseSRTP,
    Heartbeat,
    ApplicationLayerProtocolNegotiation,
    SignedCertificateTimestamp,
    ClientCertificateType,
    ServerCertificateType,
    Padding,
    PreSharedKey,
    EarlyData,
    SupportedVersions,
    Cookie,
    PskKeyExchangeModes,
    CertificateAuthorities,
    OidFilters,
    PostHandshakeAuth, // < Empty struct
    SignatureAlgorithmsCert,
    KeyShare,
    Unknown(u16),
}

impl ExtensionType {
    fn to_u16(&self) -> u16 {
        use ExtensionType::*;
        match self {
            ServerName => 0,
            MaxFragmentLength => 1,
            StatusRequest => 5,
            SupportedGroups => 10,
            SignatureAlgorithms => 13,
            UseSRTP => 14,
            Heartbeat => 15,
            ApplicationLayerProtocolNegotiation => 16,
            SignedCertificateTimestamp => 18,
            ClientCertificateType => 19,
            ServerCertificateType => 20,
            Padding => 21,
            PreSharedKey => 41,
            EarlyData => 42,
            SupportedVersions => 43,
            Cookie => 44,
            PskKeyExchangeModes => 45,
            CertificateAuthorities => 47,
            OidFilters => 48,
            PostHandshakeAuth => 49,
            SignatureAlgorithmsCert => 50,
            KeyShare => 51,
            Unknown(v) => *v,
        }
    }
    // TODO: This should be allowed to return None so that we can store unknown
    // extensions opaquely?
    fn from_u16(v: u16) -> Self {
        match v {
            0 => Self::ServerName,
            1 => Self::MaxFragmentLength,
            5 => Self::StatusRequest,
            10 => Self::SupportedGroups,
            13 => Self::SignatureAlgorithms,
            14 => Self::UseSRTP,
            15 => Self::Heartbeat,
            16 => Self::ApplicationLayerProtocolNegotiation,
            18 => Self::SignedCertificateTimestamp,
            19 => Self::ClientCertificateType,
            20 => Self::ServerCertificateType,
            21 => Self::Padding,
            41 => Self::PreSharedKey,
            42 => Self::EarlyData,
            43 => Self::SupportedVersions,
            44 => Self::Cookie,
            45 => Self::PskKeyExchangeModes,
            47 => Self::CertificateAuthorities,
            48 => Self::OidFilters,
            49 => Self::PostHandshakeAuth,
            50 => Self::SignatureAlgorithmsCert,
            51 => Self::KeyShare,
            _ => Self::Unknown(v),
        }
    }

    /// See the table on https://tools.ietf.org/html/rfc8446#section-4.2.
    /// TODO: Send 'illegal_parameter' if this happens.
    fn allowed(&self, msg_type: HandshakeType) -> bool {
        use ExtensionType::*;
        use HandshakeType::*;
        match self {
            ServerName => (msg_type == client_hello || msg_type == encrypted_extensions),
            MaxFragmentLength => (msg_type == client_hello || msg_type == encrypted_extensions),
            StatusRequest => {
                msg_type == client_hello
                    || msg_type == certificate_request
                    || msg_type == certificate
            }
            SupportedGroups => (msg_type == client_hello || msg_type == encrypted_extensions),
            SignatureAlgorithms => (msg_type == client_hello || msg_type == certificate_request),
            UseSRTP => (msg_type == client_hello || msg_type == encrypted_extensions),
            Heartbeat => (msg_type == client_hello || msg_type == encrypted_extensions),
            ApplicationLayerProtocolNegotiation => {
                msg_type == client_hello || msg_type == encrypted_extensions
            }
            SignedCertificateTimestamp => {
                msg_type == client_hello
                    || msg_type == certificate_request
                    || msg_type == certificate
            }
            ClientCertificateType => {
                msg_type == client_hello || msg_type == encrypted_extensions
            }
            ServerCertificateType => {
                msg_type == client_hello || msg_type == encrypted_extensions
            }
            Padding => (msg_type == client_hello),
            KeyShare => {
                msg_type == client_hello
                    || msg_type == server_hello
                    || msg_type == hello_retry_request
            }
            PreSharedKey => (msg_type == client_hello || msg_type == server_hello),
            PskKeyExchangeModes => (msg_type == client_hello),
            EarlyData => {
                msg_type == client_hello
                    || msg_type == encrypted_extensions
                    || msg_type == new_session_ticket
            }
            Cookie => (msg_type == client_hello || msg_type == hello_retry_request),
            SupportedVersions => {
                msg_type == client_hello
                    || msg_type == server_hello
                    || msg_type == hello_retry_request
            }
            CertificateAuthorities => (msg_type == client_hello || msg_type == certificate),
            OidFilters => (msg_type == certificate),
            PostHandshakeAuth => (msg_type == client_hello),
            SignatureAlgorithmsCert => (msg_type == client_hello || msg_type == certificate),
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

/// See RFC 6066 Section 3
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

    // TODO: When a HostName, this will be strictly ASCII
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
            let (groups, _) = complete(many1(NamedGroup::parse))(data)?;
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

    Unknown(u16),
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
            Unknown(v) => *v,
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
            _ => Self::Unknown(v),
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


////////////////////////////////////////////////////////////////////////////////
// RFC 7301: Section 3.1
// https://datatracker.ietf.org/doc/html/rfc7301#section-3.1

/*
opaque ProtocolName<1..2^8-1>;

struct {
    ProtocolName protocol_name_list<2..2^16-1>
} ProtocolNameList;
*/

#[derive(Debug)]
pub struct ProtocolNameList {
    /// In descending order of preferance.
    pub names: Vec<Bytes>
}

impl ProtocolNameList {
    parser!(parse<Self> => {
        seq!(c => {
            let names = c.next(many(varlen_vector(1, U8_LIMIT)))?;
            Ok(ProtocolNameList { names })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        for name in &self.names {
            serialize_varlen_vector(1, U8_LIMIT, out, |out| {
                out.extend_from_slice(name.as_ref());
            })
        }
    }
}

