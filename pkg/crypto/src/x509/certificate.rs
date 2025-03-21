use std::collections::HashMap;
use std::convert::{AsRef, TryFrom, TryInto};
use std::string::String;
use std::string::ToString;
use std::sync::Arc;
use std::vec::Vec;

use asn::builtin::{Null, ObjectIdentifier, OctetString};
use asn::encoding::{der_eq, Any, DERReadable, DERReader, DERWriteable};
use common::bytes::Bytes;
use common::chrono::{DateTime, Utc};
use common::errors::*;
use common::failure::ResultExt;
use math::big::{BigInt, BigUint, Modulo};
use pkix::{
    PKIX1Algorithms2008, PKIX1Algorithms88, PKIX1Explicit88, PKIX1Implicit88,
    PKIX1_PSS_OAEP_Algorithms, NIST_SHA2, PKCS_1,
};

use crate::elliptic::EllipticCurveGroup;
use crate::hasher::Hasher;
use crate::pem::*;
use crate::rsa::*;
use crate::tls::extensions::ExtensionType::PskKeyExchangeModes;
use crate::x509::certificate_registry::CertificateRegistry;
use crate::x509::signature_key::SignatureKeyConstraints;

use super::PublicKey;

fn Time_to_datetime(t: &PKIX1Explicit88::Time) -> DateTime<Utc> {
    match t {
        PKIX1Explicit88::Time::generalTime(t) => t.to_datetime(),
        PKIX1Explicit88::Time::utcTime(t) => t.to_datetime().into(),
    }
}

#[derive(Debug)]
pub struct Validity {
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
}

#[derive(Debug)]
pub struct Certificate {
    validity: Validity,

    /// Reference to the DER encoded buffer from which the TBSCertificate inside
    /// of the root struct was parsed (in other words, this is the buffer that
    /// is signed).
    ///
    /// (not meant to be BER or CER).
    plaintext: Bytes,

    subject_key_id: Bytes,

    authority_key_id: Bytes,
    authority_serial_number: Option<BigInt>,

    extensions: CertificateExtensions,

    /// Raw parsed ASN sequence backing this certificate.
    ///
    /// TODO: Eventualyl make private again.
    pub raw: PKIX1Explicit88::Certificate,
}

#[derive(Debug)]
pub(super) struct CertificateExtensions {
    map: HashMap<ObjectIdentifier, CertificateExtensionEntry>,
}

#[derive(Debug)]
struct CertificateExtensionEntry {
    value: Bytes,
    critical: bool,
}

impl CertificateExtensions {
    pub fn from(exts: &[PKIX1Explicit88::Extension]) -> Result<Self> {
        let mut map = HashMap::new();
        for e in exts {
            let id = e.extnID.clone();
            let val = e.extnValue.to_bytes();

            // It is illegal for certificates to contain duplicate
            // extensions.
            if map.contains_key(&id) {
                return Err(err_msg("Extension with duplicate id"));
            }

            map.insert(
                id,
                CertificateExtensionEntry {
                    value: val,
                    critical: e.critical,
                },
            );
        }

        Ok(Self { map })
    }

    pub fn get(&self, id: &ObjectIdentifier) -> Option<Bytes> {
        self.map.get(id).map(|v| &v.value).cloned()
    }

    pub fn get_as<T: DERReadable>(&self, id: &ObjectIdentifier) -> Result<Option<T>> {
        match self.get(id) {
            Some(data) => Ok(Some(Any::from(data)?.parse_as()?)),
            None => Ok(None),
        }
    }
}

impl Certificate {
    // TODO: Verify that we have used all critical extensions.
    // critical to implement: keyUsage 2.5.29.15, basicConstraints 2.5.29.19

    // Internal constructor. All creations should go through this.
    fn new(raw: PKIX1Explicit88::Certificate, plaintext: Bytes) -> Result<Self> {
        //		if raw.tbsCertificate.version != PKIX1Explicit88::Version::v3 {
        //			return Err(err_msg("Unsupported version"));
        //		}

        if !der_eq(&raw.signatureAlgorithm, &raw.tbsCertificate.signature) {
            return Err(err_msg("Mismatching signature algorithms"));
        }

        let validity = Validity {
            not_before: Time_to_datetime(&raw.tbsCertificate.validity.notBefore),
            not_after: Time_to_datetime(&raw.tbsCertificate.validity.notAfter),
        };

        if validity.not_after < validity.not_before {
            return Err(err_msg("Out of order validity range"));
        }

        if raw.tbsCertificate.subjectUniqueID.is_some()
            || raw.tbsCertificate.issuerUniqueID.is_some()
        {
            return Err(err_msg("Certificate contains deprecated unique id fields"));
        }

        // NOTE: Some non-conforming CAs use non-positive or zero serial numbers.
        // Up to 20 octets (+ a sign bit)
        if raw.tbsCertificate.serialNumber.value_bits() > (8 * 20 + 1)
        // || !raw.tbsCertificate.serialNumber.is_positive()
        // || raw.tbsCertificate.serialNumber.is_zero()
        {
            println!("{:?}", raw);

            return Err(err_msg("Invalid certificate serial number."));
        }

        let extensions = CertificateExtensions::from(
            raw.tbsCertificate
                .extensions
                .as_ref()
                .map(|e| e.as_ref())
                .unwrap_or(&[]),
        )?;

        // NOTE: This extension should always be non-critical.
        let subject_key_id = extensions
            .get_as::<PKIX1Implicit88::SubjectKeyIdentifier>(
                &PKIX1Implicit88::ID_CE_SUBJECTKEYIDENTIFIER,
            )?
            .map(|k| k.to_bytes())
            .unwrap_or(Bytes::new());

        // NOTE: This extension should always be non-critical.
        let (authority_key_id, authority_serial_number) =
            match extensions.get_as::<PKIX1Implicit88::AuthorityKeyIdentifier>(
                &PKIX1Implicit88::ID_CE_AUTHORITYKEYIDENTIFIER,
            )? {
                Some(id) => {
                    // Technically we should allow this, but we don't support looking up
                    // certificates with this custom issuer and this may lead to having weird chains
                    // like 'A -> B -> C' where A signs B and C which makes it challenging to
                    // validate 'C' as we need to ensure A is a parent of C's parent (B).
                    // if let Some(authority_issuer) = &id.authorityCertIssuer {
                    //     if !der_eq(authority_issuer, &raw.tbsCertificate.issuer) {
                    //         return Err(format_err!(
                    //             "Different authority issuer not supported: {:?}",
                    //             authority_issuer
                    //         ));
                    //     }
                    // }

                    (
                        id.keyIdentifier.map(|v| v.to_bytes()).unwrap_or_default(),
                        id.authorityCertSerialNumber.clone().map(|v| v.into()),
                    )
                }
                None => (Bytes::new(), None),
            };

        let supported_extension_ids = [
            PKIX1Implicit88::ID_CE_AUTHORITYKEYIDENTIFIER,
            PKIX1Implicit88::ID_CE_SUBJECTKEYIDENTIFIER,
            PKIX1Implicit88::ID_CE_SUBJECTALTNAME,
            PKIX1Implicit88::ID_CE_KEYUSAGE,
            PKIX1Implicit88::ID_CE_BASICCONSTRAINTS,
            PKIX1Implicit88::ID_CE_NAMECONSTRAINTS,
        ];

        // Verify all extensions are supported by our code.
        // TODO: Also pass in a set of user supported ids here.
        for (id, value) in extensions.map.iter() {
            if !value.critical {
                continue;
            }

            if !supported_extension_ids.contains(id) {
                return Err(format_err!(
                    "Certificate contains unsupported critical extension with id: {:?}",
                    id,
                ));
            }
        }

        Ok(Self {
            validity,
            plaintext,
            extensions,
            raw,
            subject_key_id,
            authority_key_id,
            authority_serial_number,
        })
    }

    pub fn from_pem(buf: Bytes) -> Result<Vec<Arc<Certificate>>> {
        let pem = PEM::parse(buf)?;

        let mut out = vec![];
        out.reserve(pem.entries.len());

        for entry in &pem.entries {
            if entry.label.as_ref() != PEM_CERTIFICATE_LABEL {
                return Err(err_msg("PEM contains a non-certificate"));
            }

            let c = Self::read(entry.to_binary()?.into())?;
            out.push(Arc::new(c));
        }

        Ok(out)
    }

    pub fn to_pem(certs: &[Arc<Certificate>]) -> String {
        let mut builder = PEMBuilder::default();
        for cert in certs {
            builder.add_binary_entry(PEM_CERTIFICATE_LABEL, &cert.raw.to_der());
        }
        builder.build()
    }

    /// Reads a certficate from DER encoded data.
    pub fn read(buf: Bytes) -> Result<Self> {
        // TODO: Ensure the buffer is read till completion.
        let mut r = DERReader::new(buf);
        let raw = PKIX1Explicit88::Certificate::read_der(&mut r)?;
        Self::new(raw, r.slices[1].clone())
    }

    pub fn to_der(&self) -> Vec<u8> {
        self.raw.to_der()
    }

    pub fn validity(&self) -> &Validity {
        &self.validity
    }

    pub fn serial_number(&self) -> &BigInt {
        self.raw.tbsCertificate.serialNumber.as_ref()
    }

    pub fn issuer(&self) -> DistinguishedName {
        DistinguishedName::from(&self.raw.tbsCertificate.issuer)
    }

    pub fn subject(&self) -> DistinguishedName {
        DistinguishedName::from(&self.raw.tbsCertificate.subject)
    }

    /// Subject Key Identifier (possibly empty slice if not present).
    pub fn subject_key_id(&self) -> &[u8] {
        self.subject_key_id.as_ref()
    }

    /// Authority Key Id (possibly empty if not present or self-signed).
    pub fn authority_key_id(&self) -> &[u8] {
        self.authority_key_id.as_ref()
    }

    pub fn authority_serial_number(&self) -> Option<&BigInt> {
        self.authority_serial_number.as_ref()
    }

    /// TODO: Validate that this has at least one name.
    pub fn subject_alt_name(&self) -> Result<Option<PKIX1Implicit88::SubjectAltName>> {
        self.extensions
            .get_as(&PKIX1Implicit88::ID_CE_SUBJECTALTNAME)
    }

    pub fn key_usage(&self) -> Result<Option<PKIX1Implicit88::KeyUsage>> {
        self.extensions.get_as(&PKIX1Implicit88::ID_CE_KEYUSAGE)
    }

    pub fn basic_constraints(&self) -> Result<Option<PKIX1Implicit88::BasicConstraints>> {
        self.extensions
            .get_as(&PKIX1Implicit88::ID_CE_BASICCONSTRAINTS)
    }

    pub fn name_constraints(&self) -> Result<Option<PKIX1Implicit88::NameConstraints>> {
        self.extensions
            .get_as(&PKIX1Implicit88::ID_CE_NAMECONSTRAINTS)
    }

    /// Whether or not this certificate is issued by the same entity that made
    /// the certificate.
    ///
    /// Does NOT verify certificate correctness
    ///
    /// NOTE: This is not the same as a self-signed certificate.
    ///
    /// NOTE: Does not verify if the signature is valid.
    pub fn self_issued(&self) -> bool {
        // TODO: Need to normalize distinguished names per https://www.rfc-editor.org/rfc/rfc5280#section-7.1 whenever we do Issuer comparison.
        der_eq(
            &self.raw.tbsCertificate.issuer,
            &self.raw.tbsCertificate.subject,
        )
    }

    /// Checks if this certificate is expected to sign itself
    ///
    /// Only root CA certificates should be self signed. This doesn't verify
    /// that the signature is actually valid though.
    pub fn self_signed(&self) -> bool {
        if !self.self_issued() {
            return false;
        }

        self.authority_key_id().is_empty() || self.authority_key_id() == self.subject_key_id()
    }

    pub fn public_key(&self, reg: &CertificateRegistry) -> Result<PublicKey> {
        let parent_key = match reg.lookup_parent(self)? {
            Some(cert) => Some(cert.public_key(reg)?),
            None => None,
        };

        PublicKey::from_asn1(
            &self.raw.tbsCertificate.subjectPublicKeyInfo,
            parent_key.as_ref(),
        )
    }

    /// Checks if the current certificate can be used to sign/verify child
    /// certificates.
    pub fn can_sign_certificates(&self) -> Result<bool> {
        if let Some(key_usage) = self.key_usage()? {
            if !key_usage.keyCertSign().unwrap_or(false) {
                return Ok(false);
            }
        }

        if let Some(constraints) = self.basic_constraints()? {
            if !constraints.cA {
                return Ok(false);
            }
        } else if self.raw.tbsCertificate.version == PKIX1Explicit88::Version::v3 {
            // RFC 5280 states that V3 certificates must have the basic
            // constraints extension to be a CA.

            // NOTE: This will return false for some root CAs which incorrectly
            // omit basic constraints.
            /*
            return Ok(false);
            */
        }

        // Fails for some trusted root CAs
        /*
        if self.subject_key_id().is_empty() {
            return Ok(false);
        }
        */

        Ok(true)
    }

    // TODO: Have a DigitalSignatureAlgorithm trait (or SignatureAlgoritm) to
    // disambiguate it.

    // RSASSA-PKCS1-v1_5
    // The key to this is the padding as described here: https://tools.ietf.org/html/rfc3447#section-9.2

    /// Using the current certificate's public key, check that some external
    /// signature was produced with the private key corresponding to the current
    /// public key.
    ///
    /// (no other validation is performed aside from checking the signature).
    ///
    /// We assume that the current certificate's signature has already been
    /// verified against its parent and this certificate is allowed to sign
    /// other certficiates.
    pub(super) fn verify_child_signature(
        &self,
        child: &Certificate,
        reg: &CertificateRegistry,
    ) -> Result<bool> {
        if !self.can_sign_certificates()? {
            return Err(err_msg("Certificate can't be used for signing others"));
        }

        let plaintext = &child.plaintext;
        // TODO: Must verify that this is divisible by 8
        let sig = child.raw.signature.as_ref();

        /*
        // TODO: Perform some type of sanity check like this once more writing
        // is implemented.
        {
            let der = self.raw.tbsCertificate.to_der();
            eprintln!("{} {}", der.len(), plaintext.len());
            assert_eq!(plaintext, &der[..]);
        }
        */

        self.public_key(reg)?.verify_signature(
            plaintext,
            sig,
            &child.raw.signatureAlgorithm,
            &SignatureKeyConstraints::default(),
        )
    }

    pub fn valid_now(&self) -> bool {
        let now = Utc::now();
        now >= self.validity.not_before && now <= self.validity.not_after
    }

    /// Checks whether or not this certificate can be used to authenticate the
    /// given dns name.
    ///
    /// NOTE: DNS names should not end in a '.'
    pub fn for_dns_name(&self, name: &str) -> Result<bool> {
        let name = name.to_ascii_lowercase();
        let name_parts = name.split('.').collect::<Vec<_>>();

        let match_with = |pattern: &str| -> bool {
            let pattern = pattern.to_ascii_lowercase();
            let pattern_parts = pattern.split('.').collect::<Vec<_>>();
            if name_parts.len() != pattern_parts.len() {
                return false;
            }

            for i in 0..pattern_parts.len() {
                if i == 0 && pattern_parts[i] == "*" {
                    continue;
                } else if name_parts[i] != pattern_parts[i] {
                    return false;
                }
            }

            true
        };

        match self.subject_alt_name()? {
            Some(v) => {
                for name in &v.items {
                    if let PKIX1Implicit88::GeneralName::dNSName(s) = name {
                        if match_with(s.data.as_ref()) {
                            return Ok(true);
                        }
                    }
                }
            }
            None => {
                // TODO: We could check the subject common name but it is pretty
                // much deprecated and discourages from being used.
            }
        };

        Ok(false)
    }
}

pub struct DistinguishedName<'a> {
    value: &'a PKIX1Explicit88::RDNSequence,
}

impl<'a> DistinguishedName<'a> {
    pub fn from(name: &'a PKIX1Explicit88::Name) -> Self {
        Self {
            value: match name {
                PKIX1Explicit88::Name::rdnSequence(v) => v,
            },
        }
    }

    pub fn common_name(&self) -> Result<Option<String>> {
        // TODO: Dedup this and verify there is only one common name.

        for item in &self.value.items {
            for item in &item.items {
                if !der_eq(&item.typ, &PKIX1Explicit88::ID_AT_COMMONNAME) {
                    continue;
                }

                let cn = item.value.parse_as::<PKIX1Explicit88::X520CommonName>()?;
                let s = match &cn {
                    PKIX1Explicit88::X520CommonName::teletexString(v) => v.as_str(),
                    PKIX1Explicit88::X520CommonName::printableString(v) => v.as_str(),
                    PKIX1Explicit88::X520CommonName::universalString(v) => v.as_str(),
                    PKIX1Explicit88::X520CommonName::utf8String(v) => v.as_str(),
                    PKIX1Explicit88::X520CommonName::bmpString(v) => v.as_str(),
                };

                return Ok(Some(s.into()));
            }
        }

        Ok(None)
    }

    pub fn to_string(&self) -> Result<String> {
        let mut out = String::new();
        for rdn in self.value.as_ref() {
            for attr in rdn.as_ref() {
                if let Some((name, f)) = ATTRIBUTE_REGISTRY.get(attr.typ.as_ref()) {
                    let val = f(attr.value.as_ref())?;
                    out += &format!("{}: {}\n", name, val);
                } else {
                    out += &format!("[unknown]: {:?}\n", &attr.typ);
                }
            }
        }

        Ok(out)
    }
}

type AttributeRegistry = std::collections::HashMap<
    ObjectIdentifier,
    (
        &'static str,
        &'static (Send + Sync + Fn(&Any) -> Result<String>),
    ),
>;

// TODO: Refactor to use AttributeType instead of ObjectIdentifier.
macro_rules! attrs {
	( $name:ident, $( $attr:tt | $id:expr => $t:ty ),* ) => {
		lazy_static! {
			pub static ref $name: AttributeRegistry = {
				let mut map = AttributeRegistry::new();
				$(
					fn $attr(a: &Any) -> Result<String> {
						a.parse_as::<$t>().map(|v| v.to_string())
					}

					map.insert($id.as_ref().clone(), (
						stringify!($attr), &$attr
					));
				)*

				map
			};
		}
	};
}

/*
extensionRequest ATTRIBUTE ::= {
        WITH SYNTAX ExtensionRequest
        SINGLE VALUE TRUE
        ID pkcs-9-at-extensionRequest
}
*/

attrs!(ATTRIBUTE_REGISTRY,
    name | PKIX1Explicit88::ID_AT_NAME => PKIX1Explicit88::X520name,
    surname | PKIX1Explicit88::ID_AT_SURNAME => PKIX1Explicit88::X520name,
    givenName | PKIX1Explicit88::ID_AT_GIVENNAME => PKIX1Explicit88::X520name,
    initials | PKIX1Explicit88::ID_AT_INITIALS => PKIX1Explicit88::X520name,
    generationQualifier | PKIX1Explicit88::ID_AT_GENERATIONQUALIFIER =>
        PKIX1Explicit88::X520name,
    commonName | PKIX1Explicit88::ID_AT_COMMONNAME =>
        PKIX1Explicit88::X520CommonName,
    localityName | PKIX1Explicit88::ID_AT_LOCALITYNAME =>
        PKIX1Explicit88::X520LocalityName,
    stateOrProvinceName | PKIX1Explicit88::ID_AT_STATEORPROVINCENAME =>
        PKIX1Explicit88::X520StateOrProvinceName,
    organizationName | PKIX1Explicit88::ID_AT_ORGANIZATIONNAME =>
        PKIX1Explicit88::X520OrganizationName,
    organizationalUnitName | PKIX1Explicit88::ID_AT_ORGANIZATIONALUNITNAME =>
        PKIX1Explicit88::X520OrganizationalUnitName,
    title | PKIX1Explicit88::ID_AT_TITLE =>
        PKIX1Explicit88::X520Title,
    dnQualifier | PKIX1Explicit88::ID_AT_DNQUALIFIER =>
        PKIX1Explicit88::X520dnQualifier,
    countryName | PKIX1Explicit88::ID_AT_COUNTRYNAME =>
        PKIX1Explicit88::X520countryName,
    serialNumber | PKIX1Explicit88::ID_AT_SERIALNUMBER =>
        PKIX1Explicit88::X520SerialNumber,
    pseudonym | PKIX1Explicit88::ID_AT_PSEUDONYM =>
        PKIX1Explicit88::X520Pseudonym
    // extensionRequest | pkix::PKCS_9::PKCS_9_AT_EXTENSIONREQUEST =>
    //     pkix::PKCS_9::ExtensionRequest
);
