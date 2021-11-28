use common::bytes::Bytes;
use common::errors::*;
use parsing::binary::*;
use parsing::*;

use super::extensions::*;
use super::parsing::*;
use crate::dh::DiffieHellmanFn;
use crate::elliptic::*;
use crate::random::*;
use crate::tls::cipher_suite::CipherSuite;
use crate::tls::options::ClientOptions;

pub const TLS_1_0_VERSION: u16 = 0x0301;
pub const TLS_1_2_VERSION: u16 = 0x0303;
pub const TLS_1_3_VERSION: u16 = 0x0304;

pub type ProtocolVersion = u16;

// TODO: Use test vectors
// https://tools.ietf.org/html/draft-ietf-tls-tls13-vectors-06

/*
struct {
    HandshakeType msg_type;    /* handshake type */
    uint24 length;             /* remaining bytes in message */
    select (Handshake.msg_type) {
        case client_hello:          ClientHello;
        case server_hello:          ServerHello;
        case end_of_early_data:     EndOfEarlyData;
        case encrypted_extensions:  EncryptedExtensions;
        case certificate_request:   CertificateRequest;
        case certificate:           Certificate;
        case certificate_verify:    CertificateVerify;
        case finished:              Finished;
        case new_session_ticket:    NewSessionTicket;
        case key_update:            KeyUpdate;
    };
} Handshake;
*/

// TODO: Definately need to implement the retry message
#[derive(Debug)]
pub enum Handshake {
    ClientHello(ClientHello),
    ServerHello(ServerHello),
    EndOfEarlyData,
    EncryptedExtensions(EncryptedExtensions),
    CertificateRequest(CertificateRequest),
    Certificate(Certificate),
    CertificateVerify(CertificateVerify),
    Finished(Finished),
    NewSessionTicket(NewSessionTicket),
    KeyUpdate(KeyUpdate), // TODO: This is something that should implement at the record layer.
    ServerHelloDone,      // Emptydata
    ClientKeyExchange(ClientKeyExchange),
    ServerKeyExchange(ServerKeyExchange),
}

impl Handshake {
    pub fn parse(input: Bytes, protocol_version: ProtocolVersion) -> Result<(Self, Bytes)> {
        seq!(c => {
            let msg_type = c.next(HandshakeType::parse)?;
            let payload = c.next(varlen_vector(0, U24_LIMIT))?;

            let res = match msg_type {
                HandshakeType::ClientHello => complete(map(
                    ClientHello::parse, |v| Handshake::ClientHello(v))
                )(payload),
                HandshakeType::ServerHello => complete(map(
                    ServerHello::parse, |v| Handshake::ServerHello(v))
                )(payload),
                HandshakeType::EndOfEarlyData => {
                    if payload.len() == 0 {
                        Ok((Handshake::EndOfEarlyData, Bytes::new()))
                    } else {
                        Err(err_msg("Expected emptydata"))
                    }
                },
                HandshakeType::EncryptedExtensions => complete(map(
                    EncryptedExtensions::parse, |v| Handshake::EncryptedExtensions(v))
                )(payload),
                HandshakeType::CertificateRequest => complete(map(
                    CertificateRequest::parse, |v| Handshake::CertificateRequest(v))
                )(payload),
                HandshakeType::Certificate => complete(map(
                    |input| Certificate::parse(input, protocol_version), |v| Handshake::Certificate(v))
                )(payload),
                HandshakeType::CertificateVerify => complete(map(
                    CertificateVerify::parse, |v| Handshake::CertificateVerify(v))
                )(payload),
                HandshakeType::Finished => complete(map(
                    Finished::parse, |v| Handshake::Finished(v))
                )(payload),
                HandshakeType::NewSessionTicket => complete(map(
                    NewSessionTicket::parse, |v| Handshake::NewSessionTicket(v))
                )(payload),

                // HandshakeType::HelloRequest => todo!(),
                HandshakeType::ServerKeyExchange => {
                    Ok((Handshake::ServerKeyExchange(ServerKeyExchange { data: payload }), Bytes::new()))
                },
                HandshakeType::ServerHelloDone => {
                    if payload.len() != 0 {
                        return Err(err_msg("Expected empty ServerHelloDone"));
                    }

                    Ok((Handshake::ServerHelloDone, Bytes::new()))
                },
                HandshakeType::ClientKeyExchange => {
                    Ok((Handshake::ClientKeyExchange(ClientKeyExchange { data: payload }), Bytes::new()))
                },
                // HandshakeType::KeyUpdate => todo!(),
                // HandshakeType::MessageHash => todo!(),

                _ => {
                    return Err(format_err!("Unsupported handshake type: {:?}", msg_type));
                }
            };

            let (v, _) = res?;
            Ok(v)
        })(input)
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        let msg_type = match self {
            Handshake::ClientHello(_) => HandshakeType::ClientHello,
            Handshake::ServerHello(_) => HandshakeType::ServerHello,
            Handshake::EndOfEarlyData => HandshakeType::EndOfEarlyData,
            Handshake::EncryptedExtensions(_) => HandshakeType::EncryptedExtensions,
            Handshake::CertificateRequest(_) => HandshakeType::CertificateRequest,
            Handshake::Certificate(_) => HandshakeType::Certificate,
            Handshake::CertificateVerify(_) => HandshakeType::CertificateVerify,
            Handshake::Finished(_) => HandshakeType::Finished,
            Handshake::NewSessionTicket(_) => HandshakeType::NewSessionTicket,
            Handshake::KeyUpdate(_) => HandshakeType::KeyUpdate,
            Handshake::ServerHelloDone => HandshakeType::ServerHelloDone,
            Handshake::ClientKeyExchange(_) => HandshakeType::ClientKeyExchange,
            Handshake::ServerKeyExchange(_) => HandshakeType::ServerKeyExchange,
        };

        msg_type.serialize(out);

        serialize_varlen_vector(0, U24_LIMIT, out, |out| match self {
            Handshake::ClientHello(v) => v.serialize(out),
            Handshake::ServerHello(v) => v.serialize(out),
            Handshake::EndOfEarlyData => {}
            Handshake::EncryptedExtensions(v) => v.serialize(out),
            Handshake::CertificateRequest(v) => v.serialize(out),
            Handshake::Certificate(v) => v.serialize(out),
            Handshake::CertificateVerify(v) => v.serialize(out),
            Handshake::Finished(v) => v.serialize(out),
            Handshake::NewSessionTicket(v) => v.serialize(out),
            Handshake::KeyUpdate(v) => v.serialize(out),
            Handshake::ServerHelloDone => {}
            Handshake::ClientKeyExchange(v) => out.extend_from_slice(&v.data),
            Handshake::ServerKeyExchange(v) => out.extend_from_slice(&v.data),
        });
    }
}

tls_enum_u8!(HandshakeType => {
    HelloRequest(0), // TLS 1.2
    ClientHello(1),
    ServerHello(2),
    NewSessionTicket(4),
    EndOfEarlyData(5),
    // HelloRetryRequestRESERVED(6), // RESERVED
    EncryptedExtensions(8),
    Certificate(11),
    ServerKeyExchange(12), // TLS 1.2
    CertificateRequest(13),
    ServerHelloDone(14), // TLS 1.2
    CertificateVerify(15),
    ClientKeyExchange(16), // TLS 1.2
    Finished(20),
    // certificate_url_RESERVED(21),
    // certificate_status_RESERVED(22),
    // supplemental_data_RESERVED(23),
    KeyUpdate(24),
    MessageHash(254),
    (255)
});

////////////////////////////////////////////////////////////////////////////////
// https://tools.ietf.org/html/rfc8446#section-4.1.2
////////////////////////////////////////////////////////////////////////////////

// See here for 1.2
// https://tools.ietf.org/html/rfc5246#section-7.4.1.2
// ^ Needs for attention to the construction of the random value

/*
struct {
    ProtocolVersion legacy_version = 0x0303;    /* TLS v1.2 */
    Random random;
    opaque legacy_session_id<0..32>;
    CipherSuite cipher_suites<2..2^16-2>;
    opaque legacy_compression_methods<1..2^8-1>;
    Extension extensions<8..2^16-1>;
} ClientHello;
*/
#[derive(Debug, Clone)]
pub struct ClientHello {
    pub legacy_version: ProtocolVersion,
    // 32 random bytes
    pub random: Bytes,
    // 0-32 bytes
    pub legacy_session_id: Bytes,
    pub cipher_suites: Vec<CipherSuite>,
    pub legacy_compression_methods: Bytes,
    pub extensions: Vec<Extension>,
}

// TODO: Support ESNI: https://blog.cloudflare.com/encrypted-sni/

impl ClientHello {
    /// Creates a reasonable
    pub async fn plain(client_shares: &[KeyShareEntry], options: &ClientOptions) -> Result<Self> {
        let mut extensions = vec![];

        let mut random_buf = [0u8; 32];
        secure_random_bytes(&mut random_buf).await?;

        // TODO: Can we send this later as an encrypted header.
        if !options.hostname.is_empty() {
            extensions.push(Extension::ServerName(ServerNameList {
                names: vec![ServerName {
                    typ: NameType::host_name,
                    data: Bytes::from(options.hostname.as_bytes()), // TODO: Must be ASCII
                }],
            }));
        }

        /*
        Other extensions to look into:
        - encrypt_then_mac
        */

        // Required to be sent in ClientHello.
        extensions.push(Extension::SupportedVersionsClientHello(
            SupportedVersionsClientHello {
                versions: vec![TLS_1_3_VERSION, TLS_1_2_VERSION],
            },
        ));

        // TLS 1.3 mandatory-to-implement ciphers are documented in:
        // https://datatracker.ietf.org/doc/html/rfc8446#section-9.1

        // Required to be send in ClientHello for DHE/ECDHE
        extensions.push(Extension::SupportedGroups(NamedGroupList {
            groups: options.supported_groups.clone(),
        }));

        // Required for certificate authentication
        extensions.push(Extension::SignatureAlgorithms(SignatureSchemeList {
            algorithms: options.supported_signature_algorithms.clone(),
        }));

        extensions.push(Extension::KeyShareClientHello(KeyShareClientHello {
            client_shares: client_shares.to_vec(),
        }));

        if !options.alpn_ids.is_empty() {
            extensions.push(Extension::ALPN(ProtocolNameList {
                names: options.alpn_ids.clone(),
            }));
        }

        // For TLS 1.2
        extensions.push(Extension::SupportedPointFormats(ECPointFormatList {
            formats: vec![ECPointFormat::uncompressed],
        }));

        if options.certificate_auth.is_some() {
            extensions.push(Extension::PostHandshakeAuth);
        }

        // TODO: PSK if any must always be the last extension.

        // XXX: See
        //  9.2.  Mandatory-to-Implement Extensions

        Ok(Self {
            legacy_version: TLS_1_2_VERSION,
            random: Bytes::from(random_buf.to_vec()),
            legacy_session_id: Bytes::new(),
            cipher_suites: options.supported_cipher_suites.clone(),
            legacy_compression_methods: Bytes::from((&[0]).to_vec()),
            extensions,
        })
    }

    parser!(parse<Self> => { seq!(c => {
		let legacy_version = c.next(as_bytes(be_u16))?;
		let random = c.next(take_exact(32))?;
		let legacy_session_id = c.next(varlen_vector(0, 32))?;
		let cipher_suites = {
			let data = c.next(varlen_vector(2, exp2(16) - 2))?;
			let (arr, _) = complete(many(CipherSuite::parse))(data)?;
			arr
		};
		let legacy_compression_methods = c.next(varlen_vector(1, U8_LIMIT))?;
		let extensions = {
			let data = c.next(varlen_vector(8, U16_LIMIT))?;
			let (arr, _) = complete(many(
				|v| Extension::parse(v, HandshakeType::ClientHello)))(data)?;
			arr
		};

		Ok(ClientHello {
			legacy_version, random, legacy_session_id, cipher_suites,
			legacy_compression_methods, extensions
		})
	}) });

    pub fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.legacy_version.to_be_bytes());
        assert_eq!(self.random.len(), 32);
        out.extend_from_slice(&self.random);

        serialize_varlen_vector(0, 32, out, |out| {
            out.extend_from_slice(&self.legacy_session_id);
        });
        serialize_varlen_vector(2, exp2(16) - 2, out, |out| {
            for c in self.cipher_suites.iter() {
                c.serialize(out);
            }
        });
        serialize_varlen_vector(1, U8_LIMIT, out, |out| {
            out.extend_from_slice(&self.legacy_compression_methods);
        });
        serialize_varlen_vector(8, U16_LIMIT, out, |out| {
            for e in self.extensions.iter() {
                e.serialize(HandshakeType::ClientHello, out).unwrap();
            }
        });
    }
}

////////////////////////////////////////////////////////////////////////////////
// RFC 8446 4.1.3. Server Hello
// https://tools.ietf.org/html/rfc8446#section-4.1.3
////////////////////////////////////////////////////////////////////////////////

/*
struct {
    ProtocolVersion legacy_version = 0x0303;    /* TLS v1.2 */
    Random random;
    opaque legacy_session_id_echo<0..32>;
    CipherSuite cipher_suite;
    uint8 legacy_compression_method = 0;
    Extension extensions<6..2^16-1>;
} ServerHello;
*/

#[derive(Debug)]
pub struct ServerHello {
    pub legacy_version: ProtocolVersion,
    pub random: Bytes,
    pub legacy_session_id_echo: Bytes, // TODO: Check this matches the client hello
    pub cipher_suite: CipherSuite,
    pub legacy_compression_method: u8,
    pub extensions: Vec<Extension>,
}

// TODO: Validate everywhere that we don't get duplicate extensions

impl ServerHello {
    parser!(parse<ServerHello> => { seq!(c => {
		let legacy_version = c.next(as_bytes(be_u16))?;
		let random = c.next(take_exact(32))?;
		let legacy_session_id_echo = c.next(varlen_vector(0, 32))?;
		let cipher_suite = c.next(CipherSuite::parse)?;
		let legacy_compression_method = c.next(as_bytes(be_u8))?;
		let extensions = {
			let data = c.next(varlen_vector(6, U16_LIMIT))?;
			let (arr, _) = complete(many(
				|i| Extension::parse(i, HandshakeType::ServerHello)))(data)?;
			arr
		};

		Ok(Self {
			legacy_version, random, legacy_session_id_echo, cipher_suite,
			legacy_compression_method, extensions
		})
	}) });

    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.legacy_version.to_be_bytes());
        out.extend_from_slice(&self.random);
        serialize_varlen_vector(0, 32, out, |out| {
            out.extend_from_slice(&self.legacy_session_id_echo);
        });
        self.cipher_suite.serialize(out);
        out.push(self.legacy_compression_method);
        serialize_varlen_vector(6, U16_LIMIT, out, |out| {
            for e in self.extensions.iter() {
                e.serialize(HandshakeType::ServerHello, out).unwrap();
            }
        });
    }
}

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    uint32 ticket_lifetime;
    uint32 ticket_age_add;
    opaque ticket_nonce<0..255>;
    opaque ticket<1..2^16-1>;
    Extension extensions<0..2^16-2>;
} NewSessionTicket;
*/

#[derive(Debug)]
pub struct NewSessionTicket {
    pub ticket_lifetime: u32,
    pub ticket_age_add: u32,
    pub ticket_nonce: Bytes,
    pub ticket: Bytes,
    pub extensions: Vec<Extension>,
}

impl NewSessionTicket {
    parser!(parse<Self> => {
        seq!(c => {
            let ticket_lifetime = c.nexts(be_u32)?;
            let ticket_age_add = c.nexts(be_u32)?;

            let ticket_nonce = c.next(varlen_vector(0, U8_LIMIT))?;
            let ticket = c.next(varlen_vector(1, U16_LIMIT))?;

            let extensions_data = c.next(varlen_vector(0, U16_LIMIT - 1))?;
            let (extensions, _) = complete(many(
                    |i| Extension::parse(i, HandshakeType::NewSessionTicket)
                ))(extensions_data)?;
            Ok(Self {
                ticket_lifetime,
                ticket_age_add,
                ticket_nonce,
                ticket,
                extensions
            })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.ticket_lifetime.to_be_bytes());
        out.extend_from_slice(&self.ticket_age_add.to_be_bytes());
        serialize_varlen_vector(0, U8_LIMIT, out, |out| {
            out.extend_from_slice(&self.ticket_nonce)
        });
        serialize_varlen_vector(1, U16_LIMIT, out, |out| out.extend_from_slice(&self.ticket));
        serialize_varlen_vector(0, U16_LIMIT - 1, out, |out| {
            for extension in &self.extensions {
                extension
                    .serialize(HandshakeType::NewSessionTicket, out)
                    .unwrap();
            }
        });
    }
}

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    NamedGroup selected_group;
} KeyShareHelloRetryRequest;
*/

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    Extension extensions<0..2^16-1>;
} EncryptedExtensions;
*/

#[derive(Debug)]
pub struct EncryptedExtensions {
    pub extensions: Vec<Extension>,
}

impl EncryptedExtensions {
    parser!(parse<Self> => {
        seq!(c => {
            let data = c.next(varlen_vector(0, U16_LIMIT))?;
            let (extensions, _) = complete(many(
                    |i| Extension::parse(i, HandshakeType::EncryptedExtensions)
                ))(data)?;
            Ok(Self { extensions })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(0, U16_LIMIT, out, |out| {
            for e in self.extensions.iter() {
                e.serialize(HandshakeType::EncryptedExtensions, out)
                    .unwrap();
            }
        });
    }
}

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    select (certificate_type) {
        case RawPublicKey:
        /* From RFC 7250 ASN.1_subjectPublicKeyInfo */
        opaque ASN1_subjectPublicKeyInfo<1..2^24-1>;

        case X509:
        opaque cert_data<1..2^24-1>;
    };
    Extension extensions<0..2^16-1>;
} CertificateEntry;

struct {
    opaque certificate_request_context<0..2^8-1>;
    CertificateEntry certificate_list<0..2^24-1>;
} Certificate;


In TLS 1.2 the format is:

opaque ASN.1Cert<1..2^24-1>;

struct {
    ASN.1Cert certificate_list<0..2^24-1>;
} Certificate;
*/

#[derive(Debug)]
pub struct Certificate {
    pub certificate_request_context: Bytes,
    pub certificate_list: Vec<CertificateEntry>,
}

impl Certificate {
    fn parse(input: Bytes, protocol_version: ProtocolVersion) -> Result<(Self, Bytes)> {
        seq!(c => {
            if protocol_version == TLS_1_3_VERSION {
                let certificate_request_context = c.next(varlen_vector(0, U8_LIMIT))?;
                let certificate_list = {
                    let data = c.next(varlen_vector(0, U24_LIMIT))?;
                    let (arr, _) = complete(many(CertificateEntry::parse))(data)?;
                    arr
                };

                Ok(Self { certificate_request_context, certificate_list })
            } else {
                let certificate_list = c.next(varlen_vector(0, U24_LIMIT))?;
                let (certs, _) = complete(many(varlen_vector(1, U24_LIMIT)))(certificate_list)?;

                Ok(Self {
                    certificate_request_context: Bytes::new(),
                    certificate_list: certs.into_iter().map(|cert| {
                        CertificateEntry { cert, extensions: vec![] }
                    }).collect()
                })
            }
        })(input)
    }

    fn serialize(&self, out: &mut Vec<u8>) {
        // TODO: Implement TLS 1.2 certificate serialization
        // Must warn if unsupported fields are passed.

        serialize_varlen_vector(0, U8_LIMIT, out, |out| {
            out.extend_from_slice(&self.certificate_request_context);
        });
        serialize_varlen_vector(0, U24_LIMIT, out, |out| {
            for c in self.certificate_list.iter() {
                c.serialize(out);
            }
        });
    }
}

/// NOTE: Only supports being placed in a Certificate message.
#[derive(Debug)]
pub struct CertificateEntry {
    pub cert: Bytes,
    pub extensions: Vec<Extension>,
}

impl CertificateEntry {
    parser!(parse<Self> => { seq!(c => {
		let cert = c.next(varlen_vector(1, U24_LIMIT))?;
		let extensions = {
			let data = c.next(varlen_vector(0, U16_LIMIT))?;
			let (arr, _) = complete(many(
				|i| Extension::parse(i, HandshakeType::Certificate)))(data)?;
			arr
		};

		Ok(Self { cert, extensions })
	}) });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(1, U24_LIMIT, out, |out| {
            out.extend_from_slice(&self.cert);
        });
        serialize_varlen_vector(0, U16_LIMIT, out, |out| {
            for e in self.extensions.iter() {
                e.serialize(HandshakeType::Certificate, out).unwrap();
            }
        });
    }
}

tls_enum_u8!(CertificateType => {
    X509(0),
    RawPublicKey(2),
    (255)
});

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    opaque certificate_request_context<0..2^8-1>;
    Extension extensions<2..2^16-1>;
} CertificateRequest;
*/

#[derive(Debug)]
pub struct CertificateRequest {
    pub certificate_request_context: Bytes,
    pub extensions: Vec<Extension>,
}

impl CertificateRequest {
    parser!(parse<Self> => { seq!(c => {
		let certificate_request_context = c.next(varlen_vector(0, U8_LIMIT))?;
		let extensions = {
			let data = c.next(varlen_vector(2, U16_LIMIT))?;
			let (arr, _) = complete(many(
					|i| Extension::parse(i, HandshakeType::CertificateRequest)
				))(data)?;
			arr
		};

		Ok(Self { certificate_request_context, extensions })
	}) });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(0, U8_LIMIT, out, |out| {
            out.extend_from_slice(&self.certificate_request_context);
        });
        serialize_varlen_vector(2, U16_LIMIT, out, |out| {
            for e in self.extensions.iter() {
                e.serialize(HandshakeType::CertificateRequest, out).unwrap();
            }
        });
    }
}

////////////////////////////////////////////////////////////////////////////////

/*
--- TLS 1.3
struct {
    SignatureScheme algorithm;
    opaque signature<0..2^16-1>;
} CertificateVerify;

--- TLS 1.2:
struct {
    SignatureAndHashAlgorithm algorithm;
    opaque signature<0..2^16-1>;
} DigitallySigned;

enum {
    signature_algorithms(13), (65535)
} ExtensionType;

enum{
    none(0), md5(1), sha1(2), sha224(3), sha256(4), sha384(5),
    sha512(6), (255)
} HashAlgorithm;
enum {
    anonymous(0), rsa(1), dsa(2), ecdsa(3), (255)
} SignatureAlgorithm;

struct {
    HashAlgorithm hash;
    SignatureAlgorithm signature;
} SignatureAndHashAlgorithm;
*/

/// The CertificateVerify struct in TLS 1.2/1.3 and the DigitallySigned struct
/// in TLS 1.2
#[derive(Debug, Clone)]
pub struct CertificateVerify {
    pub algorithm: SignatureScheme,
    pub signature: Bytes,
}

impl CertificateVerify {
    parser!(parse<Self> => { seq!(c => {
		let algorithm = c.next(SignatureScheme::parse)?;
		let signature = c.next(varlen_vector(0, U16_LIMIT))?;
		Ok(Self { algorithm, signature })
	}) });

    fn serialize(&self, out: &mut Vec<u8>) {
        self.algorithm.serialize(out);
        serialize_varlen_vector(0, U16_LIMIT, out, |out| {
            out.extend_from_slice(&self.signature);
        });
    }
}

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    opaque verify_data[Hash.length];
} Finished;
*/

#[derive(Debug)]
pub struct Finished {
    pub verify_data: Bytes,
}

impl Finished {
    // Need to know the hash length to parse this (or just take everything?)
    fn parse(input: Bytes /* , hash_len: usize */) -> ParseResult<Self> {
        let v = Self { verify_data: input };
        Ok((v, Bytes::new()))
        // let parser = map(take_exact(hash_len), |v| {
        // 	Self { verify_data: v }
        // });

        // parser(input)
    }

    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.verify_data);
    }
}

////////////////////////////////////////////////////////////////////////////////

tls_struct!(KeyUpdate => {
    KeyUpdateRequest request_update;
});

tls_enum_u8!(KeyUpdateRequest => {
    update_not_requested(0), update_requested(1), (255)
});

////////////////////////////////////////////////////////////////////////////////

// rfc8422

#[derive(Debug)]
pub struct ServerKeyExchange {
    pub data: Bytes,
}

impl ServerKeyExchange {
    pub fn ec_diffie_hellman(&self) -> Result<ServerKeyExchangeECDHE> {
        let (v, _) = complete(ServerKeyExchangeECDHE::parse)(self.data.clone())?;
        Ok(v)
    }
}

// Parsing as 'ec_diffie_hellman()'

#[derive(Debug)]
pub struct ClientKeyExchange {
    pub data: Bytes,
}

/*
struct {
    select (KeyExchangeAlgorithm) {
        case rsa:
            EncryptedPreMasterSecret;
        case dhe_dss:
        case dhe_rsa:
        case dh_dss:
        case dh_rsa:
        case dh_anon:
            ClientDiffieHellmanPublic;
    } exchange_keys;
} ClientKeyExchange;
*/

/*
struct {
    select (KeyExchangeAlgorithm) {
        case dh_anon:
            ServerDHParams params;
        case dhe_dss:
        case dhe_rsa:
            ServerDHParams params;
            digitally-signed struct {
                opaque client_random[32];
                opaque server_random[32];
                ServerDHParams params;
            } signed_params;
        case rsa:
        case dh_dss:
        case dh_rsa:
            struct {} ;
            /* message is omitted for rsa, dh_dss, and dh_rsa */
        /* may be extended, e.g., for ECDH -- see [TLSECC] */
    };
} ServerKeyExchange;

enum {
    deprecated (1..2),
    named_curve (3),
    reserved(248..255)
} ECCurveType;

struct {
    ECCurveType    curve_type;
    select (curve_type) {
        case named_curve:
            NamedCurve namedcurve;
    };
} ECParameters;

struct {
    opaque point <1..2^8-1>;
} ECPoint;

struct {
    ECParameters    curve_params;
    ECPoint         public;
} ServerECDHParams;

select (KeyExchangeAlgorithm) {
    case ec_diffie_hellman:
        ServerECDHParams    params;
        Signature           signed_params;
} ServerKeyExchange;
*/

#[derive(Debug, Clone)]
pub struct ECPoint {
    pub point: Bytes,
}

impl ECPoint {
    parser!(parse<Self> => {
        seq!(c => {
            let point = c.next(varlen_vector(1, U8_LIMIT))?;
            Ok(Self { point })
        })
    });

    pub fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(1, U8_LIMIT, out, |out| {
            out.extend_from_slice(&self.point);
        })
    }
}

tls_enum_u8!(ECCurveType => {
    named_curve(3),
    (255)
});

tls_struct!(ECParameters => {
    ECCurveType curve_type;
    // TODO: Only present if the curve_type == named_curve
    NamedGroup named_curve;
});

tls_struct!(ServerECDHParams => {
    ECParameters curve_params;
    ECPoint public;
});

tls_struct!(ServerKeyExchangeECDHE => {
    ServerECDHParams params;
    CertificateVerify signed_params;
});
