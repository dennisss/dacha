use alloc::string::String;
use common::bits::BitVector;
use core::convert::TryInto;

use asn::builtin::ObjectIdentifier;
use asn::builtin::{BitString, Null, OctetString};
use asn::encoding::{der_eq, Any, DERWriteable};
use common::bytes::Bytes;
use common::errors::*;
use pkix::{
    PKIX1Algorithms2008, PKIX1Algorithms88, PKIX1Explicit88, PKIX1_PSS_OAEP_Algorithms,
    Safecurves_pkix_18, PKCS_1,
};

use crate::elliptic::{EdwardsCurveGroup, EllipticCurveGroup};
use crate::hasher::Hasher;
use crate::rsa::{RSAPublicKey, RSASSA_PKCS_v1_5, RSASSA_PSS};
use crate::x509::signature_algorithm::DigitalSignatureAlgorithm;
use crate::x509::signature_key::*;

#[derive(Clone, Debug)]
pub enum PublicKey {
    RSA(RSAPublicKey),
    // If parameters are given in the public key, then the same parameters must be used to do
    // signatures (except the salt length must be >= the one in the public key).
    RSASSA_PSS(
        RSAPublicKey,
        Option<PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params>,
    ),
    EC(ObjectIdentifier, EllipticCurveGroup, Bytes),
    Ed25519(Bytes),
}

impl PublicKey {
    pub fn from_pem(data: Bytes) -> Result<Self> {
        todo!()
    }

    /// parent_key: Should be the public key of the CA certificate that signed
    /// the certificate in which this public key appears.
    pub fn from_asn1(
        pk: &PKIX1Explicit88::SubjectPublicKeyInfo,
        parent_key: Option<&Self>,
    ) -> Result<Self> {
        if pk.algorithm.algorithm == PKIX1Algorithms2008::RSAENCRYPTION {
            if !der_eq(
                &pk.algorithm.parameters,
                &Some(PKIX1_PSS_OAEP_Algorithms::NULLPARAMETERS),
            ) {
                return Err(err_msg("Expected RSA public key to have a null parameter"));
            }

            let data = &pk.subjectPublicKey.data;
            if data.len() % 8 != 0 {
                return Err(err_msg("Not complete bytes"));
            }

            // NOTE: PKCS_1::RSAPublicKey is basically the same as
            // PKIX1Algorithms2008::RSAPublicKey
            Ok(PublicKey::RSA(
                Any::from(Bytes::from(data.as_ref()))?
                    .parse_as::<PKCS_1::RSAPublicKey>()?
                    .try_into()?,
            ))
        } else if pk.algorithm.algorithm == PKIX1Algorithms2008::ID_ECPUBLICKEY {
            let params = match &pk.algorithm.parameters {
                Some(any) => any.parse_as::<PKIX1Algorithms88::EcpkParameters>()?,
                None => {
                    return Err(err_msg("No EC params specified"));
                }
            };

            let group_id;
            let group = match params {
                PKIX1Algorithms88::EcpkParameters::namedCurve(id) => {
                    group_id = id.clone();
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
                    let parent_key = parent_key.ok_or_else(|| err_msg("Unknown parent CA key"))?;
                    let (id, group, _) = parent_key.as_ec_key()?;
                    group_id = id.clone();
                    group.clone()
                }
                _ => {
                    return Err(err_msg("Unsupported curve format"));
                }
            };

            let point = PKIX1Algorithms2008::ECPoint::from(OctetString::from(
                pk.subjectPublicKey.data.as_ref(),
            ));

            Ok(Self::EC(
                group_id,
                group,
                // TODO: Check this?
                std::convert::Into::<OctetString>::into(point).into_bytes(),
            ))
        } else if pk.algorithm.algorithm == PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS {
            // See https://datatracker.ietf.org/doc/html/rfc4055.
            // Parameters are optional in the public key but must be present in signatures.
            // TODO: Use this.

            let mut params = None;
            if let Some(params_data) = pk.algorithm.parameters.as_ref() {
                params =
                    Some(params_data.parse_as::<PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params>()?);
            }

            let data = &pk.subjectPublicKey.data;
            let public_key: PKCS_1::RSAPublicKey =
                Any::from(Bytes::from(data.as_ref()))?.parse_as()?;

            Ok(Self::RSASSA_PSS(public_key.try_into()?, params))
        } else if pk.algorithm.algorithm == Safecurves_pkix_18::ID_ED25519 {
            if pk.algorithm.parameters.is_some() {
                return Err(err_msg(
                    "Ed25519 public key should not have any parameters.",
                ));
            }

            let data = &pk.subjectPublicKey.data;
            if data.len() % 8 != 0 {
                return Err(err_msg("Not complete bytes"));
            }

            // TODO: Check the size.

            Ok(Self::Ed25519(Bytes::from(data.as_ref())))
        } else {
            Err(format_err!(
                "Unsupported public key algorithm: {:?}",
                pk.algorithm
            ))
        }
    }

    pub fn to_pem(&self) -> String {
        todo!()
    }

    pub fn to_asn1(&self) -> PKIX1Explicit88::SubjectPublicKeyInfo {
        match self {
            PublicKey::RSA(key) => PKIX1Explicit88::SubjectPublicKeyInfo {
                algorithm: PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1Algorithms2008::RSAENCRYPTION,
                    parameters: Some(asn_any!(PKIX1_PSS_OAEP_Algorithms::NULLPARAMETERS)),
                },
                subjectPublicKey: BitString::from(BitVector::from_bytes(&key.to_asn1().to_der())),
            },
            PublicKey::RSASSA_PSS(_, _) => todo!(),
            PublicKey::EC(group_id, _, key) => PKIX1Explicit88::SubjectPublicKeyInfo {
                algorithm: PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1Algorithms2008::ID_ECPUBLICKEY,
                    parameters: Some(asn_any!(PKIX1Algorithms2008::ECParameters::namedCurve(
                        group_id.clone(),
                    ))),
                },
                subjectPublicKey: BitString::from(BitVector::from_bytes(&key)),
            },
            PublicKey::Ed25519(key) => PKIX1Explicit88::SubjectPublicKeyInfo {
                algorithm: PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: Safecurves_pkix_18::ID_ED25519,
                    parameters: None,
                },
                subjectPublicKey: BitString::from(BitVector::from_bytes(&key)),
            },
        }
    }

    /// For unknown/unsupported algorithms, this will return false.
    pub fn can_verify_signature(
        &self,
        signature_algorithm: &PKIX1Explicit88::AlgorithmIdentifier,
        constraints: &SignatureKeyConstraints,
    ) -> Result<bool> {
        let sk = match self {
            Self::RSA(_) => SignatureKeyParameters::RSA,
            Self::RSASSA_PSS(_, params) => SignatureKeyParameters::RSASSA_PSS(params.clone()),
            Self::EC(_, group, _) => SignatureKeyParameters::ECDSA(group.clone()),
            Self::Ed25519(_) => SignatureKeyParameters::Ed25519,
        };

        sk.can_use_with(signature_algorithm, constraints)
    }

    /// Will return an error if !self.can_verify_signature().
    pub fn verify_signature(
        &self,
        plaintext: &[u8],
        signature: &[u8],
        signature_algorithm: &PKIX1Explicit88::AlgorithmIdentifier,
        constraints: &SignatureKeyConstraints,
    ) -> Result<bool> {
        if !self.can_verify_signature(signature_algorithm, constraints)? {
            return Err(err_msg(
                "Signature algorithm not compatible with the public key",
            ));
        }

        match DigitalSignatureAlgorithm::create(signature_algorithm)? {
            DigitalSignatureAlgorithm::RSASSA_PKCS_v1_5(rsa) => {
                return rsa.verify_signature(self.as_rsa_key()?, signature, plaintext);
            }
            DigitalSignatureAlgorithm::RSASSA_PSS(rsa) => {
                return rsa.verify_signature(self.as_rsa_key()?, signature, plaintext);
            }
            DigitalSignatureAlgorithm::Ed25519(group) => {
                return group.verify_signature(self.as_ed25519_key()?, signature, plaintext);
            }
            DigitalSignatureAlgorithm::EcDSA(hasher_factory) => {
                let mut hasher = hasher_factory.create();
                let (_, group, point) = self.as_ec_key()?;
                return group.verify_signature(
                    point.as_ref(),
                    signature,
                    constraints
                        .ecdsa_signature_format
                        .unwrap_or(crate::elliptic::EllipticCurveSignatureFormat::X509),
                    plaintext,
                    hasher.as_mut(),
                );
            }
        }
    }

    fn as_ec_key(&self) -> Result<(&ObjectIdentifier, &EllipticCurveGroup, &Bytes)> {
        match self {
            Self::EC(a, b, c) => Ok((a, b, c)),
            _ => Err(err_msg("Expected an EC public key")),
        }
    }

    fn as_ed25519_key(&self) -> Result<&[u8]> {
        match self {
            Self::Ed25519(v) => Ok(v.as_ref()),
            _ => Err(err_msg("Expected an Ed25519 public key")),
        }
    }

    fn as_rsa_key(&self) -> Result<&RSAPublicKey> {
        match self {
            Self::RSA(v) => Ok(v),
            Self::RSASSA_PSS(v, _) => Ok(v),
            _ => Err(err_msg("Expected an RSA public key")),
        }
    }
}
