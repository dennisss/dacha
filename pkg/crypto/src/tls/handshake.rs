use super::extensions::*;
use super::parsing::*;
use crate::dh::DiffieHellmanFn;
use crate::elliptic::*;
use crate::random::*;
use common::bytes::Bytes;
use common::errors::*;
use parsing::binary::*;
use parsing::*;

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
    KeyUpdate(KeyUpdate),
}

impl Handshake {
    parser!(pub parse<Self> => {
        seq!(c => {
            let msg_type = c.next(HandshakeType::parse)?;
            let payload = c.next(varlen_vector(0, U24_LIMIT))?;

            use HandshakeType::*;
            let res = match msg_type {
                client_hello => complete(map(
                        ClientHello::parse, |v| Handshake::ClientHello(v))
                    )(payload),
                server_hello => complete(map(
                        ServerHello::parse, |v| Handshake::ServerHello(v))
                    )(payload),
                end_of_early_data => {
                    if payload.len() == 0 {
                        Ok((Handshake::EndOfEarlyData, Bytes::new()))
                    } else {
                        Err(err_msg("Expected emptydata"))
                    }
                },
                encrypted_extensions => complete(map(
                        EncryptedExtensions::parse, |v| Handshake::EncryptedExtensions(v))
                    )(payload),
                certificate_request => complete(map(
                        CertificateRequest::parse, |v| Handshake::CertificateRequest(v))
                    )(payload),
                certificate => complete(map(
                        Certificate::parse, |v| Handshake::Certificate(v))
                    )(payload),
                certificate_verify => complete(map(
                        CertificateVerify::parse, |v| Handshake::CertificateVerify(v))
                    )(payload),
                finished => complete(map(
                        Finished::parse, |v| Handshake::Finished(v))
                    )(payload),
                _ => panic!("Unsupported handshake type")
            };

            let (v, _) = res?;
            Ok(v)
        })
    });

    pub fn serialize(&self, out: &mut Vec<u8>) {
        use Handshake::*;
        use HandshakeType::*;
        let msg_type = match self {
            ClientHello(v) => client_hello,
            ServerHello(v) => server_hello,
            EndOfEarlyData => end_of_early_data,
            EncryptedExtensions(v) => encrypted_extensions,
            CertificateRequest(v) => certificate_request,
            Certificate(v) => certificate,
            CertificateVerify(v) => certificate_verify,
            Finished(v) => finished,
            NewSessionTicket(v) => new_session_ticket,
            KeyUpdate(v) => key_update,
        };

        msg_type.serialize(out);

        serialize_varlen_vector(0, U24_LIMIT, out, |out| match self {
            ClientHello(v) => v.serialize(out),
            ServerHello(v) => v.serialize(out),
            EndOfEarlyData => {}
            EncryptedExtensions(v) => v.serialize(out),
            CertificateRequest(v) => v.serialize(out),
            Certificate(v) => v.serialize(out),
            CertificateVerify(v) => v.serialize(out),
            Finished(v) => v.serialize(out),
            // NewSessionTicket(v) => v.serialize(out),
            // KeyUpdate(v) => v.serialize(out)
            _ => panic!("Unimplemented"),
        });
    }
}

tls_enum_u8!(HandshakeType => {
    hello_request(0), // TLS 1.2
    client_hello(1),
    server_hello(2),
    new_session_ticket(4),
    end_of_early_data(5),
    hello_retry_request(6), // RESERVED
    encrypted_extensions(8),
    certificate(11),
    server_key_exchange(12), // TLS 1.2
    certificate_request(13),
    certificate_verify(15),
    client_key_exchange(16), // TLS 1.2
    finished(20),
    key_update(24),
    message_hash(254),
    (255)
});

pub const TLS_1_2_VERSION: u16 = 0x0303;
pub const TLS_1_3_VERSION: u16 = 0x0304;

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
#[derive(Debug)]
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

impl ClientHello {
    pub async fn plain(client_share: KeyShareEntry) -> Result<Self> {
        let mut random_buf = [0u8; 32];
        secure_random_bytes(&mut random_buf).await?;

        let server_name = ServerNameList {
            names: vec![ServerName {
                typ: NameType::host_name,
                data: Bytes::from(b"google.com".to_vec()),
            }],
        };

        let supported_versions = SupportedVersionsClientHello {
            versions: vec![TLS_1_3_VERSION, TLS_1_2_VERSION],
        };

        let supported_groups = NamedGroupList {
            groups: vec![
                NamedGroup::x25519,
                NamedGroup::secp256r1,
                NamedGroup::secp384r1,
            ],
        };

        let supported_algorithms = SignatureSchemeList {
            algorithms: vec![
                // Minimum required set to implement.
                SignatureScheme::ecdsa_secp256r1_sha256,
                SignatureScheme::rsa_pkcs1_sha256,
                SignatureScheme::rsa_pss_rsae_sha256,
            ],
        };

        let key_share = KeyShareClientHello {
            client_shares: vec![client_share],
        };

        Ok(Self {
            legacy_version: TLS_1_2_VERSION,
            random: Bytes::from(random_buf.to_vec()),
            legacy_session_id: Bytes::new(),
            cipher_suites: vec![
                CipherSuite::TLS_AES_128_GCM_SHA256,
                CipherSuite::TLS_AES_256_GCM_SHA384,
                CipherSuite::TLS_CHACHA20_POLY1305_SHA256,
            ],
            legacy_compression_methods: Bytes::from((&[0]).to_vec()),
            extensions: vec![
                Extension::ServerName(server_name),
                Extension::SupportedVersionsClientHello(supported_versions),
                Extension::SupportedGroups(supported_groups),
                Extension::SignatureAlgorithms(supported_algorithms),
                Extension::KeyShareClientHello(key_share),
            ],
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
				|v| Extension::parse(v, HandshakeType::client_hello)))(data)?;
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
                e.serialize(HandshakeType::client_hello, out).unwrap();
            }
        });
    }
}

#[derive(Debug)]
pub enum CipherSuite {
    TLS_AES_128_GCM_SHA256,
    TLS_AES_256_GCM_SHA384,
    TLS_CHACHA20_POLY1305_SHA256,
    TLS_AES_128_CCM_SHA256,
    TLS_AES_128_CCM_8_SHA256,
    Unknown(u16),
}
impl CipherSuite {
    fn to_u16(&self) -> u16 {
        use CipherSuite::*;
        match self {
            TLS_AES_128_GCM_SHA256 => 0x1301,
            TLS_AES_256_GCM_SHA384 => 0x1302,
            TLS_CHACHA20_POLY1305_SHA256 => 0x1303,
            TLS_AES_128_CCM_SHA256 => 0x1304,
            TLS_AES_128_CCM_8_SHA256 => 0x1305,
            Unknown(v) => *v,
        }
    }
    fn from_u16(v: u16) -> Self {
        use CipherSuite::*;
        match v {
            0x1301 => TLS_AES_128_GCM_SHA256,
            0x1302 => TLS_AES_256_GCM_SHA384,
            0x1303 => TLS_CHACHA20_POLY1305_SHA256,
            0x1304 => TLS_AES_128_CCM_SHA256,
            0x1305 => TLS_AES_128_CCM_8_SHA256,
            _ => Unknown(v),
        }
    }
    parser!(parse<Self> => {
        map(as_bytes(be_u16), |v| Self::from_u16(v))
    });
    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_u16().to_be_bytes());
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
    pub legacy_session_id_echo: Bytes,
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
				|i| Extension::parse(i, HandshakeType::server_hello)))(data)?;
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
                e.serialize(HandshakeType::server_hello, out).unwrap();
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
                    |i| Extension::parse(i, HandshakeType::encrypted_extensions)
                ))(data)?;
            Ok(Self { extensions })
        })
    });

    fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varlen_vector(0, U16_LIMIT, out, |out| {
            for e in self.extensions.iter() {
                e.serialize(HandshakeType::encrypted_extensions, out)
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
*/

#[derive(Debug)]
pub struct Certificate {
    pub certificate_request_context: Bytes,
    pub certificate_list: Vec<CertificateEntry>,
}

impl Certificate {
    parser!(parse<Self> => { seq!(c => {
		let certificate_request_context = c.next(varlen_vector(0, U8_LIMIT))?;
		let certificate_list = {
			let data = c.next(varlen_vector(0, U24_LIMIT))?;
			let (arr, _) = complete(many(CertificateEntry::parse))(data)?;
			arr
		};

		Ok(Self { certificate_request_context, certificate_list })
	}) });

    fn serialize(&self, out: &mut Vec<u8>) {
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
				|i| Extension::parse(i, HandshakeType::certificate)))(data)?;
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
                e.serialize(HandshakeType::certificate, out).unwrap();
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
					|i| Extension::parse(i, HandshakeType::certificate_request)
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
                e.serialize(HandshakeType::certificate_request, out)
                    .unwrap();
            }
        });
    }
}

////////////////////////////////////////////////////////////////////////////////

/*
struct {
    SignatureScheme algorithm;
    opaque signature<0..2^16-1>;
} CertificateVerify;
*/

#[derive(Debug)]
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
