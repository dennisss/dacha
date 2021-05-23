use common::bytes::{Bytes, BytesMut};
use common::errors::*;
use common::io::ReadWriteable;

use crate::tls::alert::*;
use crate::tls::record::*;
use crate::tls::cipher::*;
use crate::tls::handshake::*;
use crate::tls::transcript::Transcript;

#[derive(Debug)]
pub enum Message {
    ChangeCipherSpec(Bytes),
    Alert(Alert),
    Handshake(Handshake),
    /// Unencrypted data to go directly to the application.
    ApplicationData(Bytes),
}

pub struct HandshakeState {
    /// NOTE: This is updated internally in the RecordStream.
    pub transcript: Transcript,

    /// Bytes of a partial handshake message which haven't been able to be
    /// parse. This is used to support coalescing/splitting of handshake
    /// messages in one or more messages.
    ///
    /// TODO: If this is non-empty, then we can't change keys and receive other
    /// types of messages.
    handshake_buf: Bytes,
}

impl HandshakeState {
    pub fn new() -> Self {
        Self {
            handshake_buf: Bytes::new(),
            transcript: Transcript::new(),
        }
    }
}


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

impl RecordStream {
    pub fn new(channel: Box<dyn ReadWriteable>) -> Self {
        Self {
            channel,
            cipher_spec: None,
        }
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

    async fn recv_record(&mut self) -> Result<RecordInner> {
        // TODO: Eventually remove this loop once ChangeCipherSpec is handled
        // elsewhere.
        loop {
            let record = Record::read(self.channel.as_read()).await?;

            // TODO: Disallow zero length records unless using application data.

            // TODO: 'Handshake messages MUST NOT span key changes'

            // Only the ClientHello should be using a different record version.
            // TODO: Client hello retry should be allowed to have a different version.
            if record.legacy_record_version != TLS_1_2_VERSION {
                return Err(err_msg("Unexpected version"));
            }

            // TODO: Can't remove this until we have a better check for
            // enforcing that everything is encrypted.
            if record.typ == ContentType::ChangeCipherSpec {
                // TODO: After the ClientHello is sent and before a Finished is
                // received, this should be valid.

                // TODO: We should validate the contents of this message.
                continue;
            }

            let inner = if let Some(cipher_spec) = self.cipher_spec.as_ref() {
                // TODO: I know that at least application_data and keyupdates
                // must always be encrypted after getting keys.

                if record.typ != ContentType::ApplicationData {
                    return Err(err_msg("Expected only encrypted data not"));
                }

                let mut key_guard = cipher_spec.server_key.lock().await;
                let key = key_guard.keying.next_keys();

                // additional_data = TLSCiphertext.opaque_type ||
                //     TLSCiphertext.legacy_record_version ||
                //     TLSCiphertext.length
                // TODO: Implement this as a slice of the original record.
                let mut additional_data = vec![];
                record.typ.serialize(&mut additional_data);
                additional_data.extend_from_slice(&record.legacy_record_version.to_be_bytes());
                additional_data.extend_from_slice(&(record.data.len() as u16).to_be_bytes());

                let mut plaintext = vec![];
                cipher_spec.aead.decrypt(
                    &key.key,
                    &key.iv,
                    &record.data,
                    &additional_data,
                    &mut plaintext,
                )?;

                // The content type is the the last non-zero byte. All zeros
                // after that are padding and can be ignored.
                let mut content_type_res = None;
                for i in (0..plaintext.len()).rev() {
                    if plaintext[i] != 0 {
                        content_type_res = Some(i);
                        break;
                    }
                }

                let content_type_i = content_type_res.ok_or_else(|| err_msg("All zero"))?;

                let content_type = ContentType::from_u8(plaintext[content_type_i]);

                plaintext.truncate(content_type_i);

                RecordInner {
                    typ: content_type,
                    data: plaintext.into(),
                }
            } else {
                if record.typ == ContentType::ApplicationData {
                    return Err(err_msg("Received application_data without a cipher"));
                }

                RecordInner {
                    typ: record.typ,
                    data: record.data,
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

    async fn send_record(&mut self, inner: RecordInner) -> Result<()> {
        let record = if let Some(cipher_spec) = self.cipher_spec.as_ref() {
            // All encrypted records will be sent with an outer version of
            // TLS 1.2 for backwards compatibility.
            let legacy_record_version: u16 = 0x0303;

            let typ = ContentType::ApplicationData;

            // How much padding to add to each plaintext record.
            // TODO: Support padding up to a block size or accepting a callback
            // to configure this.
            let padding_size = 0;

            // Total expected size of cipher text. We need one byte at the end
            // for the content type.
            let total_size = cipher_spec.aead.expanded_size(inner.data.len() + 1) + padding_size;

            let mut additional_data = vec![];
            typ.serialize(&mut additional_data);
            additional_data.extend_from_slice(&legacy_record_version.to_be_bytes());
            additional_data.extend_from_slice(&(total_size as u16).to_be_bytes());

            let mut plaintext = vec![];
            plaintext.resize(inner.data.len() + 1 + padding_size, 0);
            plaintext[0..inner.data.len()].copy_from_slice(&inner.data);
            plaintext[inner.data.len()] = inner.typ.to_u8();

            // TODO: Make this condition depending on whether we are the client or server.
            let mut key_guard = cipher_spec.client_key.lock().await;

            let key = key_guard.keying.next_keys();

            let mut ciphertext = vec![];
            ciphertext.reserve(total_size);
            cipher_spec.aead.encrypt(
                &key.key,
                &key.iv,
                &plaintext,
                &additional_data,
                &mut ciphertext,
            );

            assert_eq!(ciphertext.len(), total_size);

            Record {
                legacy_record_version,
                typ,
                data: ciphertext.into(),
            }
        } else {
            if inner.typ == ContentType::ApplicationData {
                return Err(err_msg(
                    "Should not be sending unencrypted application data",
                ));
            }

            Record {
                // TODO: Implement this.
                // rfc8446: 'In order to maximize backward compatibility, a record containing an
                // initial ClientHello SHOULD have version 0x0301 (reflecting TLS 1.0) and a record
                // containing a second ClientHello or a ServerHello MUST have version 0x0303'
                legacy_record_version: 0x0301, // TLS 1.0
                typ: inner.typ,
                data: inner.data,
            }
        };

        let mut record_data = vec![];
        record.serialize(&mut record_data);

        self.channel.write_all(&record_data).await?;
        Ok(())
    }

    /// Recieves the next full message from the socket.
    /// In the case of handshake messages, this message may span multiple previous/future records.
    pub async fn recv(&mut self, mut handshake_state: Option<&mut HandshakeState>) -> Result<Message> {
        // Partial data received for a handshake message. Handshakes may be
        // split between consecutive records.
        let mut handshake_buf = BytesMut::new();

        let mut previous_handshake_record = None;
        if let Some(state) = handshake_state.as_mut() {
            if state.handshake_buf.len() > 0 {
                previous_handshake_record = Some(RecordInner {
                    typ: ContentType::Handshake,
                    data: state.handshake_buf.split_off(0),
                });
            }
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
                    let res = Handshake::parse(handshake_data.clone());

                    let (val, rest) = match res {
                        Ok(v) => v,
                        Err(e) => {
                            if parsing::is_incomplete(&e) {
                                handshake_buf = handshake_data.try_mut().unwrap();
                                continue;
                            } else {
                                // TODO: We received an invalid message, send an alert.
                                return Err(e);
                            }
                        }
                    };

                    let state = if let Some(s) = handshake_state {
                        s
                    } else {
                        return Err(err_msg("Not currently performing a handshake"));
                    };

                    // Append to transcript ignoring any padding
                    state
                        .transcript
                        .push(handshake_data.slice(0..(handshake_data.len() - rest.len())));

                    state.handshake_buf = rest;

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

    // TODO: Messages that are too long may need to be split up.
    pub async fn send_handshake(
        &mut self,
        msg: Handshake,
        state: &mut HandshakeState,
    ) -> Result<()> {
        let mut data = vec![];
        msg.serialize(&mut data);
        let buf = Bytes::from(data);

        state.transcript.push(buf.clone());

        self.send_record(RecordInner {
            typ: ContentType::Handshake,
            data: buf,
        })
        .await?;
        Ok(())
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
        self.channel.flush().await
    }
}