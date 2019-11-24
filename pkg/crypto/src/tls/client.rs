use super::record::*;
use super::handshake::*;
use super::alert::*;
use super::extensions::*;
use parsing::is_incomplete;
use async_std::net::TcpStream;
use async_std::prelude::*;
use common::errors::*;
use crate::hasher::*;
use crate::sha256::SHA256Hasher;
use crate::sha384::SHA384Hasher;
use crate::hkdf::*;
use super::key_schedule::*;
use crate::dh::*;
use crate::elliptic::*;
use crate::aead::*;
use crate::gcm::*;
use std::collections::VecDeque;
use super::transcript::*;
use common::io::*;
use async_std::sync::Mutex;

use bytes::{BytesMut, Bytes, Buf};

// TODO: Should abort the connection if negotiation results in more than one
// retry as the first retry should always have enough information.

enum State {
	Start,
	WaitServerHello,
	WaitEncryptedExtensions,
	WaitCertificate,
	WaitCertificateVerify,
	Wait
}

#[derive(Debug)]
enum Message {
	ChangeCipherSpec,
	Alert(Alert),
	Handshake(Handshake),
	/// Unencrypted data to go directly to the application.
	ApplicationData(Bytes)
}

/*
	We should implement a TLSStream which optionally can 
*/

pub struct Stream {
	stream: Box<dyn ReadWriteable>,

	// If specified then the connection is encrypted.
	cipher_spec: Option<CipherSpec>,

	read_buffer: Mutex<Bytes>,
}

impl Stream {
	pub(crate) fn from(stream: Box<dyn ReadWriteable>) -> Self {
		Self {
			stream,
			cipher_spec: None,
			read_buffer: Mutex::new(Bytes::new())
		}
	}

	// TODO: Instead functions should return a Result indicating the Alert
	// they want to convey back to the server.
	async fn fatal(&mut self, description: AlertDescription) -> Result<()> {
		let alert = Alert { level: AlertLevel::fatal, description };
		let mut data = vec![];
		alert.serialize(&mut data);

		let record = RecordInner {
			typ: ContentType::alert,
			data: data.into()
		};

		self.send_record(record).await
	}

	async fn recv_record(&self) -> Result<RecordInner> {
		// TODO: Eventually remove this loop once change_cipher_spec is handled
		// elsewhere.
		loop {
			let record = Record::read(self.stream.as_read()).await?;

			// TODO: Disallow zero length records unless using application data.

			// Only the ClientHello should be using a different record version.
			if record.legacy_record_version != TLS_1_2_VERSION {
				return Err("Unexpected version".into());
			}

			// TODO: Can't remove this until we have a better check for
			// enforcing that everything is encrypted.
			if record.typ == ContentType::change_cipher_spec {
				// TODO: After the ClientHello is sent and before a Finished is
				// received, this should be valid.
				continue;
			}

			let inner = if let Some(cipher_spec) = self.cipher_spec.as_ref() {
				// TODO: I know that at least application_data and keyupdates
				// must always be encrypted after getting keys.

				if record.typ != ContentType::application_data {
					return Err("Expected only encrypted data not".into());
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
					&key.key, &key.iv, &record.data, &additional_data, &mut plaintext)?;

				// The content type is the the last non-zero byte. All zeros
				// after that are padding and can be ignored.
				let mut content_type_res = None;
				for i in (0..plaintext.len()).rev() {
					if plaintext[i] != 0 {
						content_type_res = Some(i);
						break;
					}
				}

				let content_type_i = content_type_res.ok_or(
					Error::from("All zero"))?;

				let content_type = ContentType::from_u8(plaintext[content_type_i]);

				plaintext.truncate(content_type_i);

				RecordInner { typ: content_type, data: plaintext.into() }
			} else {
				if record.typ == ContentType::application_data {
					return Err("Received application_data without a cipher".into());
				}

				RecordInner { typ: record.typ, data: record.data  }
			};

			// Empty records are only allowed for
			// TODO: Does this apply to anything other than Handshake?
			// if inner.typ != ContentType::application_data && inner.data.len() == 0 {
			// 	return Err("Empty record not allowed".into());
			// }

			return Ok(inner)
		}
	}

	async fn send_record(&self, inner: RecordInner) -> Result<()> {
		let record = if let Some(cipher_spec) = self.cipher_spec.as_ref() {

			// All encrypted records will be sent with an outer version of
			// TLS 1.2 for backwards compatibility.
			let legacy_record_version: u16 = 0x0303;

			let typ = ContentType::application_data;

			// How much padding to add to each plaintext record.
			// TODO: Support padding up to a block size or accepting a callback
			// to configure this.
			let padding_size = 0;

			// Total expected size of cipher text. We need one byte at the end
			// for the content type.
			let total_size = cipher_spec.aead.expanded_size(
				inner.data.len() + 1) + padding_size;

			let mut additional_data = vec![];
			typ.serialize(&mut additional_data);
			additional_data.extend_from_slice(&legacy_record_version.to_be_bytes());
			additional_data.extend_from_slice(&(total_size as u16).to_be_bytes());

			let mut plaintext = vec![];
			plaintext.resize(inner.data.len() + 1 + padding_size, 0);
			plaintext[0..inner.data.len()].copy_from_slice(&inner.data);
			plaintext[inner.data.len()] = inner.typ.to_u8();

			let mut key_guard = cipher_spec.client_key.lock().await;
			let key = key_guard.keying.next_keys();

			let mut ciphertext = vec![];
			ciphertext.reserve(total_size);
			cipher_spec.aead.encrypt(
				&key.key, &key.iv, &plaintext, &additional_data,
				&mut ciphertext);

			assert_eq!(ciphertext.len(), total_size);

			Record {
				legacy_record_version, typ, data: ciphertext.into()
			}
		} else {
			if inner.typ == ContentType::application_data {
				return Err(
					"Should not be sending unencrypted application data"
						.into());
			}

			Record {
				// TODO: Implement this.
				// rfc8446: 'In order to maximize backward compatibility, a record containing an initial ClientHello SHOULD have version 0x0301 (reflecting TLS 1.0) and a record containing a second ClientHello or a ServerHello MUST have version 0x0303'
				legacy_record_version: 0x0301, // TLS 1.0
				typ: inner.typ,
				data: inner.data
			}
		};

		let mut record_data = vec![];
		record.serialize(&mut record_data);

		self.stream.write_all(&record_data).await?;
		Ok(())
	}

	/// Recieves the next full message from the socket.
	async fn recv(&self, mut handshake_state: Option<&mut HandshakeState>)
		-> Result<Message> {

		// Partial data received for a handshake message. Handshakes may be
		// split between consecutive records.
		let mut handshake_buf = BytesMut::new();

		let mut previous_handshake_record = None;
		if let Some(state) = handshake_state.as_mut() {
			if state.handshake_buf.len() > 0 {
				previous_handshake_record = Some(RecordInner {
					typ: ContentType::handshake,
					data: state.handshake_buf.split_off(0)
				});
			}
		}

		loop {
			let record =
				if let Some(r) = previous_handshake_record.take() {
					r
				} else {
					self.recv_record().await?
				};

			if handshake_buf.len() != 0 &&
				record.typ != ContentType::handshake {
				return Err("Data interleaved in handshake".into());
			}

			let (val, rest) = match record.typ {
				ContentType::handshake => {
					let handshake_data =
						if handshake_buf.len() > 0 {
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
							if is_incomplete(&e) {
								handshake_buf = handshake_data.try_mut().unwrap();
								continue;
							} else {
								// TODO: We received an invalid message, send an alert.
								return Err(e);
							}
						}
					};

					let state =
						if let Some(s) = handshake_state { s } else {
							return Err("Not currently performing a handshake".into());
						};

					// Append to transcript ignoring any padding
					state.transcript.push(handshake_data.slice(
						0..(handshake_data.len() - rest.len())
					));

					state.handshake_buf = rest;

					(Message::Handshake(val), Bytes::new())
				},
				ContentType::alert => {
					Alert::parse(record.data).map(
						|(a, r)| (Message::Alert(a), r))?
				},
				ContentType::application_data => {
					(Message::ApplicationData(record.data), Bytes::new())
				},
				_ => { return Err(
					format!("Unknown record type {:?}", record.typ).into()); }
			};

			if rest.len() != 0 {
				return Err("Unexpected data after message".into());
			}

			return Ok(val);
		}
	}

	// TODO: Messages that are too long may need to be split up.
	pub async fn send_handshake(&mut self, msg: Handshake,
								state: &mut HandshakeState) -> Result<()> {
		let mut data = vec![];
		msg.serialize(&mut data);
		let buf = Bytes::from(data);

		state.transcript.push(buf.clone());

		self.send_record(
			RecordInner { typ: ContentType::handshake, data: buf }).await?;
		Ok(())
	}

	pub async fn send(&self, data: &[u8]) -> Result<()> {
		// TODO: Avoid the clone in converting to a Bytes
		self.send_record(RecordInner {
			data: data.into(),
			typ: ContentType::application_data }).await
	}
}

#[async_trait]
impl Readable for Stream {
	async fn read(&self, mut buf: &mut [u8]) -> Result<usize> {
		// TODO: We should dedup this with the http::Body code.
		let mut read_buffer = self.read_buffer.lock().await;
		let mut nread = 0;
		if read_buffer.len() > 0 {
			let n = std::cmp::min(buf.len(), read_buffer.len());
			buf[0..n].copy_from_slice(&read_buffer[0..n]);
			read_buffer.advance(n);
			buf = &mut buf[n..];
			nread += n;
		}

		if buf.len() == 0 {
			return Ok(nread);
		}

		let msg = self.recv(None).await?;
		if let Message::ApplicationData(mut data) = msg {
			let n = std::cmp::min(data.len(), buf.len());
			buf[0..n].copy_from_slice(&data[0..n]);
			nread += n;
			data.advance(n);

			*read_buffer = data;

			Ok(nread)
		} else {
			Err("Unexpected data seen on stream".into())
		}
	}
}

#[async_trait]
impl Writeable for Stream {
	async fn write(&self, buf: &[u8]) -> Result<usize> {
		// TODO: We may need to split up a packet that is too large.
		self.send(buf).await?;
		Ok(buf.len())
	}

	async fn flush(&self) -> Result<()> {
		self.stream.flush().await?;
		Ok(())
	}
}


pub struct HandshakeState {
	transcript: Transcript,

	/// Bytes of a partial handshake message which haven't been able to be
	/// parse. This is used to support coalescing/splitting of handshake
	/// messages in one or more messages.
	///
	/// TODO: If this is non-empty, then we can't change keys and receive other
	/// types of messages.
	handshake_buf: Bytes,
}

impl HandshakeState {
	fn new() -> Self {
		Self {
			handshake_buf: Bytes::new(),
			transcript: Transcript::new()
		}
	}
}


struct CipherSpec {
	aead: Box<dyn AuthEncAD>,
	hkdf: HKDF,

	client_key: Mutex<CipherEndpointKey>,
	server_key: Mutex<CipherEndpointKey>
}

impl CipherSpec {
	pub fn from_keys(aead: Box<dyn AuthEncAD>, hkdf: HKDF,
					 client_traffic_secret: Bytes,
					 server_traffic_secret: Bytes) -> Self {
		// TODO: This is very redundant with replace_keys
		let client_key = Mutex::new(CipherEndpointKey::from_key(
			aead.as_ref(), &hkdf, client_traffic_secret));
		let server_key = Mutex::new(CipherEndpointKey::from_key(
			aead.as_ref(), &hkdf, server_traffic_secret));

		Self {
			aead, hkdf, client_key, server_key
		}
	}

	// TODO: If there are multiple readers, then this must always occur
	// during the same locking cycle as the
	// TODO: Only valid for application keys and not for handshake keys.


	pub async fn replace_keys(&mut self, client_traffic_secret: Bytes,
						server_traffic_secret: Bytes) {
		*self.client_key.lock().await = CipherEndpointKey::from_key(
			self.aead.as_ref(), &self.hkdf, client_traffic_secret);
		*self.server_key.lock().await = CipherEndpointKey::from_key(
			self.aead.as_ref(), &self.hkdf, server_traffic_secret);
	}
}

pub struct CipherEndpointKey {
	traffic_secret: Bytes,
	/// Derived from the above key.
	keying: TrafficKeyingMaterial
}

impl CipherEndpointKey {
	fn from_key(aead: &dyn AuthEncAD, hkdf: &HKDF,
				traffic_secret: Bytes) -> Self {
		let keying = TrafficKeyingMaterial::from_secret(
			hkdf, aead, &traffic_secret);
		Self {
			traffic_secret, keying
		}
	}

	// NOTE: This must be called under the same lock that sent/received the key
	// change request to ensure no other messages are received/send under the
	// old keys.
	//
	// application_traffic_secret_N+1 =
	//        HKDF-Expand-Label(application_traffic_secret_N,
	//                          "traffic upd", "", Hash.length)
	fn update_key(&mut self, aead: &dyn AuthEncAD, hkdf: &HKDF) {
		let next_secret = hkdf_expand_label(
			&hkdf, &self.traffic_secret, b"traffic upd", b"",
			hkdf.hash_size() as u16).into();

		*self = Self::from_key(aead, hkdf, next_secret);
	}
}


pub struct Client {

	// hasher

	// diffie helman private key

	// transcript of handshake messages (do i also need alerts?)

	// pending partially updated

	// cookie (to be validated and passed in the next )

	// /// Messages that have been received but haven't yet been processed.
	// pending_messages: VecDeque<Message>
	
}





fn find_supported_versions_sh(extensions: &Vec<Extension>)
-> Option<&SupportedVersionsServerHello> {
	for e in extensions {
		if let Extension::SupportedVersionsServerHello(v) = e {
			return Some(v);
		}
	}

	None
}

fn find_key_share_sh(extensions: &Vec<Extension>)
-> Option<&KeyShareServerHello> {
	for e in extensions {
		if let Extension::KeyShareServerHello(v) = e {
			return Some(v);
		}
	}

	None
}

// TODO: Must implement bubbling up Alert messages

// TODO: Unused?
const TLS13_CERTIFICATEVERIFY_CLIENT_CTX: &'static [u8] =
	b"TLS 1.3, client CertificateVerify";
const TLS13_CERTIFICATEVERIFY_SERVER_CTX: &'static [u8] =
	b"TLS 1.3, server CertificateVerify";

impl Client {

	pub fn new() -> Self {
		Self {}
	}

	pub async fn connect(&mut self, input: Box<dyn ReadWriteable>,
						 hostname: &str) -> Result<Stream> {
		let mut stream = Stream::from(input);

		let mut handshake_state = HandshakeState::new();

		let group = MontgomeryCurveGroup::x25519();
		let secret_value = group.secret_value().await?;
		let client_share = KeyShareEntry {
			group: NamedGroup::x25519,
			key_exchange: group.public_value(&secret_value)?.into()
		};

		let client_hello = Handshake::ClientHello(
			ClientHello::plain(client_share).await?);
		stream.send_handshake(client_hello, &mut handshake_state).await?;

		// TODO: Use buffered reads.
		// let mut res_data = vec![];
		// res_data.resize(512, 0);
		// let n = stream.read(&mut res_data).await?;

		// Receive ServerHello
		// let mut handshake_buf = vec![];

		let msg = stream.recv(Some(&mut handshake_state)).await?;
		// TODO: First handle all alerts.

		let server_hello =
			if let Message::Handshake(Handshake::ServerHello(sh)) = msg { sh }
			else { return Err("Unexpected message".into()); };

		// Check that the version is TLS 1.2
		// Then look for a SupportedVersions extension to see if it is TLS 1.3
		let is_tls13 = server_hello.legacy_version == TLS_1_2_VERSION &&
			find_supported_versions_sh(&server_hello.extensions).map(|sv| {
				sv.selected_version == TLS_1_3_VERSION
			}).unwrap_or(false);
		if !is_tls13 {
			return Err("Only support TLS 1.3".into());
		}

		// TODO: Must match ClientHello?
		if server_hello.legacy_compression_method != 0 {
			return Err("Unexpected compression method".into());
		}

		// TODO: Must check the random bytes received.

		// Derive the key_share

		let cipher_suite = server_hello.cipher_suite;

		// TODO: Validate it was one of the ones we asked for.
		let (aead, hasher_factory) = match cipher_suite {
			CipherSuite::TLS_AES_128_GCM_SHA256 => {
				(Box::new(AES_GCM::aes128()), SHA256Hasher::factory())
			},
			 CipherSuite::TLS_AES_256_GCM_SHA384 => {
			 	(Box::new(AES_GCM::aes256()), SHA384Hasher::factory())
			 },
			// CipherSuite::TLS_CHACHA20_POLY1305_SHA256 => {
			// 	SHA256Hasher::factory()
			// },
			_ => { return Err("Bad cipher suite".into()); }
		};

		let hkdf = HKDF::new(hasher_factory.box_clone());

		let mut key_schedule = KeySchedule::new(hkdf.clone(),
												hasher_factory.box_clone());

		key_schedule.early_secret(None);

		let server_public = find_key_share_sh(&server_hello.extensions)
			.ok_or(Error::from("ServerHello missing key_share"))?;

		// Must match what was given in our ClientHello
		assert_eq!(server_public.server_share.group, NamedGroup::x25519);

		let shared_secret = group.shared_secret(
			&server_public.server_share.key_exchange, &secret_value)?;

		key_schedule.handshake_secret(&shared_secret);

		let (client_handshake_traffic_secret,
			server_handshake_traffic_secret) = {
			let s = key_schedule.handshake_traffic_secrets(&handshake_state.transcript);
			(s.client_handshake_traffic_secret,
			 s.server_handshake_traffic_secret)
		};

		key_schedule.master_secret();

		stream.cipher_spec = Some(CipherSpec::from_keys(
			aead, hkdf.clone(),
			client_handshake_traffic_secret.into(),
			server_handshake_traffic_secret.into()));

		// Receive Encrypted Extensions
		
		let ee = stream.recv(Some(&mut handshake_state)).await?;

		let cert = match stream.recv(Some(&mut handshake_state)).await? {
			Message::Handshake(Handshake::Certificate((c))) => c,
			_ => { return Err("Expected certificate message".into()); }
		};

		if cert.certificate_request_context.len() != 0 {
			return Err("Unexpected request context width certificate".into());
		}

		let mut cert_list = vec![];
		for c in &cert.certificate_list {
			cert_list.push(std::sync::Arc::new(
				crate::x509::Certificate::read(c.cert.clone())?));
		}

		if cert_list.len() < 1 {
			return Err("Expected at least one certificate".into());
		}

		let mut registry = crate::x509::CertificateRegistry::public_roots()?;
		// This will verify that they are all legit.
		registry.append(&cert_list, false)?;

		if !cert_list[0].valid_now() {
			return Err("Certificate not valid now".into());
		}

		if let Some(usage) = cert_list[0].key_usage()? {
			if !usage.digitalSignature().unwrap_or(false) {
				return Err(
					"Certificate can't be used for signature verification".into());
			}
		}

		if !cert_list[0].for_dns_name(hostname)? {
			return Err("Certificate not valid for DNS name".into());
		}


		// Transcript hash for ClientHello through to the Certificate.
		let ch_ct_transcript_hash = handshake_state.transcript.hash(&hasher_factory);


		let cert_verify = match stream.recv(Some(&mut handshake_state)).await? {
			Message::Handshake(Handshake::CertificateVerify(c)) => c,
			_ => { return Err("Expected certificate verify".into()); }
		};

		let mut plaintext = vec![];
		for _ in 0..64 {
			plaintext.push(0x20);
		}
		plaintext.extend_from_slice(&TLS13_CERTIFICATEVERIFY_SERVER_CTX);
		plaintext.push(0);
		plaintext.extend_from_slice(&ch_ct_transcript_hash);

		// TODO: Verify this is an algorithm that we requested (and that it
		// matches all relevant params in the certificate.
		match cert_verify.algorithm {
			SignatureScheme::ecdsa_secp256r1_sha256 => {
				let (params, point) = cert_list[0].ec_public_key(&registry)?;
				let group = EllipticCurveGroup::secp256r1();

				let mut hasher = crate::sha256::SHA256Hasher::default();
				let good = group.verify_signature(point.as_ref(),
												  &cert_verify.signature,
												  &plaintext, &mut hasher)?;
				if !good {
					return Err("Invalid certificate verify signature".into());
				}
			},
			// TODO:
			// SignatureScheme::rsa_pkcs1_sha256,
			// SignatureScheme::rsa_pss_rsae_sha256
			_ => {
				return Err("Unsupported cert verify algorithm".into());
			}
		};

		let verify_data_server =
			key_schedule.verify_data_server(&handshake_state.transcript);

		let finished = match stream.recv(Some(&mut handshake_state)).await? {
			Message::Handshake(Handshake::Finished(v)) => v,
			_ => { return Err("Expected Finished messages".into()); }
		};

		if finished.verify_data != verify_data_server {
			return Err("Incorrect server verify_data".into());
		}

		let verify_data_client =
			key_schedule.verify_data_client(&handshake_state.transcript);

		let finished_client = Handshake::Finished(Finished {
			verify_data: verify_data_client
		});

		let final_secrets = key_schedule.finished(&handshake_state.transcript);

		stream.send_handshake(finished_client, &mut handshake_state).await?;

		stream.cipher_spec.as_mut().unwrap().replace_keys(
			final_secrets.client_application_traffic_secret_0,
			final_secrets.server_application_traffic_secret_0
		).await;

		Ok(stream)

		//////

		// TODO: Validate that all extensions have been interprated in some way.

		// If 1.3, check the selected group is one of the ones that we wanted


		// Assert that there is a key_share in the ServerHello

		// Decode the cipher suite in order to at least start using the right hash.

		// Generate the shared secret

		// AES AEAD stuff is here: https://tools.ietf.org/html/rfc5116
		// https://tools.ietf.org/html/rfc5288

		// ChaCha20 in here: https://tools.ietf.org/html/rfc8439

		// println!("RES: {:?}", &res_data[0..n]);

	}


}
