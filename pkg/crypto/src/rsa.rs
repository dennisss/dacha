
use asn::builtin::{Null, ObjectIdentifier, OctetString};
use asn::encoding::{Any, DERWriteable};
use common::errors::*;
use common::LeftPad;
use math::big::BigUint;
use math::big::Modulo;
use pkix::{
    PKIX1Algorithms2008, PKIX1Algorithms88, PKIX1Explicit88, PKIX1Implicit88,
    PKIX1_PSS_OAEP_Algorithms, NIST_SHA2, PKCS_1,
};

use crate::hasher::{Hasher, HasherFactory, GetHasherFactory};
use crate::md5::*;
use crate::sha1::*;
use crate::sha224::*;
use crate::sha256::*;
use crate::sha384::*;
use crate::sha512::*;

// TODO: Follow everything outlined in https://tools.ietf.org/html/rfc8017

macro_rules! ctor {
    ($name:ident, $id:ident, $hasher:ident) => {
        pub fn $name() -> Self {
            Self {
                digest_algorithm: PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: NIST_SHA2::$id.clone(),
                    parameters: Some(asn_any!(Null::new())),
                },
                digester_factory: $hasher::factory()
            }
        }
    };
}

#[allow(non_camel_case_types)]
pub struct RSASSA_PSS {
    hasher_factory: HasherFactory,
    salt_length: usize,
}

impl RSASSA_PSS {
    /// NOTE: This will use MGF1 as the mask generation function
    pub fn new(hasher_factory: HasherFactory, salt_length: usize) -> Self {
        Self { hasher_factory, salt_length }
    }

    /// RFC 8017: Section 8.1.2
    pub fn verify_signature(
        &self, public_key: &PKCS_1::RSAPublicKey, signature: &[u8], data: &[u8]
    ) -> Result<bool> {
        // TODO: Implement a more efficient way of getting this size.
        let k = common::ceil_div(public_key.modulus.nbits() - 1, 8);

        // Step 1
        if signature.len() != k {
            return Ok(false);
        }

        let modulus = public_key.modulus.to_uint()?;
        let public_exponent = public_key.publicExponent.to_uint()?;

        let s = BigUint::from_be_bytes(signature);

        let message = rsavp1(&modulus, &public_exponent, &s)?;

        // TODO: Verify has at least one bit.
        let encoded_len = common::ceil_div(modulus.nbits() - 1, 8);

        // Step 2.c
        let encoded_message = i2osp(&message, encoded_len);

        // Step 3
        let result = emsa_pss_verify(
            data, &encoded_message, modulus.nbits() - 1,
            &self.hasher_factory, self.salt_length)?;
        
        // Step 4
        Ok(result)
    }

}

#[allow(non_camel_case_types)]
pub struct RSASSA_PKCS_v1_5 {
    digest_algorithm: PKIX1Explicit88::AlgorithmIdentifier,
    
    /// Factory that can be used to create a digester corresponding to the
    /// 'digest_algorithm'.
    digester_factory: HasherFactory 
}

impl RSASSA_PKCS_v1_5 {

    // Supported hashes are documented in RFC 8017: A.2.4

    // TODO: MD2
    ctor!(md5, ID_MD5, MD5Hasher);
    ctor!(sha1, ID_SHA1, SHA1Hasher);
    ctor!(sha224, ID_SHA224, SHA224Hasher);
    ctor!(sha256, ID_SHA256, SHA256Hasher);
    ctor!(sha384, ID_SHA384, SHA384Hasher);
    ctor!(sha512, ID_SHA512, SHA512Hasher);
    ctor!(sha512_224, ID_SHA512_224, SHA512_224Hasher);
    ctor!(sha512_256, ID_SHA512_256, SHA512_256Hasher);

    /// Based on RFC 8017: Section 8.2.1
    pub fn create_signature(
        &self, private_key: &PKCS_1::RSAPrivateKey, data: &[u8]
    ) -> Result<Vec<u8>> {
        let k = common::ceil_div(private_key.modulus.nbits() - 1, 8);

        // Step 1
        let em = emsa_pkcs1_v1_5_encode(
            data, &self.digest_algorithm, self.digester_factory.create(), k)?;

        // Step 2.a
        let m = BigUint::from_be_bytes(&em);

        // Step 2.b
        let s = rsasp1(private_key, &m)?;

        // Step 2.c
        let signature = i2osp(&s, k);

        // Step 3
        Ok(signature)
    }


    /// Based on RFC 8017: Section 8.2.2
    pub fn verify_signature(
        &self, public_key: &PKCS_1::RSAPublicKey, signature: &[u8], data: &[u8]
    ) -> Result<bool> {
        // TODO: Implement a more efficient way of getting this size.
        let k = common::ceil_div(public_key.modulus.nbits() - 1, 8);

        if signature.len() != k {
            return Ok(false);
        }

        let e = public_key.publicExponent.to_uint()?;
        let n = public_key.modulus.to_uint()?;

        // TODO: Be I need to verify that it is not negative
        let s = BigUint::from_be_bytes(signature);

        // Step 2.b
        let m = rsavp1(&n, &e, &s)?;

        // Step 2: Pad to the length of the modulus.
        let em = i2osp(&m, k);

        // Step 3
        // TODO: May want to return a "RSA modulus too short" error.
        let em2 = emsa_pkcs1_v1_5_encode(
            data, &self.digest_algorithm, self.digester_factory.create(), k)?;

        // TODO: Probably want to use a secure compare here?
        Ok(em == em2)
    }

}

/// RFC 8017: Section 4.1
fn i2osp(x: &BigUint, x_len: usize) -> Vec<u8> {
    x.to_be_bytes().left_pad(x_len, 0)
}

/// RFC 8017: 5.2.1
fn rsasp1(private_key: &PKCS_1::RSAPrivateKey, message: &BigUint) -> Result<BigUint> {
    // TODO: support the form where the modulus and exponent are not available
    
    let modulus = private_key.modulus.to_uint()?;
    let private_exponent = private_key.privateExponent.to_uint()?;

    if message >= &private_exponent {
        return Err(err_msg("message representative out of range"));
    }

    let signature = Modulo::new(&modulus).pow(message, &private_exponent);
    Ok(signature)
}

/// RFC 8017: Section 5.2.2
fn rsavp1(n: &BigUint, e: &BigUint, s: &BigUint) -> Result<BigUint> {
    if s >= n {
        return Err(err_msg("signature representative out of range"));
    }

    // m = s^e mod n
    let message = Modulo::new(n).pow(s, e);
    Ok(message)
}

/// RFC 8017: Section 9.1.1
async fn emsa_pss_encode(
    message: &[u8],
    max_output_bits: usize,
    hasher_factory: &HasherFactory,
    salt_length: usize
) -> Result<Vec<u8>> {
    // TODO: Step 1

    // Step 2
    let message_hash = {
        let mut hasher = hasher_factory.create();
        hasher.update(message);
        hasher.finish()
    };

    // Also known as 'emLen'
    let output_length = common::ceil_div(max_output_bits, 8);

    if output_length < message_hash.len() + salt_length + 2 {
        return Err(err_msg("encoding error"));
    }

    // Step 4
    let mut salt = vec![];
    if salt_length > 0 {
        salt.resize(salt_length, 0);
        crate::random::secure_random_bytes(&mut salt).await?;
    }

    // Step 5
    let mut message_hash_salted = vec![];
    message_hash_salted.resize(8, 0);
    message_hash_salted.extend_from_slice(&message_hash);
    message_hash_salted.extend_from_slice(&salt);

    // Step 6
    let hash = {
        let mut hasher = hasher_factory.create();
        hasher.update(&message_hash_salted);
        hasher.finish()
    };

    // Step 7
    let mut ps = vec![];
    ps.resize(output_length - salt_length - hash.len() - 2, 0);

    // Step 8
    let mut db = vec![];
    db.extend_from_slice(&ps);
    db.push(0x01);
    db.extend_from_slice(&salt);
    assert_eq!(db.len(), output_length - hash.len() - 1);

    // Step 9
    let db_mask = mgf1(&hash, db.len(), hasher_factory);

    // Step 10
    let mut masked_db = db;
    crate::utils::xor_inplace(&db_mask, &mut masked_db);

    // Step 11
    {
        let nclear = 8*output_length - max_output_bits;
        for i in 0..nclear {
            common::bits::bitset(&mut masked_db[0], false, (7 - i) as u8);
        }
    }

    // Step 12
    let mut encoded_message = masked_db;
    encoded_message.extend_from_slice(&hash);
    encoded_message.push(0xBC);

    Ok(encoded_message)
}

/// RFC 8017: Section 9.1.2
fn emsa_pss_verify(
    message: &[u8], encoded_message: &[u8], em_bits: usize,
    mut hasher_factory: &HasherFactory,
    salt_length: usize,
) -> Result<bool> {
    // TODO: Verify that the input size is within the input size limit for the hasher

    // Step 2
    let message_hash = {
        let mut hasher = hasher_factory.create(); 
        hasher.update(message);
        hasher.finish()
    };

    // Step 3
    if encoded_message.len() < message_hash.len() + salt_length + 2 {
        return Ok(false);
    }

    // Step 4
    if *encoded_message.last().unwrap() != 0xBC {
        return Ok(false)
    }

    // Step 5
    let (masked_db, hash) = {
        let i = encoded_message.len() - message_hash.len() - 1;
        (&encoded_message[0..i], &encoded_message[i..(i + message_hash.len())])
    };

    // Step 6: Verifying that the top bits are zero
    {
        // TODO: Check  for overflow?
        let nbits = 8*encoded_message.len() - em_bits;
        for i in 0..nbits {
            if common::bits::bitget(masked_db[0], (7 - i) as u8) {
                return Ok(false);
            }
        }
    }

    // Step 7
    let db_mask = mgf1(&hash, masked_db.len(), hasher_factory);

    // Step 8
    let mut db = masked_db.to_vec();
    crate::utils::xor_inplace(&db_mask, &mut db);

    // Step 9: Set top most bits to zero
    {
        let nbits = 8*encoded_message.len() - em_bits;
        for i in 0..nbits {
            common::bits::bitset(&mut db[0], false, (7 - i) as u8);
        }
    }

    // Step 10: Checking ps is zero
    {
        let n = encoded_message.len() - hash.len() - salt_length - 2;
        for i in 0..n {
            if db[i] != 0 {
                return Ok(false);
            }
        }

        if db[n] != 1 {
            return Ok(false);
        }
    }

    // Step 11
    let salt = &db[(db.len() - salt_length)..];

    // Step 12
    let mut message_hash_salted = vec![];
    message_hash_salted.resize(8, 0);
    message_hash_salted.extend_from_slice(&message_hash);
    message_hash_salted.extend_from_slice(salt);

    let hash2 = {
        let mut hasher = hasher_factory.create();
        hasher.update(&message_hash_salted);
        hasher.finish()
    };

    Ok(hash == hash2)
}


/// RFC 8017: Section 9.2
fn emsa_pkcs1_v1_5_encode(
    input: &[u8],
    algorithm: &PKIX1Explicit88::AlgorithmIdentifier,
    mut hasher: Box<dyn Hasher>,
    output_len: usize,
) -> Result<Vec<u8>> {
    hasher.update(input);
    let digest = hasher.finish();

    let info = PKCS_1::DigestInfo {
        digestAlgorithm: algorithm.clone(),
        digest: digest.into(),
    }
    .to_der();

    if output_len < info.len() + 11 {
        return Err(err_msg("intended encoded message length too short"));
    }

    let mut output = vec![];
    output.push(0x00);
    output.push(0x01);
    output.resize(output.len() + (output_len - info.len() - 3), 0xff);
    output.push(0x00);
    output.extend_from_slice(&info);
    Ok(output)
}

/// RFC 8017: B.2.1
fn mgf1(mgf_seed: &[u8], mask_len: usize, hasher_factory: &HasherFactory) -> Vec<u8> {
    // TODO: Step 1

    // Step 2
    // TODO: Reserve space
    let mut output = vec![];

    // Step 3
    for counter in 0..common::ceil_div(mask_len, hasher_factory.create().output_size()) {
        // Step 3.A
        let c = counter.to_be_bytes().to_vec().left_pad(4, 0);

        // Step 3.B
        let mut plain = mgf_seed.to_vec();
        plain.extend_from_slice(&c);

        let hash = {
            let mut hasher = hasher_factory.create();
            hasher.update(&plain);
            hasher.finish()
        };

        output.extend_from_slice(&plain);
    }

    // Step 4
    output.truncate(mask_len);

    output
}



#[cfg(test)]
mod tests {


}