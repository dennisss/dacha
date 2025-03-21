use alloc::string::String;
use alloc::vec::Vec;

use asn::encoding::{der_eq, DERReadable, DERReader, DERWriteable};
use common::bytes::Bytes;
use common::errors::*;
use pkix::{PKIX1Explicit88, PKIX1Implicit88, PKCS_10};

use crate::pem::{PEMBuilder, PEM_CERTIFICATE_REQUEST_LABEL};
use crate::x509::{PublicKey, SignatureKeyConstraints};

use crate::x509::certificate::CertificateExtensions;

pub struct CertificateRequest {
    raw: PKCS_10::CertificationRequest,
    extensions: CertificateExtensions,
}

impl CertificateRequest {
    pub fn new(raw: PKCS_10::CertificationRequest) -> Result<Self> {
        let mut extensions = CertificateExtensions::from(&[])?;
        for attr in &raw.certificationRequestInfo.attributes.items {
            if attr.typ.as_ref() != &pkix::PKCS_9::PKCS_9_AT_EXTENSIONREQUEST {
                continue;
            }

            if attr.values.items.len() != 1 {
                return Err(err_msg(
                    "Expected extension request to have exactly one value",
                ));
            }

            let extension_req =
                attr.values.items[0].parse_as::<pkix::PKCS_9::ExtensionRequest>()?;

            extensions = CertificateExtensions::from(&extension_req.items)?;
            break;
        }

        Ok(Self { raw, extensions })
    }

    /// Reads a certficate request from DER encoded data.
    pub fn from_der(buf: Bytes) -> Result<Self> {
        // TODO: Ensure the buffer is read till completion.
        let mut r = DERReader::new(buf);
        let raw = PKCS_10::CertificationRequest::read_der(&mut r)?;
        Self::new(raw)
    }

    pub fn to_der(&self) -> Vec<u8> {
        self.raw.to_der()
    }

    pub fn raw(&self) -> &PKCS_10::CertificationRequest {
        &self.raw
    }

    pub fn to_pem(&self) -> String {
        let data = self.raw.to_der();

        PEMBuilder::default()
            .add_binary_entry(PEM_CERTIFICATE_REQUEST_LABEL, &data)
            .build()
    }

    /// Expects the subject to consist of exactly one field which is a common
    /// name.
    ///
    /// Returns the common name if it is, else, returns None.
    pub fn subject_as_common_name(&self) -> Result<Option<String>> {
        // TODO: Verify that there is only one name.

        let seq = match &self.raw.certificationRequestInfo.subject {
            pkix::PKIX1Explicit88::Name::rdnSequence(seq) => seq,
        };

        if seq.items.len() != 1 || seq.items[0].items.len() != 1 {
            return Ok(None);
        }

        let item = &seq.items[0].items[0];

        if !der_eq(&item.typ, &PKIX1Explicit88::ID_AT_COMMONNAME) {
            return Ok(None);
        }

        let cn = item.value.parse_as::<PKIX1Explicit88::X520CommonName>()?;

        let s = match &cn {
            PKIX1Explicit88::X520CommonName::teletexString(v) => v.as_str(),
            PKIX1Explicit88::X520CommonName::printableString(v) => v.as_str(),
            PKIX1Explicit88::X520CommonName::universalString(v) => v.as_str(),
            PKIX1Explicit88::X520CommonName::utf8String(v) => v.as_str(),
            PKIX1Explicit88::X520CommonName::bmpString(v) => v.as_str(),
        };

        Ok(Some(s.into()))
    }

    pub fn subject_alt_name(&self) -> Result<Option<PKIX1Implicit88::SubjectAltName>> {
        self.extensions
            .get_as(&PKIX1Implicit88::ID_CE_SUBJECTALTNAME)
    }

    pub fn public_key(&self) -> Result<PublicKey> {
        PublicKey::from_asn1(&self.raw.certificationRequestInfo.subjectPKInfo, None)
    }

    pub fn verify_signature(&self) -> Result<bool> {
        let public_key = self.public_key()?;
        let plaintext = self.raw.certificationRequestInfo.to_der();

        let signature = self.raw.signature.as_ref();
        if self.raw.signature.len() % 8 != 0 {
            return Err(err_msg("Expected signature to have a multiple of 8 bits"));
        }

        public_key.verify_signature(
            &plaintext,
            signature,
            &self.raw.signatureAlgorithm,
            &SignatureKeyConstraints::default(),
        )
    }
}
