use std::vec::Vec;

use common::bytes::Bytes;
use common::errors::*;
use common::io::Readable;
use parsing::binary::*;
use parsing::*;

use crate::tls::parsing::*;

// 'Implementations MUST NOT send zero-length fragments of Handshake,
// Alert, or ChangeCipherSpec content types.  Zero-length fragments of
// Application data MAY be sent as they are potentially useful as a
// traffic analysis countermeasure.'

// See https://tools.ietf.org/html/rfc5246#section-7.1 for TLS 1.2 change cipher

/*
TLS 1.3

struct {
    ContentType type;
    ProtocolVersion legacy_record_version;
    uint16 length;
    opaque fragment[TLSPlaintext.length];
} TLSPlaintext;

struct {
    opaque content[TLSPlaintext.length];
    ContentType type;
    uint8 zeros[length_of_padding];
} TLSInnerPlaintext;

struct {
    ContentType opaque_type = application_data; /* 23 */
    ProtocolVersion legacy_record_version = 0x0303; /* TLS v1.2 */
    uint16 length;
    opaque encrypted_record[TLSCiphertext.length];
} TLSCiphertext;

TLS 1.2

struct {
    ContentType type;
    ProtocolVersion version;
    uint16 length;
    opaque fragment[TLSPlaintext.length];
} TLSPlaintext;

struct {
    ContentType type;       /* same as TLSPlaintext.type */
    ProtocolVersion version;/* same as TLSPlaintext.version */
    uint16 length;
    opaque fragment[TLSCompressed.length];
} TLSCompressed;

struct {
    ContentType type;
    ProtocolVersion version;
    uint16 length;
    select (SecurityParameters.cipher_type) {
        case stream: GenericStreamCipher;
        case block:  GenericBlockCipher;
        case aead:   GenericAEADCipher;
    } fragment;
} TLSCiphertext;

struct {
    opaque nonce_explicit[SecurityParameters.record_iv_length];
    aead-ciphered struct {
        opaque content[TLSCompressed.length];
    };
} GenericAEADCipher;

*/

// XXX: We can use a fixed size buffer:
// An AEAD algorithm used in TLS 1.3 MUST NOT produce an expansion
//    greater than 255 octets.

/// Outer most data type trasmitted on the wire.
#[derive(Debug)]
pub struct Record {
    pub typ: ContentType,
    // TODO: Consider propagating this out to the RecordInner so that outer code can check the
    // version.
    pub legacy_record_version: u16, // ProtocolVersion,
    // length: u16,
    /// If typ == application_data, then this is encrypted data.
    pub data: Bytes,
}

impl Record {
    pub async fn read(reader: &mut dyn Readable) -> Result<Record> {
        let mut buf = [0u8; 5];
        reader.read_exact(&mut buf).await?;

        let typ = ContentType::from_u8(buf[0]);
        let legacy_record_version = u16::from_be_bytes(*array_ref![buf, 1, 2]);
        let length = u16::from_be_bytes(*array_ref![buf, 3, 2]);

        if length > (exp2(14) + 256) as u16 {
            return Err(err_msg("alert: record_overflow"));
        }

        let mut data = vec![];
        data.resize(length as usize, 0);
        reader.read_exact(&mut data).await?;
        Ok(Record {
            typ,
            legacy_record_version,
            data: Bytes::from(data),
        })
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        out.push(self.typ.to_u8());
        out.extend_from_slice(&self.legacy_record_version.to_be_bytes());
        // TODO: Use varlen code
        assert!(self.data.len() < U16_LIMIT);
        out.extend_from_slice(&(self.data.len() as u16).to_be_bytes());
        out.extend_from_slice(&self.data);
    }
}

tls_enum_u8!(ContentType => {
    Invalid(0),
    ChangeCipherSpec(20),
    Alert(21),
    Handshake(22),
    ApplicationData(23),
    (255)
});

/// Inner record format used as the plain text data of encrypted TLS 1.3
/// records. This will always be sent inside an outer Record with ContentType
/// application_data.
#[derive(Debug)]
pub struct RecordInner {
    pub typ: ContentType,
    pub data: Bytes,
}
