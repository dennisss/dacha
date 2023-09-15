use alloc::string::String;

use asn::encoding::DERWriteable;
use common::errors::*;
use pkix::PKCS_10;

use crate::pem::{PEMBuilder, PEM_CERTIFICATE_REQUEST_LABEL};
use crate::x509::{PublicKey, SignatureKeyConstraints};

pub struct CertificateRequest {
    raw: PKCS_10::CertificationRequest,
}

impl CertificateRequest {
    pub fn new(raw: PKCS_10::CertificationRequest) -> Self {
        Self { raw }
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
