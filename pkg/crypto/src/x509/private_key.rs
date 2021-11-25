use std::convert::TryInto;

use asn::builtin::{Null, ObjectIdentifier, OctetString};
use asn::encoding::{der_eq, DERReadable};
use common::bytes::Bytes;
use common::errors::*;
use pkix::PKIX1Algorithms2008;
use pkix::PKIX1Algorithms88;
use pkix::PKCS_1;

use crate::elliptic::EllipticCurveGroup;
use crate::pem::{PEM, PEM_PRIVATE_KEY_LABEL};
use crate::rsa::RSAPrivateKey;

#[derive(Debug, Clone)]
pub enum PrivateKey {
    RSA(RSAPrivateKey),
    ECDSA(ObjectIdentifier, EllipticCurveGroup, Bytes),
}

impl PrivateKey {
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
                        todo!()
                        // let ca = reg.lookup_parent(self)?.ok_or(err_msg("
                        // Unknown parent"))?;
                        // let (group, _) = ca.ec_public_key(reg)?;
                        // group
                    }
                    _ => {
                        return Err(err_msg("Unsupported curve format"));
                    }
                };

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
            } else {
                return Err(err_msg("Unsupported private key algorithm"));
            }
        } else {
            return Err(err_msg("Unsupported PEM label"));
        }

        // println!("{}", entry.label.as_ref());
        // let data = entry.to_binary()?.into();

        // println!("{:#?}", pkey);

        // asn::debug::print_debug_string(data);
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
