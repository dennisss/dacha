use asn::{builtin::ObjectIdentifier, encoding::der_eq};
use common::errors::*;
use pkix::{
    PKIX1Algorithms2008, PKIX1Explicit88, PKIX1_PSS_OAEP_Algorithms, Safecurves_pkix_18, PKCS_1,
};

use crate::elliptic::EllipticCurveGroup;

/// Helper for representing the shared parameters between a public/private key.
pub enum SignatureKeyParameters {
    RSA,

    // TODO: Refuse to use any suggested parameters here if they are insecure.
    RSASSA_PSS(Option<PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params>),

    ECDSA(EllipticCurveGroup),

    Ed25519,
}

///
#[derive(Default)]
pub struct SignatureKeyConstraints {
    /// When using an ECDSA key, this must be the group that it is using.
    pub ecdsa_group: Option<EllipticCurveGroup>,

    pub key_oid: Option<ObjectIdentifier>,
}

impl SignatureKeyParameters {
    /// Determines if the key with these parameters can be used to sign/verify a
    /// signature with the given algorithm.
    ///
    /// TODO: Implement this using the DigitalSignatureAlgorithm class?
    ///
    /// For unknown/unsupported algorithms, this will return false.
    pub fn can_use_with(
        &self,
        signature_algorithm: &PKIX1Explicit88::AlgorithmIdentifier,
        constraints: &SignatureKeyConstraints,
    ) -> Result<bool> {
        if let Some(target_key_oid) = &constraints.key_oid {
            let key_oid = match self {
                SignatureKeyParameters::RSA => PKIX1Algorithms2008::RSAENCRYPTION,
                SignatureKeyParameters::RSASSA_PSS(_) => PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS,
                SignatureKeyParameters::ECDSA(_) => PKIX1Algorithms2008::ID_ECPUBLICKEY,
                SignatureKeyParameters::Ed25519 => Safecurves_pkix_18::ID_ED25519,
            };

            if key_oid != *target_key_oid {
                return Ok(false);
            }
        }

        let supported_algorithm = match self {
            Self::RSA => {
                signature_algorithm.algorithm == PKIX1_PSS_OAEP_Algorithms::SHA224WITHRSAENCRYPTION
                    || signature_algorithm.algorithm == PKCS_1::SHA1WITHRSAENCRYPTION
                    || signature_algorithm.algorithm == PKCS_1::SHA256WITHRSAENCRYPTION
                    || signature_algorithm.algorithm == PKCS_1::SHA384WITHRSAENCRYPTION
                    || signature_algorithm.algorithm == PKCS_1::SHA512_224WITHRSAENCRYPTION
                    || signature_algorithm.algorithm == PKCS_1::SHA512_256WITHRSAENCRYPTION
                    || signature_algorithm.algorithm == PKCS_1::SHA512WITHRSAENCRYPTION
                    || signature_algorithm.algorithm == PKCS_1::SHA512WITHRSAENCRYPTION
                    || signature_algorithm.algorithm == PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS
            }
            Self::RSASSA_PSS(params) => {
                let valid =
                    signature_algorithm.algorithm == PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS;
                if !valid {
                    return Ok(false);
                }

                if let Some(key_params) = params {
                    let target_params = signature_algorithm
                        .parameters
                        .as_ref()
                        .ok_or_else(|| {
                            err_msg("RSASSA-PSS signature algorithm must specify parameters")
                        })?
                        .parse_as::<PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params>()?;

                    if !der_eq(&key_params.hashAlgorithm, &target_params.hashAlgorithm)
                        || !der_eq(
                            &key_params.maskGenAlgorithm,
                            &target_params.maskGenAlgorithm,
                        )
                        || key_params.trailerField != target_params.trailerField
                        || key_params.saltLength.to_isize()?
                            > target_params.saltLength.to_isize()?
                    {
                        return Ok(false);
                    }
                }

                true
            }
            Self::ECDSA(group) => {
                let valid = signature_algorithm.algorithm == PKIX1Algorithms2008::ECDSA_WITH_SHA256
                    || signature_algorithm.algorithm == PKIX1Algorithms2008::ECDSA_WITH_SHA384
                    || signature_algorithm.algorithm == PKIX1Algorithms2008::ECDSA_WITH_SHA512;
                if !valid {
                    return Ok(false);
                }

                if let Some(target_group) = &constraints.ecdsa_group {
                    if target_group != group {
                        return Ok(false);
                    }
                }

                true
            }
            Self::Ed25519 => signature_algorithm.algorithm == Safecurves_pkix_18::ID_ED25519,
        };

        if !supported_algorithm {
            return Ok(false);
        }

        Ok(true)
    }
}
