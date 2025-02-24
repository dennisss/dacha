use std::collections::HashMap;
use std::convert::{AsRef, TryFrom, TryInto};
use std::string::String;
use std::string::ToString;
use std::sync::Arc;
use std::vec::Vec;

use asn::encoding::{der_eq, Any, DERReadable, DERReader, DERWriteable};
use common::bytes::Bytes;
use common::chrono::{DateTime, Utc};
use common::errors::*;
use common::failure::ResultExt;
use pkix::{
    PKIX1Algorithms2008, PKIX1Algorithms88, PKIX1Explicit88, PKIX1Implicit88,
    PKIX1_PSS_OAEP_Algorithms, NIST_SHA2, PKCS_1,
};

use crate::x509::certificate::Certificate;
use crate::x509::certificate_verified::CertificateVerified;

/// Wrapper around a PKIX1Explicit88::Name which can be compared and is hashable
/// so can be used as a key in a map.
/// NOTE: all internal properties are immutable.
#[derive(PartialEq, Eq, Hash)]
struct NameKey {
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
    /// NOTE: Certificates are only added to this once all its parents are added
    /// so this should never contain a cycle.
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
    ///
    /// TODO: Cache this and return an immutable Arc<>
    pub async fn public_roots() -> Result<Self> {
        let mut data = file::read(project_path!("third_party/chromium/root_store.bin")).await?;

        let buf = Bytes::from(data);

        let mut certs = vec![];
        let mut i = 0;
        while i < buf.len() {
            let len = u32::from_le_bytes(*array_ref![&buf, i, 4]) as usize;
            i += 4;

            let data = buf.slice(i..(i + len));
            i += len;

            let cert = Certificate::read(data)
                .with_context(|e| format!("While parsing certificate: {}", e))?;
            certs.push(Arc::new(cert));
        }

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
    /// Will return None for self-signed certificates.
    ///
    /// We assume that the 'issuer' in the certificate is the same as the issuer
    /// in the AuthorityKeyIdentifier (if present).
    pub(crate) fn lookup_parent(
        &self,
        cert: &Certificate,
    ) -> Result<Option<Arc<CertificateVerified>>> {
        if let Some(parent) = &self.parent {
            if let Some(v) = parent.lookup_parent(cert)? {
                return Ok(Some(v));
            }
        }

        if cert.self_signed() {
            return Ok(None);
        }

        let issuer = NameKey::from(&cert.raw.tbsCertificate.issuer);
        let certs = match self.certs.get(&issuer) {
            Some(list) => list,
            None => {
                return Ok(None);
            }
        };

        // NOTE: Per https://www.rfc-editor.org/rfc/rfc4158#section-3.5.12, the authory key id extensions should only be used for hinting at the parent certificate, through we require the exact key id to the present in the child and optionally have the serial number.

        // NOTE: One issuer may have multipl certificates
        for c in certs {
            if cert.authority_key_id() != c.subject_key_id() {
                continue;
            }

            if let Some(authority_serial_number) = cert.authority_serial_number() {
                if authority_serial_number != c.serial_number() {
                    continue;
                }
            }

            return Ok(Some(c.clone()));
        }

        Ok(None)
    }

    // TODO: Need to perform an exact comparison to be sure.
    fn contains(&self, cert: &Certificate) -> Result<bool> {
        let list = self
            .certs
            .get(&NameKey::from(&cert.raw.tbsCertificate.subject))
            .map(|v| &v[..])
            .unwrap_or(&[]);

        for c2 in list.iter() {
            if cert.subject_key_id() != c2.subject_key_id() {
                continue;
            }

            if cert.serial_number() != c2.serial_number() {
                continue;
            }

            if !der_eq(&cert.raw, &c2.raw) {
                return Err(err_msg(
                    "Registry contains different data for same certificate.",
                ));
            }

            return Ok(true);
        }

        if let Some(parent) = &self.parent {
            return parent.contains(cert);
        }

        Ok(false)
    }

    /// Performs insertion into the inner certificate map. This assumes that the
    /// certificate chain has already been verified and that the certificate is
    /// NOT already in the registry.
    ///
    /// A certificate can only be inserted if there is no other certificate with
    /// the same (issuer, serial number) or (issuer, subject key id) pair.
    ///
    /// Returns whether or not it was inserted newly. If false, then an
    /// identical certificate already existed in the registry with the exact
    /// same contents.
    /// TODO: Implement allowing exact matches.
    fn insert(&mut self, cert: Arc<CertificateVerified>) -> Result<()> {
        let c = cert.as_ref();

        // Already checked in append().
        // if self.contains(c)? {
        //     return Ok(());
        // }

        let list = self
            .certs
            .entry(NameKey::from(&c.raw.tbsCertificate.subject))
            .or_insert(vec![]);

        list.push(cert);
        Ok(())
    }

    /// Adds all of the given certificates to the registry.
    ///
    /// NOTE: This is currently O(n*k) where n is the number of certificates
    /// given and k is the length of the chain in the given certificates.
    ///
    /// TODO: If the user passes in a certificate already in the registry,
    /// deduplicate the memory pointers between them.
    pub fn append(&mut self, certs: &[Arc<Certificate>], trusted: bool) -> Result<()> {
        let mut remaining = certs.to_vec();
        while remaining.len() > 0 {
            let mut changed = false;
            for raw_cert in remaining.split_off(0) {
                // No need to verify the signature if we have an exact match to the certificate
                // in our registry.
                if self.contains(&raw_cert)? {
                    changed = true;
                    continue;
                }

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
