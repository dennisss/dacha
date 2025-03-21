use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;
use std::time::SystemTime;

use asn::builtin::{
    BitString, GeneralizedTime, IA5String, OctetString, SequenceOf, SetOf, UTF8String,
};
use asn::encoding::{Any, DERWriteable};
use base_error::*;
use common::bits::BitVector;
use common::chrono::{DateTime, Timelike, Utc};
use math::big::BigInt;
use pkix::PKIX1Explicit88::{
    AttributeTypeAndValue, Name, RDNSequence, RelativeDistinguishedName, SubjectPublicKeyInfo,
};
use pkix::PKIX1Implicit88::{GeneralSubtree, GeneralSubtrees, KeyIdentifier};
use pkix::{
    PKIX1Explicit88::{self, CertificateSerialNumber, Extensions, TBSCertificate},
    PKIX1Implicit88,
};

use crate::hasher::Hasher;
use crate::random::Rng;
use crate::sha256::SHA256Hasher;
use crate::x509::SignatureKeyConstraints;

use super::{
    Certificate, CertificateRegistry, CertificateRequest, CertificateVerified, PrivateKey,
};

/// Builds a Certificate by signing a CertificateRequest with a CA certificate /
/// private key.
pub struct CertificateBuilder {
    request: CertificateRequest,
    duration: Duration,
    creating_ca: bool,
    subject: SubjectValue,
    san: SubjectAltNameValue,
    permitted_subtrees: Option<Vec<String>>,
}

/// Determines how to derive the subject field of the certificate being built.
pub enum SubjectValue {
    CopyCSR,
    CommonName(String),
}

pub enum SubjectAltNameValue {
    None,
    CopyCSR,
    DNSNames(Vec<String>),
}

impl CertificateBuilder {
    /// Initializes a builder from a request.
    ///
    /// Note that by default nothing is copied from the CSR aside from the
    /// public key info. The caller needs to specify using the other methods of
    /// Self which extensions to transfer over or reconfigure.
    pub fn new(
        request: CertificateRequest,
        duration: Duration,
        subject: SubjectValue,
    ) -> Result<Self> {
        if !request.verify_signature()? {
            return Err(err_msg("Certificate request is not correctly signed"));
        }

        // TODO: Ensure that we capture all critical attributes and see if we will allow
        // to passthrough any extensions.

        // TODO: Check that the private key matches the CA

        Ok(Self {
            request,
            duration,
            creating_ca: false,
            subject,
            san: SubjectAltNameValue::None,
            permitted_subtrees: None,
        })
    }

    pub fn set_subject_alt_names(&mut self, san: SubjectAltNameValue) -> &mut Self {
        self.san = san;
        self
    }

    /// Call to indicate that we should create a CA certificate (one that is
    /// allowed to sign other certificates).
    pub fn create_ca(&mut self) -> &mut Self {
        self.creating_ca = true;
        self
    }

    pub fn set_permitted_subtrees(&mut self, names: &[String]) -> &mut Self {
        self.permitted_subtrees = Some(names.to_vec());
        self
    }

    /// Finishes building the certificate by signing with an issuer certificate.
    pub async fn build(
        &self,
        ca: Option<&Certificate>,
        private_key: &PrivateKey,
    ) -> Result<Vec<u8>> {
        // TODO: Verify the issuer name constriants are ok (this needs to verify the
        // whole chain up to the root cert).

        if let Some(ca) = ca {
            if !ca.can_sign_certificates()? {
                return Err(err_msg(
                    "This CA certificate can not be used to sign child certificates.",
                ));
            }

            if ca.subject_key_id().is_empty() {
                return Err(err_msg("Expecting CA to have a non-empty serial number"));
            }
        } else {
            if !self.creating_ca {
                return Err(err_msg("Self signed certificate must be a CA"));
            }
        }

        let subject = match &self.subject {
            SubjectValue::CopyCSR => self.request.raw().certificationRequestInfo.subject.clone(),
            SubjectValue::CommonName(name) => {
                let id = PKIX1Explicit88::ID_AT_COMMONNAME.clone().into();
                let value = Any::from(
                    PKIX1Explicit88::X520CommonName::utf8String(UTF8String::new(name))
                        .to_der()
                        .into(),
                )?;

                let mut subject_rdns = vec![];
                subject_rdns.push(RelativeDistinguishedName::from(SetOf::from(vec![
                    AttributeTypeAndValue {
                        typ: id,
                        value: value.clone().into(),
                    },
                ])));

                Name::rdnSequence(RDNSequence::from(SequenceOf::from(subject_rdns)))
            }
        };

        let issuer = match ca.clone() {
            Some(ca) => ca.raw.tbsCertificate.subject.clone(),
            None => subject.clone(),
        };

        let serialNumber = Self::generate_serial_number();

        // TODO: Verify that this is a reasonably secure algorithm.
        let subjectPublicKeyInfo = self
            .request
            .raw()
            .certificationRequestInfo
            .subjectPKInfo
            .clone();

        let subject_key_id = Self::generate_key_id(&subjectPublicKeyInfo);

        let authority_key_id = match ca {
            Some(ca) => ca.subject_key_id().to_vec(),
            None => subject_key_id.clone(),
        };

        // TODO: Should these use 'Any'
        let mut extensions = vec![];

        extensions.push(PKIX1Explicit88::Extension {
            extnID: PKIX1Implicit88::ID_CE_BASICCONSTRAINTS,
            critical: true,
            extnValue: PKIX1Implicit88::BasicConstraints {
                cA: self.creating_ca,
                pathLenConstraint: None,
            }
            .to_der()
            .into(),
        });

        extensions.push(PKIX1Explicit88::Extension {
            extnID: PKIX1Implicit88::ID_CE_AUTHORITYKEYIDENTIFIER,
            critical: false,
            extnValue: PKIX1Implicit88::AuthorityKeyIdentifier {
                keyIdentifier: Some(OctetString::from(authority_key_id).into()),
                // Not needed since we use key identifiers.
                authorityCertIssuer: None,
                authorityCertSerialNumber: None,
            }
            .to_der()
            .into(),
        });

        extensions.push(PKIX1Explicit88::Extension {
            extnID: PKIX1Implicit88::ID_CE_SUBJECTKEYIDENTIFIER,
            critical: false,
            extnValue: PKIX1Implicit88::SubjectKeyIdentifier::from(
                PKIX1Implicit88::KeyIdentifier::from(OctetString::from(subject_key_id)),
            )
            .to_der()
            .into(),
        });

        match &self.san {
            SubjectAltNameValue::None => {}
            SubjectAltNameValue::CopyCSR => {
                if let Some(san) = self.request.subject_alt_name()? {
                    extensions.push(PKIX1Explicit88::Extension {
                        extnID: PKIX1Implicit88::ID_CE_SUBJECTALTNAME,
                        critical: false,
                        extnValue: san.to_der().into(),
                    });
                }
            }
            SubjectAltNameValue::DNSNames(dns_names) => {
                let mut names = vec![];
                for n in dns_names {
                    names.push(PKIX1Implicit88::GeneralName::dNSName(IA5String::new(
                        n.as_ref(),
                    )?));
                }

                let san = PKIX1Implicit88::SubjectAltName::from(
                    PKIX1Implicit88::GeneralNames::from(SequenceOf::from(names)),
                );

                extensions.push(PKIX1Explicit88::Extension {
                    extnID: PKIX1Implicit88::ID_CE_SUBJECTALTNAME,
                    critical: false,
                    extnValue: san.to_der().into(),
                });
            }
        }

        if let Some(permitted_subtrees) = &self.permitted_subtrees {
            let mut subtrees: Vec<GeneralSubtree> = vec![];

            for name in permitted_subtrees {
                subtrees.push(GeneralSubtree {
                    base: PKIX1Implicit88::GeneralName::dNSName(IA5String::new(name.as_ref())?),
                    minimum: BigInt::zero().into(),
                    maximum: None,
                });
            }

            extensions.push(PKIX1Explicit88::Extension {
                extnID: PKIX1Implicit88::ID_CE_NAMECONSTRAINTS,
                critical: true, // TODO: Check this
                extnValue: PKIX1Implicit88::NameConstraints {
                    permittedSubtrees: Some(GeneralSubtrees::from(SequenceOf::from(subtrees))),
                    excludedSubtrees: None,
                }
                .to_der()
                .into(),
            });
        }

        let now = DateTime::<Utc>::from(SystemTime::now())
            .with_nanosecond(0)
            .unwrap();

        // TODO: Verify expiration is before expiration of issuer cert.

        let expire = now + common::chrono::Duration::from_std(self.duration).unwrap();

        // TODO: Make this configurable.
        let algorithm_ident = private_key.default_signature_algorithm();

        let tbsCertificate = TBSCertificate {
            version: pkix::PKIX1Explicit88::Version::v3,
            serialNumber: Self::generate_serial_number(),
            signature: algorithm_ident.clone(),
            issuer,
            validity: PKIX1Explicit88::Validity {
                notBefore: PKIX1Explicit88::Time::generalTime(
                    GeneralizedTime::from_datetime(now).into(),
                ),
                notAfter: PKIX1Explicit88::Time::generalTime(
                    GeneralizedTime::from_datetime(expire).into(),
                ),
            },
            subject,
            subjectPublicKeyInfo,
            issuerUniqueID: None,
            subjectUniqueID: None,
            extensions: Some(PKIX1Explicit88::Extensions::from(SequenceOf::from(
                extensions,
            ))),
        };

        let signature = {
            let plaintext = tbsCertificate.to_der();

            private_key
                .create_signature(
                    &plaintext,
                    &algorithm_ident,
                    &SignatureKeyConstraints::default(),
                )
                .await?
        };

        let cert = PKIX1Explicit88::Certificate {
            tbsCertificate,
            signatureAlgorithm: algorithm_ident,
            signature: BitString::from(BitVector::from(signature.as_ref(), signature.len() * 8)),
        };

        Ok(cert.to_der())
    }

    // TODO: Use secure random
    fn generate_serial_number() -> CertificateSerialNumber {
        // Max length is 20 octets.
        let mut num = vec![0u8; 20];
        crate::random::clocked_rng().generate_bytes(&mut num);

        // Serial number must be '> 0'
        num[0] &= 0b01111111;

        let int = BigInt::from_le_bytes(&num);

        int.into()
    }

    fn generate_key_id(pkey_info: &SubjectPublicKeyInfo) -> Vec<u8> {
        let mut hasher = SHA256Hasher::default();
        hasher.update(pkey_info.subjectPublicKey.as_ref());

        let mut id = hasher.finish();
        id.truncate(160 / 8);

        id
    }
}
