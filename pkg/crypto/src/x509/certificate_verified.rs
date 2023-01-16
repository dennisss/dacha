use core::ops::Deref;
use std::sync::Arc;

use common::errors::*;
use pkix::PKIX1Implicit88;

use crate::x509::name_constraints::NameConstraints;
use crate::x509::Certificate;

use super::CertificateRegistry;

/// If true, we won't perform signature verification on self-signed
/// certificates (this will save startup time if loading a lot of CAs).
const SKIP_SELF_SIGNED_VERIFICATION: bool = false;

/// A certificate which has been 'verified' implying that all constrains imposed
/// by a parent certificate are satified by this certificate.
///
/// This also stores inherited constraints from parent certificates which will
/// apply to children of this certificate.
pub struct CertificateVerified {
    certificate: Arc<Certificate>,

    /// Maximum number of certificates that can appear below this certificate in
    /// a chain.
    ///
    /// - This if off-by-one compared to the pathLenConstraint in the basic
    ///   constraints.
    /// - 0 means that if this certificate can't actually sign any child
    ///   certificates.
    /// - This value already includes the minimum value from parent
    ///   certificates.
    max_path_length: Option<usize>,

    // parent_name_constraints: Option<Arc<NameConstraints>>,
    /// Contraints placed on the names used by this and child certificates.
    name_constraints: Option<Arc<NameConstraints>>,
}

impl Deref for CertificateVerified {
    type Target = Certificate;

    fn deref(&self) -> &Self::Target {
        &self.certificate
    }
}

impl CertificateVerified {
    /// Assuming we can trust the certificate, verifies a self-signed
    /// certificate.
    pub fn verify_self_signed(
        certificate: Arc<Certificate>,
        registry: &CertificateRegistry,
    ) -> Result<Self> {
        if !certificate.self_signed() {
            return Err(err_msg("Not self signed"));
        }

        if !SKIP_SELF_SIGNED_VERIFICATION
            && !certificate.verify_child_signature(&certificate, registry)?
        {
            return Err(err_msg("Self-signed signature invalid"));
        }

        let inst = Self::create_raw(certificate)?;

        Self::verify_names(&inst.certificate, &inst.name_constraints)?;

        Ok(inst)
    }

    /// Interprets the current signature as a CA and validates the given child
    /// certificate.
    pub fn verify_child(
        &self,
        child: Arc<Certificate>,
        registry: &CertificateRegistry,
    ) -> Result<Self> {
        if self.validity.not_before > child.validity.not_before
            || self.validity.not_after < child.validity.not_after
        {
            return Err(err_msg("Child certificate outlives parent"));
        }

        if !self.certificate.verify_child_signature(&child, registry)? {
            return Err(err_msg("Child certificate signature invalid"));
        }

        let mut inst = Self::create_raw(child)?;
        let is_self_issued = inst.self_issued();

        // Inherit max_path_length from parent.
        if let Some(max_len) = self.max_path_length.clone() {
            if max_len == 0 {
                return Err(err_msg("Certificate exceeded max path length"));
            }

            let max_child_len = if is_self_issued { max_len } else { max_len - 1 };

            inst.max_path_length = Some(
                inst.max_path_length
                    .unwrap_or(usize::MAX)
                    .min(max_child_len),
            );
        }

        // Inherit name constraints.
        Self::verify_names(&inst.certificate, &self.name_constraints)?;
        if let Some(constraints) = &self.name_constraints {
            if let Some(child_constraints) = &mut inst.name_constraints {
                Arc::get_mut(child_constraints)
                    .unwrap()
                    .inherit(constraints.as_ref());
            } else {
                inst.name_constraints = self.name_constraints.clone();
            }
        }

        Ok(inst)
    }

    /// Raw creation of a CertificateVerified struct for a single certificate
    /// (ignores parent constraints).
    fn create_raw(certificate: Arc<Certificate>) -> Result<Self> {
        let mut name_constraints = None;
        if let Some(value) = certificate.name_constraints()? {
            name_constraints = Some(Arc::new(NameConstraints::create(&value)?));
        }

        let mut max_path_length = None;
        if let Some(constraints) = certificate.basic_constraints()? {
            if let Some(len) = constraints.pathLenConstraint {
                let len = len.to_isize()?;
                if len < 0 {
                    return Err(err_msg("Certificate has a negative path len constraint"));
                }

                max_path_length = Some((len + 1) as usize);
            }
        }

        Ok(Self {
            certificate,
            name_constraints,
            max_path_length,
        })
    }

    /// Verifies that the names specified in a certificate are valid based on
    /// the constraints of the parent certificate.
    ///
    /// TODO: Technically we should only verify this for self-issued
    /// certificates if the certificate is used as the final certificate in a
    /// chain, but we currently don't provide a way to indicate whether a
    /// certificate is 'final', so we just validate it always.
    fn verify_names(cert: &Certificate, constraints: &Option<Arc<NameConstraints>>) -> Result<()> {
        let constraints = match constraints {
            Some(v) => v,
            None => return Ok(()),
        };

        if let Some(names) = cert.subject_alt_name()? {
            for name in &names.items {
                if let PKIX1Implicit88::GeneralName::dNSName(s) = name {
                    if !constraints.is_allowed_dns_name(s.data.as_str()) {
                        return Err(format_err!(
                            "DNS name not allowed by certificate name constraints: {}",
                            s.data.as_str()
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}
