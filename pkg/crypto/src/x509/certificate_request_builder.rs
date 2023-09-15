use std::collections::HashMap;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use asn::builtin::{BitString, IA5String, ObjectIdentifier, SequenceOf, SetOf, UTF8String};
use asn::encoding::{Any, DERWriteable};
use common::bits::BitVector;
use common::bytes::Bytes;
use common::errors::*;
use common::libc::signal;
use pkix::PKIX1Explicit88::{
    self, AlgorithmIdentifier, AttributeTypeAndValue, AttributeValue, Name, RDNSequence,
    RelativeDistinguishedName,
};
use pkix::PKCS_10::Attributes;
use pkix::{PKIX1Implicit88, PKCS_10};

use crate::x509::CertificateRequest;
use crate::x509::PrivateKey;
use crate::x509::SignatureKeyConstraints;

// TODO: Maybe add extendedKeyUsage

#[derive(Default)]
pub struct CertificateRequestBuilder {
    // TODO: Error on duplicate extensions or attributes.
    subject: BTreeMap<ObjectIdentifier, Any>,
    extensions: BTreeMap<ObjectIdentifier, ExtensionEntry>,
}

struct ExtensionEntry {
    critical: bool,
    value: Bytes,
}

impl CertificateRequestBuilder {
    pub fn set_common_name(&mut self, name: &str) -> Result<&mut Self> {
        let id = PKIX1Explicit88::ID_AT_COMMONNAME.clone().into();
        let value = Any::from(
            PKIX1Explicit88::X520CommonName::utf8String(UTF8String::new(name))
                .to_der()
                .into(),
        )?;

        self.subject.insert(id, value);

        Ok(self)
    }

    pub fn set_extension<V: DERWriteable>(
        &mut self,
        id: ObjectIdentifier,
        critical: bool,
        value: V,
    ) -> &mut Self {
        self.extensions.insert(
            id,
            ExtensionEntry {
                critical,
                value: value.to_der().into(),
            },
        );

        self
    }

    pub fn set_subject_alt_names<S: AsRef<str>>(&mut self, dns_names: &[S]) -> Result<&mut Self> {
        let mut names = vec![];
        for n in dns_names {
            names.push(PKIX1Implicit88::GeneralName::dNSName(IA5String::new(
                n.as_ref(),
            )?));
        }

        let san = PKIX1Implicit88::SubjectAltName::from(PKIX1Implicit88::GeneralNames::from(
            SequenceOf::from(names),
        ));

        Ok(self.set_extension(PKIX1Implicit88::ID_CE_SUBJECTALTNAME, false, san))
    }

    /// Signs and builds the CertificateRequest returning it as a DER encoded
    /// binary object.
    pub async fn build(&self, private_key: &PrivateKey) -> Result<CertificateRequest> {
        let mut subject_rdns = vec![];
        for (typ, value) in &self.subject {
            // NOTE: Must CAs will use one attribute per RDN.
            subject_rdns.push(RelativeDistinguishedName::from(SetOf::from(vec![
                AttributeTypeAndValue {
                    typ: typ.clone().into(),
                    value: value.clone().into(),
                },
            ])));
        }

        let mut attributes = vec![];

        if !self.extensions.is_empty() {
            let mut extensions = vec![];
            for (id, entry) in &self.extensions {
                extensions.push(PKIX1Explicit88::Extension {
                    extnID: id.clone(),
                    critical: entry.critical,
                    extnValue: entry.value.clone().into(),
                });
            }

            let extension_request = pkix::PKCS_9::ExtensionRequest::from(
                PKIX1Explicit88::Extensions::from(SequenceOf::from(extensions)),
            );

            attributes.push(PKIX1Explicit88::Attribute {
                typ: pkix::PKCS_9::PKCS_9_AT_EXTENSIONREQUEST.into(),
                values: SetOf::from(vec![Any::from(extension_request.to_der().into())?.into()]),
            });
        }

        let request_info = PKCS_10::CertificationRequestInfo {
            version: PKCS_10::Version::v1,
            subject: Name::rdnSequence(RDNSequence::from(SequenceOf::from(subject_rdns))),
            subjectPKInfo: private_key.public_key()?.to_asn1(),
            attributes: Attributes::from(SetOf::from(attributes)),
        };

        // TODO: Make this configurable.
        let algorithm_ident = private_key.default_signature_algorithm();

        let signature = {
            let plaintext = request_info.to_der();

            private_key
                .create_signature(
                    &plaintext,
                    &algorithm_ident,
                    &SignatureKeyConstraints::default(),
                )
                .await?
        };

        let cert = PKCS_10::CertificationRequest {
            certificationRequestInfo: request_info,
            signatureAlgorithm: algorithm_ident,
            signature: BitString::from(BitVector::from(signature.as_ref(), signature.len() * 8)),
        };

        Ok(CertificateRequest::new(cert))
    }
}
