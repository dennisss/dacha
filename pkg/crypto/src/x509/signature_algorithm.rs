use asn::builtin::{BitString, Null, OctetString};
use asn::encoding::{der_eq, Any};
use common::errors::*;
use pkix::{
    PKIX1Algorithms2008, PKIX1Explicit88, PKIX1_PSS_OAEP_Algorithms, Safecurves_pkix_18, PKCS_1,
};

use crate::elliptic::EdwardsCurveGroup;
use crate::hasher::{GetHasherFactory, HasherFactory};
use crate::rsa::{RSASSA_PKCS_v1_5, RSASSA_PSS};
use crate::sha256::SHA256Hasher;
use crate::sha384::SHA384Hasher;
use crate::sha512::SHA512Hasher;

pub enum DigitalSignatureAlgorithm {
    RSASSA_PKCS_v1_5(RSASSA_PKCS_v1_5),
    RSASSA_PSS(RSASSA_PSS),
    Ed25519(EdwardsCurveGroup),
    EcDSA(HasherFactory),
}

impl DigitalSignatureAlgorithm {
    pub fn create(signature_algorithm: &PKIX1Explicit88::AlgorithmIdentifier) -> Result<Self> {
        // NOTE: Most standards require this to be strictly missing but we also tolerate
        // null values.
        let check_null_params = || -> Result<()> {
            if signature_algorithm.parameters.is_some()
                && !der_eq(&signature_algorithm.parameters, &Null::new())
            {
                return Err(err_msg("Expected null params for algorithm"));
            }
            Ok(())
        };

        let alg = &signature_algorithm.algorithm;
        if alg == &PKCS_1::SHA1WITHRSAENCRYPTION {
            check_null_params()?;
            return Ok(Self::RSASSA_PKCS_v1_5(RSASSA_PKCS_v1_5::sha1()));
        } else if alg == &PKIX1_PSS_OAEP_Algorithms::SHA224WITHRSAENCRYPTION {
            check_null_params()?;
            return Ok(Self::RSASSA_PKCS_v1_5(RSASSA_PKCS_v1_5::sha224()));
        } else if alg == &PKCS_1::SHA256WITHRSAENCRYPTION {
            check_null_params()?;
            return Ok(Self::RSASSA_PKCS_v1_5(RSASSA_PKCS_v1_5::sha256()));
        } else if alg == &PKCS_1::SHA384WITHRSAENCRYPTION {
            check_null_params()?;
            return Ok(Self::RSASSA_PKCS_v1_5(RSASSA_PKCS_v1_5::sha384()));
        } else if alg == &PKCS_1::SHA512_224WITHRSAENCRYPTION {
            check_null_params()?;
            return Ok(Self::RSASSA_PKCS_v1_5(RSASSA_PKCS_v1_5::sha512_224()));
        } else if alg == &PKCS_1::SHA512_256WITHRSAENCRYPTION {
            check_null_params()?;
            return Ok(Self::RSASSA_PKCS_v1_5(RSASSA_PKCS_v1_5::sha512_256()));
        } else if alg == &PKCS_1::SHA512WITHRSAENCRYPTION {
            check_null_params()?;
            return Ok(Self::RSASSA_PKCS_v1_5(RSASSA_PKCS_v1_5::sha512()));
        } else if alg == &PKIX1Algorithms2008::ECDSA_WITH_SHA256 {
            check_null_params()?;
            return Ok(Self::EcDSA((SHA256Hasher::factory())));
        } else if alg == &PKIX1Algorithms2008::ECDSA_WITH_SHA384 {
            check_null_params()?;
            return Ok(Self::EcDSA((SHA384Hasher::factory())));
        } else if alg == &PKIX1Algorithms2008::ECDSA_WITH_SHA512 {
            check_null_params()?;
            return Ok(Self::EcDSA((SHA512Hasher::factory())));
        } else if alg == &Safecurves_pkix_18::ID_ED25519 {
            check_null_params()?;
            return Ok(Self::Ed25519(EdwardsCurveGroup::ed25519()));
        } else if alg == &PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS {
            let params = signature_algorithm
                .parameters
                .as_ref()
                .ok_or_else(|| err_msg("Missing parameters for RSASSA-PSS signature algorithm"))?
                .parse_as::<PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params>()?;

            // This is an enum where '1' equals 0xBC which is the standard trailer byte
            // value.
            if !params.trailerField.is_one() {
                return Err(err_msg("Unsupported trailer field"));
            }

            let salt_length = params.saltLength.to_isize()?;
            if salt_length < 0 || salt_length > 1024 {
                return Err(err_msg("Invalid salt length"));
            }

            let (hasher_factory, expected_mask_algorithm) = {
                if der_eq(
                    &params.hashAlgorithm,
                    &*PKIX1_PSS_OAEP_Algorithms::SHA256IDENTIFIER,
                ) {
                    (
                        SHA256Hasher::factory(),
                        &*PKIX1_PSS_OAEP_Algorithms::MGF1SHA256IDENTIFIER,
                    )
                } else if der_eq(
                    &params.hashAlgorithm,
                    &*PKIX1_PSS_OAEP_Algorithms::SHA384IDENTIFIER,
                ) {
                    (
                        SHA384Hasher::factory(),
                        &*PKIX1_PSS_OAEP_Algorithms::MGF1SHA384IDENTIFIER,
                    )
                } else if der_eq(
                    &params.hashAlgorithm,
                    &*PKIX1_PSS_OAEP_Algorithms::SHA512IDENTIFIER,
                ) {
                    (
                        SHA512Hasher::factory(),
                        &*PKIX1_PSS_OAEP_Algorithms::MGF1SHA512IDENTIFIER,
                    )
                } else {
                    return Err(format_err!(
                        "Unsupported hashing algorithm: {:?}",
                        params.hashAlgorithm
                    ));
                }
            };

            if !der_eq(expected_mask_algorithm, &params.maskGenAlgorithm) {
                return Err(err_msg("Mismatching mask algorithm"));
            }

            return Ok(Self::RSASSA_PSS(RSASSA_PSS::new(
                hasher_factory,
                salt_length as usize,
            )));
        }

        Err(format_err!("Unsupported signature algorithm {:?}", alg))
    }
}
