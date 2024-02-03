use alloc::boxed::Box;

use common::bytes::{Bytes, BytesMut};
use common::errors::*;
use common::io::{SharedReadable, SharedWriteable, Writeable};

use crate::tls::alert::*;
use crate::tls::handshake::*;
use crate::tls::record::*;
use crate::tls::transcript::Transcript;

use super::cipher::CipherEndpointSpec;

// TODO: Also include information on the hinted legacy version.
#[derive(Debug)]
pub enum Message {
    ChangeCipherSpec(Bytes),
    Alert(Alert),
    Handshake(Handshake),
    /// Unencrypted data to go directed to the application.
    ApplicationData(Bytes),
}

// We can just have atomic counters to deal with key updates and avoid half
// locking TODO: If we do have a split interface, then we need to ensure that if
// there is a fatal processing error on only one half, then we close both the
// reader and writer halves  of the connection.

pub struct RecordReader {
    reader: Box<dyn SharedReadable>,

    is_server: bool,

    received_first_record: bool,

    /// TODO: Make private.
    pub protocol_version: ProtocolVersion,

    /// Cipher parameters used by the remote endpoint to encrypt records.
    /// Initially this is empty meaning that no encryption is expected.
    /// This should always be set after the handshake is complete.
    remote_cipher_spec: Option<CipherEndpointSpec>,

    /// Bytes of a partial handshake message which haven't been able to be
    /// parse. This is used to support coalescing/splitting of handshake
    /// messages in one or more messages.
    ///
    /// TODO: If this is non-empty, then we can't change keys and receive other
    /// types of messages.
    handshake_buffer: Bytes,
}

impl RecordReader {
    pub fn new(reader: Box<dyn SharedReadable>, is_server: bool) -> Self {
        Self {
            reader,
            is_server,
            received_first_record: false,
            remote_cipher_spec: None,
            protocol_version: TLS_1_0_VERSION,
            handshake_buffer: Bytes::new(),
        }
    }

    pub fn set_remote_cipher_spec(&mut self, remote_cipher_spec: CipherEndpointSpec) -> Result<()> {
        if !self.handshake_buffer.is_empty() {
            return Err(err_msg("Key change across a partial handshake message"));
        }

        self.remote_cipher_spec = Some(remote_cipher_spec);
        Ok(())
    }

    /// Assuming we have configured a TLS 1.3 cipher, this will change the
    /// cipher to a new traffic secret.
    pub fn replace_remote_key(&mut self, traffic_secret: Bytes) -> Result<()> {
        match self.remote_cipher_spec.as_mut() {
            Some(CipherEndpointSpec::TLS13(cipher_spec)) => {
                cipher_spec.replace_key(traffic_secret);
                Ok(())
            }
            Some(_) => Err(err_msg("No using TLS 1.3")),
            None => Err(err_msg("Cipher spec not set yet")),
        }
    }

    /// Recieves the next full message from the socket.
    /// In the case of handshake messages, this message may span multiple
    /// previous/future records.
    ///
    /// During the initial handshake, the 'transcript' can also be passed and
    /// the reader will append the raw bytes of any handshake message
    /// received.
    pub async fn recv(&mut self, transcript: Option<&mut Transcript>) -> Result<Message> {
        // Partial data received for a handshake message. Handshakes may be
        // split between consecutive records.
        let mut handshake_buf = BytesMut::new();

        let mut previous_handshake_record = None;
        if self.handshake_buffer.len() > 0 {
            previous_handshake_record = Some(Record {
                legacy_record_version: 0, // Not currently used.
                typ: ContentType::Handshake,
                data: self.handshake_buffer.split_off(0),
            });
        }

        loop {
            let record = if let Some(r) = previous_handshake_record.take() {
                r
            } else {
                self.recv_record().await?
            };

            if handshake_buf.len() != 0 && record.typ != ContentType::Handshake {
                return Err(err_msg("Data interleaved in handshake"));
            }

            let (val, rest) = match record.typ {
                ContentType::Handshake => {
                    let handshake_data = if handshake_buf.len() > 0 {
                        let mut data_mut = handshake_buf;
                        data_mut.extend_from_slice(&record.data);
                        data_mut.freeze()
                    } else {
                        record.data
                    };

                    let res = Handshake::parse(handshake_data.clone(), self.protocol_version);

                    let (val, rest) = match res {
                        Ok(v) => v,
                        Err(e) => {
                            if parsing::is_incomplete(&e) {
                                handshake_buf = handshake_data.try_mut().unwrap();
                                continue;
                            } else {
                                // TODO: We received an invalid message, send an alert.
                                return Err(format_err!(
                                    "While parsing TLS handhake message: {}",
                                    e
                                ));
                            }
                        }
                    };

                    // Append to transcript ignoring any padding
                    if let Some(transcript) = transcript {
                        transcript
                            .push(handshake_data.slice(0..(handshake_data.len() - rest.len())));
                    }

                    self.handshake_buffer = rest;

                    (Message::Handshake(val), Bytes::new())
                }
                ContentType::Alert => {
                    Alert::parse(record.data).map(|(a, r)| (Message::Alert(a), r))?
                }
                ContentType::ApplicationData => {
                    (Message::ApplicationData(record.data), Bytes::new())
                }
                _ => {
                    return Err(format_err!("Unknown record type {:?}", record.typ));
                }
            };

            if rest.len() != 0 {
                return Err(err_msg("Unexpected data after message"));
            }

            return Ok(val);
        }
    }

    async fn recv_record(&mut self) -> Result<Record> {
        // TODO: In TLS 1.2, we should be able to receive un-encrypted alert messages
        // before the ChangeCipherSpec is received. (also need to support this when
        // sending stuff).

        // TODO: Eventually remove this loop once ChangeCipherSpec is handled
        // elsewhere.
        loop {
            let record = Record::read(self.reader.as_mut()).await?;

            // TODO: Disallow zero length records unless using application data.

            // TODO: 'Handshake messages MUST NOT span key changes'

            // We only support TLS 1.2 and 1.3.
            // Only the first ClientHello should have a backwards compat version of 1.0 and
            // all following packets should use 1.2.
            let expected_version = {
                if self.is_server && !self.received_first_record {
                    TLS_1_0_VERSION
                } else {
                    TLS_1_2_VERSION
                }
            };

            if expected_version != record.legacy_record_version {
                return Err(err_msg("Unexpected version"));
            }

            self.received_first_record = true;

            // TODO: Can't remove this until we have a better check for
            // enforcing that everything is encrypted.
            if record.typ == ContentType::ChangeCipherSpec {
                // TODO: After the ClientHello is sent and before a Finished is
                // received, this should be valid.

                // TODO: We should validate the contents of this message.
                continue;
            }

            let inner = match self.remote_cipher_spec.as_mut() {
                Some(CipherEndpointSpec::TLS13(cipher_spec)) => {
                    // TODO: I know that at least application_data and keyupdates
                    // must always be encrypted after getting keys.

                    cipher_spec.decrypt(record)?
                }
                Some(CipherEndpointSpec::TLS12(cipher_spec)) => cipher_spec.decrypt(record)?,
                None => {
                    if record.typ == ContentType::ApplicationData {
                        return Err(err_msg("Received application_data without a cipher"));
                    }

                    record
                }
            };

            // Empty records are only allowed for
            // TODO: Does this apply to anything other than Handshake?
            // if inner.typ != ContentType::application_data && inner.data.len() == 0 {
            // 	return Err(err_msg("Empty record not allowed"));
            // }

            return Ok(inner);
        }
    }
}

pub struct RecordWriter {
    writer: Box<dyn SharedWriteable>,

    is_server: bool,

    sent_first_record: bool,

    pub local_cipher_spec: Option<CipherEndpointSpec>,
}

impl RecordWriter {
    pub fn new(writer: Box<dyn SharedWriteable>, is_server: bool) -> Self {
        Self {
            writer,
            is_server,
            sent_first_record: false,
            local_cipher_spec: None,
        }
    }

    pub fn replace_local_key(&mut self, traffic_secret: Bytes) -> Result<()> {
        match self.local_cipher_spec.as_mut() {
            Some(CipherEndpointSpec::TLS13(cipher_spec)) => {
                cipher_spec.replace_key(traffic_secret);
                Ok(())
            }
            Some(_) => Err(err_msg("No using TLS 1.3")),
            None => Err(err_msg("Cipher spec not set yet")),
        }
    }

    // TODO: Messages that are too long may need to be split up.
    pub async fn send_handshake(
        &mut self,
        msg: Handshake,
        transcript: Option<&mut Transcript>,
    ) -> Result<()> {
        let mut data = vec![];
        msg.serialize(&mut data);
        let buf = Bytes::from(data);

        if let Some(transcript) = transcript {
            transcript.push(buf.clone());
        }

        self.send_record(RecordInner {
            typ: ContentType::Handshake,
            data: buf,
        })
        .await?;
        Ok(())
    }

    pub async fn send_change_cipher_spec(&mut self) -> Result<()> {
        self.send_record(RecordInner {
            data: vec![1].into(),
            typ: ContentType::ChangeCipherSpec,
        })
        .await
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        // TODO: Avoid the clone in converting to a Bytes
        self.send_record(RecordInner {
            data: data.into(),
            typ: ContentType::ApplicationData,
        })
        .await
    }

    pub async fn flush(&mut self) -> Result<()> {
        self.writer.flush().await
    }

    // TODO: Instead functions should return a Result indicating the Alert
    // they want to convey back to the server.
    async fn fatal(&mut self, description: AlertDescription) -> Result<()> {
        let alert = Alert {
            level: AlertLevel::fatal,
            description,
        };
        let mut data = vec![];
        alert.serialize(&mut data);

        let record = RecordInner {
            typ: ContentType::Alert,
            data: data.into(),
        };

        self.send_record(record).await
    }

    async fn send_record(&mut self, inner: RecordInner) -> Result<()> {
        // All encrypted records will be sent with an outer version of
        // TLS 1.2 for backwards compatibility.
        let legacy_record_version = {
            // rfc8446: 'In order to maximize backward compatibility, a record containing an
            // initial ClientHello SHOULD have version 0x0301 (reflecting TLS 1.0) and a
            // record containing a second ClientHello or a ServerHello MUST have
            // version 0x0303'
            if !self.is_server && !self.sent_first_record {
                TLS_1_0_VERSION
            } else {
                TLS_1_2_VERSION
            }
        };

        let inner = Record {
            legacy_record_version,
            typ: inner.typ,
            data: inner.data,
        };

        let record = match self.local_cipher_spec.as_mut() {
            Some(CipherEndpointSpec::TLS13(cipher_spec)) => cipher_spec.encrypt(inner),
            Some(CipherEndpointSpec::TLS12(cipher_spec)) => cipher_spec.encrypt(inner),
            None => {
                if inner.typ == ContentType::ApplicationData {
                    return Err(err_msg(
                        "Should not be sending unencrypted application data",
                    ));
                }

                inner
            }
        };

        self.sent_first_record = true;

        let mut record_data = vec![];
        record.serialize(&mut record_data);

        self.writer.write_all(&record_data).await?;
        Ok(())
    }
}

/*

/// Interface for sending and receiving TLS Records over the raw connection.
///
/// This interface also handles appropriately encrypting/decrypting records once
/// keys have been negotiated.
pub struct RecordStream {
    /// The underlying byte based transport layer used for sending/recieving Records.
    channel: Box<dyn ReadWriteable>,

    /// Current encryption configuration for the connection.
    ///
    /// Initially this will None indicating that we haven't yet agreed upon keys and
    /// will eventually
    ///  If specified then the connection is encrypted.
    ///
    /// TODO: Make this private.
    pub cipher_spec: Option<CipherSpec>,
}

*/
