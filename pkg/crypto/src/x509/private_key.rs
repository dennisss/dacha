use alloc::string::String;
use alloc::vec::Vec;
use std::convert::TryInto;

use asn::builtin::{Null, ObjectIdentifier, OctetString};
use asn::encoding::{der_eq, DERReadable, DERWriteable};
use common::bytes::Bytes;
use common::errors::*;
use pkix::{PKIX1Algorithms2008, PKIX1Explicit88, PKIX1_PSS_OAEP_Algorithms};
use pkix::{PKIX1Algorithms88, PKCS_8};
use pkix::{Safecurves_pkix_18, PKCS_1};

use crate::dh::DiffieHellmanFn;
use crate::elliptic::{EdwardsCurveGroup, EllipticCurveGroup};
use crate::pem::{PEMBuilder, PEM, PEM_PRIVATE_KEY_LABEL};
use crate::rsa::RSAPrivateKey;
use crate::x509::signature_algorithm::*;
use crate::x509::signature_key::*;
use crate::x509::PublicKey;

pub enum PrivateKeyType {
    Ed25519,
    ECDSA_SECP256R1,
}

#[derive(Debug, Clone)]
pub enum PrivateKey {
    RSA(RSAPrivateKey),

    RSASSA_PSS(
        RSAPrivateKey,
        Option<PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params>,
    ),

    /// (GroupId, Group, Key)
    ECDSA(ObjectIdentifier, EllipticCurveGroup, Bytes),

    Ed25519(Bytes),
}

impl PrivateKey {
    /// Uses default parameters to generate a private key.
    pub async fn generate_default() -> Result<Self> {
        Self::generate(PrivateKeyType::Ed25519).await
    }

    pub async fn generate(typ: PrivateKeyType) -> Result<Self> {
        Ok(match typ {
            PrivateKeyType::Ed25519 => Self::Ed25519(
                EdwardsCurveGroup::ed25519()
                    .generate_private_key()
                    .await
                    .into(),
            ),
            PrivateKeyType::ECDSA_SECP256R1 => {
                let id = PKIX1Algorithms2008::SECP256R1;
                let group = EllipticCurveGroup::secp256r1();
                let key = group.generate_private_key().await;
                Self::ECDSA(id, group, key.into())
            }
        })
    }

    pub fn from_pem(data: Bytes) -> Result<Self> {
        let mut pem = PEM::parse(data)?;
        if pem.entries.len() != 1 {
            return Err(err_msg("Wrong number of private keys in PEM"));
        }

        let entry = pem.entries.pop().unwrap();

        if entry.label.as_str() == PEM_PRIVATE_KEY_LABEL {
            let pkey_info = pkix::PKCS_8::PrivateKeyInfo::from_der(entry.to_binary()?.into())?;

            let check_null_params = || -> Result<()> {
                if !der_eq(&pkey_info.privateKeyAlgorithm.parameters, &Null::new()) {
                    return Err(err_msg("Expected null params for algorithm"));
                }
                Ok(())
            };

            // TODO: Check version.

            if pkey_info.privateKeyAlgorithm.algorithm == PKCS_1::RSAENCRYPTION {
                check_null_params()?;
                let pkey = PKCS_1::RSAPrivateKey::from_der(pkey_info.privateKey.to_bytes())?;
                return Ok(Self::RSA((&pkey).try_into()?));
            } else if pkey_info.privateKeyAlgorithm.algorithm == PKIX1Algorithms2008::ID_ECPUBLICKEY
            {
                // TODO: Deduplicate this logic with the ec_public_key logic which is basically
                // identical.

                let params = match &pkey_info.privateKeyAlgorithm.parameters {
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
                        return Err(err_msg(
                            "Don't support loading PEM private key using implicitlyCA params",
                        ));
                    }
                    _ => {
                        return Err(err_msg("Unsupported curve format"));
                    }
                };

                // TODO: Verify that the parameters in the ECPrivateKEy match the ones in the
                // privateKeyAlgorithm.
                let key = PKIX1Algorithms2008::ECPrivateKey::from_der(
                    Into::<OctetString>::into(pkey_info.privateKey.clone()).to_bytes(),
                )?
                .privateKey
                .to_bytes();

                /*
                println!("{:#?}", ppkey);

                // TODO: Reduce the number of conversions needed for this.
                let point = PKIX1Algorithms2008::ECPoint::from(Into::<OctetString>::into(
                    pkey_info.privateKey.clone(),
                ));
                */

                return Ok(Self::ECDSA(group_id, group, key));
            } else if pkey_info.privateKeyAlgorithm.algorithm
                == pkix::Safecurves_pkix_18::ID_ED25519
            {
                if !pkey_info.privateKeyAlgorithm.parameters.is_none() {
                    return Err(err_msg("Expected params to be absent for safecurves"));
                }

                let key: OctetString = pkix::Safecurves_pkix_18::CurvePrivateKey::from_der(
                    Into::<OctetString>::into(pkey_info.privateKey.clone()).to_bytes(),
                )?
                .into();

                if key.len() != 32 {
                    return Err(err_msg("Wrong length of Ed25519 private key"));
                }

                return Ok(Self::Ed25519(key.into_bytes()));
            } else {
                return Err(format_err!(
                    "Unsupported private key algorithm: {:?}",
                    pkey_info.privateKeyAlgorithm.algorithm
                ));
            }
        } else {
            return Err(format_err!(
                "Unsupported PEM label for private key: {}",
                entry.label.as_str()
            ));
        }

        // println!("{}", entry.label.as_ref());
        // let data = entry.to_binary()?.into();

        // println!("{:#?}", pkey);

        // asn::debug::print_debug_string(data);
    }

    pub fn to_pem(&self) -> String {
        PEMBuilder::default()
            .add_binary_entry(PEM_PRIVATE_KEY_LABEL, &self.to_asn1().to_der())
            .build()
    }

    pub fn to_asn1(&self) -> pkix::PKCS_8::PrivateKeyInfo {
        match self {
            PrivateKey::RSA(_) => todo!(),
            PrivateKey::RSASSA_PSS(_, _) => todo!(),
            PrivateKey::ECDSA(group_id, group, key) => {
                // See
                // https://datatracker.ietf.org/doc/html/rfc5915

                // NOTE: It's a bit inconclusive as to whether or not we should put the
                // parameters in privateKeyAlgorithm or ECPrivateKey but for safety, we put them
                // in both.

                let private_key = PKIX1Algorithms2008::ECPrivateKey {
                    version: PKIX1Algorithms2008::ecprivatekey::Version::ecPrivkeyVer1,
                    privateKey: OctetString(asn::builtin::BytesRef::Dynamic(key.clone())),
                    parameters: Some(PKIX1Algorithms2008::ECParameters::namedCurve(
                        group_id.clone(),
                    )),
                    // TODO:
                    publicKey: None,
                };

                pkix::PKCS_8::PrivateKeyInfo {
                    version: pkix::PKCS_8::Version::v1,
                    privateKeyAlgorithm: PKIX1Explicit88::AlgorithmIdentifier {
                        algorithm: PKIX1Algorithms2008::ID_ECPUBLICKEY,
                        parameters: Some(asn_any!(PKIX1Algorithms88::EcpkParameters::namedCurve(
                            group_id.clone()
                        ))),
                    },
                    privateKey: PKCS_8::PrivateKey::from(OctetString(
                        asn::builtin::BytesRef::Dynamic(private_key.to_der().into()),
                    )),
                }
            }
            PrivateKey::Ed25519(private_key) => {
                let key = pkix::Safecurves_pkix_18::CurvePrivateKey::from(OctetString(
                    asn::builtin::BytesRef::Dynamic(private_key.clone()),
                ))
                .to_der();

                pkix::PKCS_8::PrivateKeyInfo {
                    version: pkix::PKCS_8::Version::v1,
                    privateKeyAlgorithm: PKIX1Explicit88::AlgorithmIdentifier {
                        algorithm: pkix::Safecurves_pkix_18::ID_ED25519,
                        parameters: None,
                    },
                    privateKey: PKCS_8::PrivateKey::from(OctetString(
                        asn::builtin::BytesRef::Dynamic(key.into()),
                    )),
                }
            }
        }
    }

    pub fn public_key(&self) -> Result<PublicKey> {
        Ok(match self {
            PrivateKey::RSA(_) => todo!(),
            PrivateKey::RSASSA_PSS(_, _) => todo!(),
            PrivateKey::ECDSA(group_id, group, private_key) => {
                let public_value = group.public_value(&private_key)?;
                PublicKey::EC(group_id.clone(), group.clone(), public_value.into())
            }
            PrivateKey::Ed25519(private_key) => {
                let ed = EdwardsCurveGroup::ed25519();
                let public_key = ed.public_key(&private_key)?;
                PublicKey::Ed25519(public_key.into())
            }
        })
    }

    /// Gets a reasonable default signing algorithm that can be used with this
    /// key.
    pub fn default_signature_algorithm(&self) -> PKIX1Explicit88::AlgorithmIdentifier {
        // TODO: Move this to some config files?

        match self {
            PrivateKey::RSA(_) => todo!(),
            PrivateKey::RSASSA_PSS(_, _) => todo!(),
            PrivateKey::ECDSA(group_id, _, _) => {
                let algorithm = {
                    if group_id == &PKIX1Algorithms2008::SECP192R1 {
                        PKIX1Algorithms2008::ECDSA_WITH_SHA256
                    } else if group_id == &PKIX1Algorithms2008::SECP224R1 {
                        PKIX1Algorithms2008::ECDSA_WITH_SHA256
                    } else if group_id == &PKIX1Algorithms2008::SECP256R1 {
                        PKIX1Algorithms2008::ECDSA_WITH_SHA256
                    } else if group_id == &PKIX1Algorithms2008::SECP384R1 {
                        PKIX1Algorithms2008::ECDSA_WITH_SHA384
                    } else if group_id == &PKIX1Algorithms2008::SECP521R1 {
                        PKIX1Algorithms2008::ECDSA_WITH_SHA512
                    } else {
                        // We don't support other curves.
                        todo!()
                    }
                };

                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm,
                    parameters: None,
                }
            }
            PrivateKey::Ed25519(_) => PKIX1Explicit88::AlgorithmIdentifier {
                algorithm: Safecurves_pkix_18::ID_ED25519,
                parameters: None,
            },
        }

        /*
               SignatureScheme::rsa_pss_rsae_sha256,
        */
    }

    /// Checks if the given signature algorithm can be used with this key.
    /// For unknown/unsupported algorithms, this will return false.
    pub fn can_create_signature(
        &self,
        signature_algorithm: &PKIX1Explicit88::AlgorithmIdentifier,
        constraints: &SignatureKeyConstraints,
    ) -> Result<bool> {
        let sk = match self {
            Self::RSA(_) => SignatureKeyParameters::RSA,
            Self::RSASSA_PSS(_, params) => SignatureKeyParameters::RSASSA_PSS(params.clone()),
            Self::ECDSA(_, group, _) => SignatureKeyParameters::ECDSA(group.clone()),
            Self::Ed25519(_) => SignatureKeyParameters::Ed25519,
        };

        sk.can_use_with(signature_algorithm, constraints)
    }

    pub async fn create_signature(
        &self,
        plaintext: &[u8],
        signature_algorithm: &PKIX1Explicit88::AlgorithmIdentifier,
        constraints: &SignatureKeyConstraints,
    ) -> Result<Vec<u8>> {
        if !self.can_create_signature(signature_algorithm, constraints)? {
            return Err(err_msg(
                "Signature algorithm not compatible with private key",
            ));
        }

        match DigitalSignatureAlgorithm::create(signature_algorithm)? {
            DigitalSignatureAlgorithm::RSASSA_PKCS_v1_5(rsa) => {
                return rsa.create_signature(self.as_rsa_key()?, plaintext);
            }
            DigitalSignatureAlgorithm::RSASSA_PSS(rsa) => {
                return rsa.create_signature(self.as_rsa_key()?, plaintext).await;
            }
            DigitalSignatureAlgorithm::Ed25519(group) => {
                return group.create_signature(self.as_ed25519_key()?, plaintext);
            }
            DigitalSignatureAlgorithm::EcDSA(hasher_factory) => {
                let mut hasher = hasher_factory.create();
                let (_, group, point) = self.as_ec_key()?;
                return group
                    .create_signature(
                        point.as_ref(),
                        plaintext,
                        constraints
                            .ecdsa_signature_format
                            .unwrap_or(crate::elliptic::EllipticCurveSignatureFormat::X509),
                        hasher.as_mut(),
                    )
                    .await;
            }
        }
    }

    fn as_ec_key(&self) -> Result<(&ObjectIdentifier, &EllipticCurveGroup, &Bytes)> {
        match self {
            Self::ECDSA(a, b, c) => Ok((a, b, c)),
            _ => Err(err_msg("Expected an EC public key")),
        }
    }

    fn as_ed25519_key(&self) -> Result<&[u8]> {
        match self {
            Self::Ed25519(v) => Ok(v.as_ref()),
            _ => Err(err_msg("Expected an Ed25519 public key")),
        }
    }

    fn as_rsa_key(&self) -> Result<&RSAPrivateKey> {
        match self {
            Self::RSA(v) => Ok(v),
            _ => Err(err_msg("Expected an RSA public key")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    #[test]
    fn rsa_private_key_pem() -> Result<()> {
        let data = fs::read(project_path!("testdata/certificates/server-ec.key"))?;

        println!("{:#?}", PrivateKey::from_pem(data.into()));

        /*
        let pk = &self.raw.tbsCertificate.subjectPublicKeyInfo;
                if pk.algorithm.algorithm != PKIX1Algorithms2008::ID_ECPUBLICKEY {
                    return Err(err_msg("Wrong public key type"));
                }


                Ok((
                    group,
                    std::convert::Into::<OctetString>::into(point).into_bytes(),
                ))

                */

        Ok(())
    }
}
