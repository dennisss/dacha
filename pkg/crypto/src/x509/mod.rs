mod certificate_verified;
mod name_constraints;
mod private_key;

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

use certificate_verified::*;
use name_constraints::*;
pub use private_key::*;

// TODO: For validating this, we also need to able to check max allowed
// certificate chain length.

// NOTE: This field MUST contain the same algorithm identifier as the
//    signature field in the sequence tbsCertificate

/*
Wrapper for reading a certificate
- Need map to know about unknown extensions

*/

/*
TODO: We can get root certificates from
https://android.googlesource.com/platform/system/ca-certificates/+/master/
*/

// TODO: Also verify that we can't use a duplicate key id to bypass the
// signature check.

// TODO: Must also deal with possible cycles.

// NOTE: Here is how OpenSSL does Name hashing:
// https://github.com/openssl/openssl/blob/47b4ccea9cb9b924d058fd5a8583f073b7a41656/crypto/x509/x509_cmp.c#L184

// TODO: Read https://tools.ietf.org/html/rfc5280#section-7.1 for the exact
// rules for comparing names for the purpose of

/// Wrapper around a PKIX1Explicit88::Name which can be compared and is hashable
/// so can be used as a key in a map.
/// NOTE: all internal properties are immutable.
#[derive(PartialEq, Eq, Hash)]
pub struct NameKey {
    // DER-encoded version of the above name.
    // TODO: Should convert this to Bytes and do more caching during parsing of
    // the original certificate.
    encoded: Vec<u8>,
}

impl NameKey {
    pub fn from(value: &PKIX1Explicit88::Name) -> Self {
        Self {
            encoded: value.to_der(),
        }
    }
}

// TODO: For simplicity, assume the key identifier is always presnet.

// TODO: Parse CertificateList for CRLs

// TODO: Must implement critical extensions and check that all extension
// constraints are satisfied.

/// A self-consistent collection of certificates. All certificates in a registry
/// have valid signatures and for each certificate in a registry all
/// certificates in the chain up to a root certificate are also in the registry.
/// (thus certificates can only be added if they are added with the full chain)
///
/// NOTE: This is intentionally not clonable as this will typically be very
/// large.
pub struct CertificateRegistry {
    /// Map of a certificate's subject name to a list of all certificates issued
    /// to that subject.
    ///
    /// TODO: Add the certificate's subjectUniqueID to the key and then use that
    /// for lookups as well
    certs: HashMap<NameKey, Vec<Arc<CertificateVerified>>>,

    parent: Option<Arc<CertificateRegistry>>,
}

impl CertificateRegistry {
    /*
    System wide certificates located at:
    - /etc/ssl/certs/ca-certificates.crt
    - https://serverfault.com/questions/62496/ssl-certificate-location-on-unix-linux
    */

    /// Creates a registry filled with all publicly trusted root certificates.
    pub async fn public_roots() -> Result<Self> {
        let mut data = file::read(project_path!(
            "third_party/ca-certificates/google/roots.pem"
        ))
        .await?;

        let buf = Bytes::from(data);

        let certs = Certificate::from_pem(buf)?;

        let mut reg = CertificateRegistry::new();
        reg.append(&certs, true)?;
        Ok(reg)
    }

    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            certs: HashMap::new(),
            parent: None,
        }
    }

    /// Creates a new mutable registry which inherits all certificates in the
    /// current registry.
    ///
    /// This is meant to be cheaper than cloning the entire registry.
    pub fn child(self: &Arc<Self>) -> Self {
        Self {
            certs: HashMap::new(),
            parent: Some(self.clone()),
        }
    }

    /// Finds the certificate that was used to sign 'cert'.
    ///
    /// NOTE: This does not support looking up the parent of a self-signed cert.
    fn lookup_parent(&self, cert: &Certificate) -> Result<Option<Arc<CertificateVerified>>> {
        if let Some(parent) = &self.parent {
            if let Some(v) = parent.lookup_parent(cert)? {
                return Ok(Some(v));
            }
        }

        if cert.self_signed() {
            return Err(err_msg(
                "Trying to lookup parent of self-signed certificate",
            ));
        }

        let issuer = NameKey::from(&cert.raw.tbsCertificate.issuer);
        let certs = match self.certs.get(&issuer) {
            Some(list) => list,
            None => {
                return Ok(None);
            }
        };

        for c in certs {
            if cert.authority_key_id() == c.subject_key_id() {
                return Ok(Some(c.clone()));
            }
        }

        Ok(None)
    }

    // TODO: Need to perform an exact comparison to be sure.
    fn contains(&self, cert: &Certificate) -> bool {
        let list = self
            .certs
            .get(&NameKey::from(&cert.raw.tbsCertificate.subject))
            .map(|v| &v[..])
            .unwrap_or(&[]);

        for c2 in list.iter() {
            if cert.serial_number() == c2.serial_number() {
                return true;
            }

            if cert.subject_key_id() == c2.subject_key_id() {
                return true;
            }
        }

        if let Some(parent) = &self.parent {
            return parent.contains(cert);
        }

        false
    }

    /// Performs insertion into the inner certificate map. This assumes that the
    /// certificate chain has already been verified.
    ///
    /// A certificate can only be inserted if there is no other certificate with
    /// the same (issuer, serial number) or (issuer, subject key id) pair.
    ///
    /// Returns whether or not it was inserted newly. If false, then an
    /// identical certificate already existed in the registry with the exact
    /// same contents.
    /// TODO: Implement allowing exact matches.
    fn insert(&mut self, cert: Arc<CertificateVerified>) -> Result<bool> {
        let c = cert.as_ref();

        if self.contains(c) {
            return Err(err_msg("Registry may already contain the certificate"));
        }

        let list = self
            .certs
            .entry(NameKey::from(&c.raw.tbsCertificate.subject))
            .or_insert(vec![]);

        list.push(cert);
        Ok(true)
    }

    /// Adds all of the given certificates to the registry.
    ///
    /// NOTE: This is currently O(n*k) where n is the number of certificates
    /// given and k is the length of the chain in the given certificates.
    pub fn append(&mut self, certs: &[Arc<Certificate>], trusted: bool) -> Result<()> {
        let mut remaining = certs.to_vec();
        while remaining.len() > 0 {
            let mut changed = false;
            for raw_cert in remaining.split_off(0) {
                let verified_cert = if raw_cert.self_signed() {
                    if !trusted {
                        return Err(err_msg("Self-signed untrusted signature"));
                    }

                    // TODO: Refactor out this circular dependency on the registry.
                    CertificateVerified::verify_self_signed(raw_cert, self)?
                } else {
                    let parent_cert = match self.lookup_parent(&raw_cert)? {
                        Some(c) => c,
                        None => {
                            // This means that we processed the child before the parent so we need
                            // to retry once the parent was processed.
                            remaining.push(raw_cert);
                            continue;
                        }
                    };

                    parent_cert.verify_child(raw_cert, self)?
                };

                changed = true;
                self.insert(Arc::new(verified_cert))?;
            }

            if !changed {
                return Err(err_msg(
                    "Appending certificates with unknown parent in chain.",
                ));
            }
        }

        Ok(())
    }
}

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

    extensions: CertificateExtensions,

    /// Raw parsed ASN sequence backing this certificate.
    ///
    /// TODO: Eventualyl make private again.
    pub raw: PKIX1Explicit88::Certificate,
}

#[derive(Debug)]
struct CertificateExtensions {
    map: HashMap<ObjectIdentifier, CertificateExtensionEntry>,
}

#[derive(Debug)]
struct CertificateExtensionEntry {
    value: Bytes,
    critical: bool,
}

impl CertificateExtensions {
    fn from(exts: &[PKIX1Explicit88::Extension]) -> Result<Self> {
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

    fn get(&self, id: &ObjectIdentifier) -> Option<Bytes> {
        self.map.get(id).map(|v| &v.value).cloned()
    }

    fn get_as<T: DERReadable>(&self, id: &ObjectIdentifier) -> Result<Option<T>> {
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
        let authority_key_id = match extensions.get_as::<PKIX1Implicit88::AuthorityKeyIdentifier>(
            &PKIX1Implicit88::ID_CE_AUTHORITYKEYIDENTIFIER,
        )? {
            Some(id) => {
                /*
                if id.authorityCertIssuer.is_some() || id.authorityCertSerialNumber.is_some() {
                    return Err(err_msg(
                        "authorityCertIssuer|authorityCertSerialNumber not supported",
                    ));
                }
                */

                id.keyIdentifier.map(|v| v.to_bytes()).unwrap_or_default()
            }
            None => Bytes::new(),
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

    /// Reads a certficate from DER encoded data.
    pub fn read(buf: Bytes) -> Result<Self> {
        // TODO: Ensure the buffer is read till completion.
        let mut r = DERReader::new(buf);
        let raw = PKIX1Explicit88::Certificate::read_der(&mut r)?;
        Self::new(raw, r.slices[1].clone())
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

    /// NOTE: The return value is basically equivalent to
    /// PKIX1Algorithms2008::RSAPublicKey.
    pub fn rsa_public_key(&self) -> Result<PKCS_1::RSAPublicKey> {
        let pk = &self.raw.tbsCertificate.subjectPublicKeyInfo;

        if pk.algorithm.algorithm != PKIX1Algorithms2008::RSAENCRYPTION
            || !der_eq(
                &pk.algorithm.parameters,
                &Some(PKIX1_PSS_OAEP_Algorithms::NULLPARAMETERS),
            )
        {
            return Err(format_err!("Wrong public key info: {:?}", pk.algorithm));
        }

        let data = &pk.subjectPublicKey.data;
        if data.len() % 8 != 0 {
            return Err(err_msg("Not complete bytes"));
        }

        Any::from(Bytes::from(data.as_ref()))?.parse_as()
    }

    pub fn rsassa_pss_public_key(
        &self,
    ) -> Result<(
        PKCS_1::RSAPublicKey,
        PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params,
    )> {
        let pk = &self.raw.tbsCertificate.subjectPublicKeyInfo;

        if pk.algorithm.algorithm != PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS {
            return Err(format_err!("Wrong public key info: {:?}", pk.algorithm));
        }

        let params_data = pk
            .algorithm
            .parameters
            .as_ref()
            .ok_or_else(|| err_msg("Missing params"))?;

        let params = params_data.parse_as::<PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params>()?;

        let data = &pk.subjectPublicKey.data;
        let public_key: PKCS_1::RSAPublicKey = Any::from(Bytes::from(data.as_ref()))?.parse_as()?;

        Ok((public_key, params))
    }

    pub fn ec_public_key(&self, reg: &CertificateRegistry) -> Result<(EllipticCurveGroup, Bytes)> {
        let pk = &self.raw.tbsCertificate.subjectPublicKeyInfo;
        if pk.algorithm.algorithm != PKIX1Algorithms2008::ID_ECPUBLICKEY {
            return Err(err_msg("Wrong public key type"));
        }

        let params = match &pk.algorithm.parameters {
            Some(any) => any.parse_as::<PKIX1Algorithms88::EcpkParameters>()?,
            None => {
                return Err(err_msg("No EC params specified"));
            }
        };

        let group = match params {
            PKIX1Algorithms88::EcpkParameters::namedCurve(id) => {
                if id == PKIX1Algorithms2008::SECP192R1 {
                    EllipticCurveGroup::secp192r1()
                } else if id == PKIX1Algorithms2008::SECP224R1 {
                    EllipticCurveGroup::secp224r1()
                } else if id == PKIX1Algorithms2008::SECP256R1 {
                    EllipticCurveGroup::secp256r1()
                } else if id == PKIX1Algorithms2008::SECP384R1 {
                    EllipticCurveGroup::secp384r1()
                } else if id == PKIX1Algorithms2008::SECP521R1 {
                    EllipticCurveGroup::secp521r1()
                } else {
                    return Err(err_msg("Unsupported named curve"));
                }
            }
            PKIX1Algorithms88::EcpkParameters::implicitlyCA(_) => {
                let ca = reg.lookup_parent(self)?.ok_or(err_msg("Unknown parent"))?;
                let (group, _) = ca.ec_public_key(reg)?;
                group
            }
            _ => {
                return Err(err_msg("Unsupported curve format"));
            }
        };

        let point = PKIX1Algorithms2008::ECPoint::from(OctetString::from(
            pk.subjectPublicKey.data.as_ref(),
        ));

        Ok((
            group,
            std::convert::Into::<OctetString>::into(point).into_bytes(),
        ))
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
    pub(self) fn verify_child_signature(
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

        let check_ecdsa = |hasher: &mut dyn Hasher| {
            let (group, point) = self.ec_public_key(reg)?;
            return group.verify_signature(point.as_ref(), sig, plaintext, hasher);
        };

        let check_null_params = || -> Result<()> {
            if child.raw.signatureAlgorithm.parameters.is_some()
                && !der_eq(&child.raw.signatureAlgorithm.parameters, &Null::new())
            {
                return Err(err_msg("Expected null params for algorithm"));
            }
            Ok(())
        };

        let alg = &child.raw.signatureAlgorithm.algorithm;
        if alg == &PKIX1_PSS_OAEP_Algorithms::SHA224WITHRSAENCRYPTION {
            check_null_params()?;
            return RSASSA_PKCS_v1_5::sha224().verify_signature(
                &self.rsa_public_key()?.try_into()?,
                sig,
                plaintext,
            );
        } else if alg == &PKCS_1::SHA1WITHRSAENCRYPTION {
            check_null_params()?;
            return RSASSA_PKCS_v1_5::sha1().verify_signature(
                &self.rsa_public_key()?.try_into()?,
                sig,
                plaintext,
            );
        } else if alg == &PKCS_1::SHA256WITHRSAENCRYPTION {
            check_null_params()?;
            return RSASSA_PKCS_v1_5::sha256().verify_signature(
                &self.rsa_public_key()?.try_into()?,
                sig,
                plaintext,
            );
        } else if alg == &PKCS_1::SHA384WITHRSAENCRYPTION {
            check_null_params()?;
            return RSASSA_PKCS_v1_5::sha384().verify_signature(
                &self.rsa_public_key()?.try_into()?,
                sig,
                plaintext,
            );
        } else if alg == &PKCS_1::SHA512_224WITHRSAENCRYPTION {
            check_null_params()?;
            return RSASSA_PKCS_v1_5::sha512_224().verify_signature(
                &self.rsa_public_key()?.try_into()?,
                sig,
                plaintext,
            );
        } else if alg == &PKCS_1::SHA512_256WITHRSAENCRYPTION {
            check_null_params()?;
            return RSASSA_PKCS_v1_5::sha512_256().verify_signature(
                &self.rsa_public_key()?.try_into()?,
                sig,
                plaintext,
            );
        } else if alg == &PKCS_1::SHA512WITHRSAENCRYPTION {
            check_null_params()?;
            return RSASSA_PKCS_v1_5::sha512().verify_signature(
                &self.rsa_public_key()?.try_into()?,
                sig,
                plaintext,
            );
        } else if alg == &PKIX1Algorithms2008::ECDSA_WITH_SHA384 {
            check_null_params()?;
            let mut hasher = crate::sha384::SHA384Hasher::default();
            return check_ecdsa(&mut hasher);
        } else if alg == &PKIX1Algorithms2008::ECDSA_WITH_SHA256 {
            check_null_params()?;
            let mut hasher = crate::sha256::SHA256Hasher::default();
            return check_ecdsa(&mut hasher);
        }

        Err(format_err!("Unsupported signature algorithm {:?}", alg))
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
// TODO: Should use lazy_static
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
);

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Read;

    #[testcase]
    async fn x509_google_cert_test() -> Result<()> {
        let read_file = |path| -> Result<Arc<Certificate>> {
            let mut f = std::fs::File::open(path)?;

            let mut data = vec![];
            f.read_to_end(&mut data)?;

            let buf = Bytes::from(data);
            let cert = Certificate::read(buf)?;
            Ok(Arc::new(cert))
        };

        let cert = read_file(project_path!("testdata/x509/google.der")).unwrap();
        let cert2 = read_file(project_path!("testdata/x509/gts.der")).unwrap();

        let mut reg = CertificateRegistry::public_roots().await?;
        reg.append(&[cert, cert2], false)?;

        // let san = cert.subject_alt_name().unwrap().unwrap();

        // println!("{:#?}", cert);
        // println!("Authority: {:?}", cert.authority_key_id().unwrap());
        // println!("Subject: {:?}", cert.subject_key_id());
        // println!("{}", cert.issuer().to_string().unwrap());
        // println!("{}", cert.subject().to_string().unwrap());

        Ok(())
    }

    #[testcase]
    async fn x509_pem_test() -> Result<()> {
        let mut buf = file::read(project_path!("testdata/certificates/server.crt")).await?;

        let certs = Certificate::from_pem(buf.into())?;
        println!("{:#?}", certs);

        Ok(())
    }

    #[testcase]
    async fn x509_registry() -> Result<()> {
        CertificateRegistry::public_roots().await?;
        Ok(())
    }
}
