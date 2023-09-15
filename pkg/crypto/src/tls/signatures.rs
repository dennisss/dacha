// Utilities for working with X509 public/private keys to verify/create
// signatures for certificate verification.

use asn::encoding::Any;
use asn::encoding::DERWriteable;
use pkix::{
    PKIX1Algorithms2008, PKIX1Explicit88, PKIX1_PSS_OAEP_Algorithms, Safecurves_pkix_18, PKCS_1,
};

use crate::{
    elliptic::EllipticCurveGroup, tls::extensions::SignatureScheme, x509::SignatureKeyConstraints,
};

impl SignatureScheme {
    /// Translates a TLS SignatureSchema into the corresponding X509
    /// id/constraints for creating/verifying signatures backed by
    /// certificate keys.
    ///
    /// TODO: Need to verify that every algorithm id mentioned here is actually
    /// supported by the x509 code.
    pub fn to_x509_signature_id(
        &self,
    ) -> Option<(
        PKIX1Explicit88::AlgorithmIdentifier,
        SignatureKeyConstraints,
    )> {
        let mut constraints = SignatureKeyConstraints::default();

        let id = match self {
            SignatureScheme::rsa_pkcs1_sha256 => PKIX1Explicit88::AlgorithmIdentifier {
                algorithm: PKCS_1::SHA256WITHRSAENCRYPTION,
                parameters: None,
            },
            SignatureScheme::rsa_pkcs1_sha384 => PKIX1Explicit88::AlgorithmIdentifier {
                algorithm: PKCS_1::SHA384WITHRSAENCRYPTION,
                parameters: None,
            },
            SignatureScheme::rsa_pkcs1_sha512 => PKIX1Explicit88::AlgorithmIdentifier {
                algorithm: PKCS_1::SHA512WITHRSAENCRYPTION,
                parameters: None,
            },
            SignatureScheme::ecdsa_secp256r1_sha256 => {
                constraints.ecdsa_group = Some(EllipticCurveGroup::secp256r1());
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1Algorithms2008::ECDSA_WITH_SHA256,
                    parameters: None,
                }
            }
            SignatureScheme::ecdsa_secp384r1_sha384 => {
                constraints.ecdsa_group = Some(EllipticCurveGroup::secp384r1());
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1Algorithms2008::ECDSA_WITH_SHA384,
                    parameters: None,
                }
            }
            SignatureScheme::ecdsa_secp521r1_sha512 => {
                constraints.ecdsa_group = Some(EllipticCurveGroup::secp521r1());
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1Algorithms2008::ECDSA_WITH_SHA512,
                    parameters: None,
                }
            }
            // TODO: Deduplicate these cases a bit.
            SignatureScheme::rsa_pss_rsae_sha256 => {
                // NOTE: Salt length should be the same as the digest/hash length.
                constraints.key_oid = Some(PKIX1Algorithms2008::RSAENCRYPTION);
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS,
                    parameters: Some(asn_any!(PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params {
                        hashAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::SHA256IDENTIFIER)
                            .clone()
                            .into(),
                        maskGenAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::MGF1SHA256IDENTIFIER)
                            .clone()
                            .into(),
                        saltLength: (256 / 8).into(),
                        trailerField: 1.into(),
                    })),
                }
            }
            SignatureScheme::rsa_pss_rsae_sha384 => {
                // NOTE: Salt length should be the same as the digest/hash length.
                constraints.key_oid = Some(PKIX1Algorithms2008::RSAENCRYPTION);
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS,
                    parameters: Some(asn_any!(PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params {
                        hashAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::SHA384IDENTIFIER)
                            .clone()
                            .into(),
                        maskGenAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::MGF1SHA384IDENTIFIER)
                            .clone()
                            .into(),
                        saltLength: (384 / 8).into(),
                        trailerField: 1.into(),
                    })),
                }
            }
            SignatureScheme::rsa_pss_rsae_sha512 => {
                // NOTE: Salt length should be the same as the digest/hash length.
                constraints.key_oid = Some(PKIX1Algorithms2008::RSAENCRYPTION);
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS,
                    parameters: Some(asn_any!(PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params {
                        hashAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::SHA512IDENTIFIER)
                            .clone()
                            .into(),
                        maskGenAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::MGF1SHA512IDENTIFIER)
                            .clone()
                            .into(),
                        saltLength: (512 / 8).into(),
                        trailerField: 1.into(),
                    })),
                }
            }
            SignatureScheme::rsa_pss_pss_sha256 => {
                // NOTE: Salt length should be the same as the digest/hash length.
                constraints.key_oid = Some(PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS);
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS,
                    parameters: Some(asn_any!(PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params {
                        hashAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::SHA256IDENTIFIER)
                            .clone()
                            .into(),
                        maskGenAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::MGF1SHA256IDENTIFIER)
                            .clone()
                            .into(),
                        saltLength: (256 / 8).into(),
                        trailerField: 1.into(),
                    })),
                }
            }
            SignatureScheme::rsa_pss_pss_sha384 => {
                // NOTE: Salt length should be the same as the digest/hash length.
                constraints.key_oid = Some(PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS);
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS,
                    parameters: Some(asn_any!(PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params {
                        hashAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::SHA384IDENTIFIER)
                            .clone()
                            .into(),
                        maskGenAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::MGF1SHA384IDENTIFIER)
                            .clone()
                            .into(),
                        saltLength: (384 / 8).into(),
                        trailerField: 1.into(),
                    })),
                }
            }
            SignatureScheme::rsa_pss_pss_sha512 => {
                // NOTE: Salt length should be the same as the digest/hash length.
                constraints.key_oid = Some(PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS);
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS,
                    parameters: Some(asn_any!(PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params {
                        hashAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::SHA512IDENTIFIER)
                            .clone()
                            .into(),
                        maskGenAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::MGF1SHA512IDENTIFIER)
                            .clone()
                            .into(),
                        saltLength: (512 / 8).into(),
                        trailerField: 1.into(),
                    })),
                }
            }
            SignatureScheme::ed25519 => PKIX1Explicit88::AlgorithmIdentifier {
                algorithm: Safecurves_pkix_18::ID_ED25519,
                parameters: None,
            },
            SignatureScheme::ed448 => PKIX1Explicit88::AlgorithmIdentifier {
                algorithm: Safecurves_pkix_18::ID_ED448,
                parameters: None,
            },

            SignatureScheme::rsa_pkcs1_sha1 => return None,
            SignatureScheme::ecdsa_sha1 => return None,
            SignatureScheme::Unknown(_) => return None,
        };

        Some((id, constraints))
    }
}
/*

*/
