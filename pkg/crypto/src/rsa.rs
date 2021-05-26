
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

pub struct RSAPublicKey {
    modulus: BigUint,
    public_exponent: BigUint
}

// TODO: Pass by ref.
impl std::convert::TryFrom<PKCS_1::RSAPublicKey> for RSAPublicKey {
    type Error = common::errors::Error;

    fn try_from(value: PKCS_1::RSAPublicKey) -> Result<Self> {
        Ok(Self {
            modulus: value.modulus.to_uint()?,
            public_exponent: value.publicExponent.to_uint()?
        })
    }
}


pub struct RSAPrivateKey {
    modulus: BigUint,
    private_exponent: BigUint
}

impl std::convert::TryFrom<&PKCS_1::RSAPrivateKey> for RSAPrivateKey {
    type Error = common::errors::Error;

    fn try_from(value: &PKCS_1::RSAPrivateKey) -> Result<Self> {
        Ok(Self {
            modulus: value.modulus.to_uint()?,
            private_exponent: value.privateExponent.to_uint()?
        })
    }
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
        &self, public_key: &RSAPublicKey, signature: &[u8], data: &[u8]
    ) -> Result<bool> {
        // TODO: Implement a more efficient way of getting this size.
        let k = common::ceil_div(public_key.modulus.nbits(), 8);

        // Step 1
        if signature.len() != k {
            return Ok(false);
        }

        let s = BigUint::from_be_bytes(signature);

        let message = rsavp1(&public_key.modulus, &public_key.public_exponent, &s)?;

        // TODO: Verify has at least one bit.
        let encoded_len = common::ceil_div(public_key.modulus.nbits() - 1, 8);

        // Step 2.c
        let encoded_message = i2osp(&message, encoded_len);

        // Step 3
        let result = emsa_pss_verify(
            data, &encoded_message, public_key.modulus.nbits() - 1,
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
        &self, private_key: &RSAPrivateKey, data: &[u8]
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
        &self, public_key: &RSAPublicKey, signature: &[u8], data: &[u8]
    ) -> Result<bool> {
        // TODO: Implement a more efficient way of getting this size.
        let k = common::ceil_div(public_key.modulus.nbits() - 1, 8);

        if signature.len() != k {
            return Ok(false);
        }

        let e = &public_key.public_exponent;
        let n = &public_key.modulus;

        // TODO: Be I need to verify that it is not negative
        let s = BigUint::from_be_bytes(signature);

        // Step 2.b
        let m = rsavp1(n, e, &s)?;

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
fn rsasp1(private_key: &RSAPrivateKey, message: &BigUint) -> Result<BigUint> {
    // TODO: support the form where the modulus and exponent are not available

    if message >= &private_key.private_exponent {
        return Err(err_msg("message representative out of range"));
    }

    let signature = Modulo::new(&private_key.modulus).pow(message, &private_key.private_exponent);
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
    hasher_factory: &HasherFactory,
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

    // Step 6: Verifying that the top bits of masked_db are zero
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

    // Step 10: Checking 'ps' is zero
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

    // TODO: Use secure comparison 
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

    // Precompute a partial hash state for 'mfg_seed'. The rest of this
    // function will repeatedly compute hashes which have this prefix.
    let prefix_hasher = {
        let mut hasher = hasher_factory.create();
        hasher.update(mgf_seed);
        hasher
    };

    // Step 3
    for counter in 0..common::ceil_div(mask_len, hasher_factory.create().output_size()) {
        // Step 3.A
        let c = (counter as u32).to_be_bytes();

        // Step 3.B
        let hash = {
            let mut hasher = prefix_hasher.box_clone();
            hasher.update(&c);
            hasher.finish()
        };

        output.extend_from_slice(&hash);
    }

    // Step 4
    output.truncate(mask_len);
    assert_eq!(mask_len, output.len());

    output
}



#[cfg(test)]
mod tests {
    use super::*;

    use common::errors::*;
    use common::hex;
    use typenum::U20;
    
    use crate::hasher::TruncatedHasher;

    #[async_std::test]
    async fn rsa_pkcs_15_nist_test() -> Result<()> {
        let file = crate::nist::response::ResponseFile::open(
            project_path!("testdata/nist/rsa/fips186_2/SigGen15_186-2.txt")).await?;

        let mut iter = file.iter();

        let mut private_key = None;
        let mut public_key = None;

        loop {
            let mut block = match iter.next() {
                Some(v) => v?, None => break };

            if block.new_attributes {
                // Modulus size in bits
                let modulus_size = block.attributes["MOD"].parse::<usize>()?;

                let modulus = block.binary_field("N")?;

                block = iter.next().unwrap()?;

                let public_exponent = block.binary_field("E")?;
                let private_exponent = block.binary_field("D")?;

                private_key = Some(RSAPrivateKey {
                    modulus: BigUint::from_be_bytes(&modulus),
                    private_exponent: BigUint::from_be_bytes(&private_exponent)
                });
    
                public_key = Some(RSAPublicKey {
                    modulus: BigUint::from_be_bytes(&modulus),
                    public_exponent: BigUint::from_be_bytes(&public_exponent)
                });

                continue;
            }
            
            println!("RUN");
            let hash_str = block.fields["SHAALG"].as_str();
            let message = block.binary_field("MSG")?;
            let signature = block.binary_field("S")?;

            let pkcs = match hash_str {
                "SHA1" => RSASSA_PKCS_v1_5::sha1(),
                "SHA224" => RSASSA_PKCS_v1_5::sha224(),
                "SHA256" => RSASSA_PKCS_v1_5::sha256(),
                "SHA384" => RSASSA_PKCS_v1_5::sha384(),
                "SHA512" => RSASSA_PKCS_v1_5::sha512(),
                _ => panic!("Unknown algorithm {}", hash_str)
            };

            let output = pkcs.create_signature(private_key.as_ref().unwrap(), &message)?;
            assert_eq!(output, signature);

            assert!(pkcs.verify_signature(public_key.as_ref().unwrap(), &signature, &message)?);
        }

        Ok(())
    }

    // TODO: Deduplicate with the previous test case.
    #[async_std::test]
    async fn rsa_pss_nist_test() -> Result<()> {
        let file = crate::nist::response::ResponseFile::open(
            project_path!("testdata/nist/rsa/fips186_2/SigGenPSS_186-2.txt")).await?;

        let mut iter = file.iter();

        let mut private_key = None;
        let mut public_key = None;

        loop {
            let mut block = match iter.next() {
                Some(v) => v?, None => break };

            if block.new_attributes {
                // Modulus size in bits
                let modulus_size = block.attributes["MOD"].parse::<usize>()?;

                let modulus = block.binary_field("N")?;

                block = iter.next().unwrap()?;

                let public_exponent = block.binary_field("E")?;
                let private_exponent = block.binary_field("D")?;

                private_key = Some(RSAPrivateKey {
                    modulus: BigUint::from_be_bytes(&modulus),
                    private_exponent: BigUint::from_be_bytes(&private_exponent)
                });
    
                public_key = Some(RSAPublicKey {
                    modulus: BigUint::from_be_bytes(&modulus),
                    public_exponent: BigUint::from_be_bytes(&public_exponent)
                });

                continue;
            }
            
            let hash_str = block.fields["SHAALG"].as_str();
            let message = block.binary_field("MSG")?;
            let signature = block.binary_field("S")?;
            let salt = block.binary_field("SALTVAL")?;

            let pkcs = match hash_str {
                "SHA1" => RSASSA_PSS::new(SHA1Hasher::factory(), salt.len()),
                "SHA224" => RSASSA_PSS::new(SHA224Hasher::factory(), salt.len()),
                "SHA256" => RSASSA_PSS::new(SHA256Hasher::factory(), salt.len()),
                "SHA384" => RSASSA_PSS::new(SHA384Hasher::factory(), salt.len()),
                "SHA512" => RSASSA_PSS::new(SHA512Hasher::factory(), salt.len()),
                _ => panic!("Unknown algorithm {}", hash_str)
            };

            // let output = pkcs.create_signature(private_key.as_ref().unwrap(), &message)?;
            // assert_eq!(output, signature);

            assert!(pkcs.verify_signature(public_key.as_ref().unwrap(), &signature, &message)?);
        }

        Ok(())
    }

    type SHA256T20Hasher = TruncatedHasher<SHA256Hasher, U20>; 

    #[test]
    fn mgf1_test() -> Result<()> {
        // Test cases from ISO/IEC 18033-2
        // (KDF 1 used in RSA-KEM where the seed is 'R' in the doc and the output mask is 'K')
        // (KDF 1 used in ECIES-KEM where the seed is 'Z||PEH')

        // (Hasher, Seed, Expected Mask)
        let tests: &[(HasherFactory, &'static str, &'static str)] = &[
            // C.2.1
            (SHA1Hasher::factory(), "5110f7e54f656e70c71ea2067c901570088a1eb1b230000abba1b2df4b774bed543c0325b7083f2b477d5c02ddcafdfec0725672da2cbed39baf75f02dc078d04e9752632f973db43ed3d06ffd5bd9e741af0f855cbc556b73ab530affd7850ca4c93d4b91d73b47db8718c05e296151e036cf9ba980cef6563af244438cac1b", "23e41472d780bfbb2daafd85a8fcdf8641fdca4d9f539a4ad175c473ca0f498728931bc311baa2c957ab528935aa22954075a2899ab1ce8ff5ba90a049aeba8cbb9019bccfc5c24c815ac8a1106e163936597b5d06ba4b52377ca48d82621b2768373a210388998b964c11b0a2780c12c49cdea2cb454543fb3b725b026443d9"),

            // C.2.2
            (SHA1Hasher::factory(), "04ccc9ea07b8b71d25646b22b0e251362a3fa9e993042315df047b2e07dd2ffb89359945f3d22ca8757874be2536e0f924cdec12c4cf1cb733a2a691ad945e124535e5fc10c70203b5", "9a709adeb6c7590ccfc7d594670dd2d74fcdda3f8622f2dbcf0f0c02966d5d9002db578c989bf4a5cc896d2a11d74e0c51efc1f8ee784897ab9b865a7232b5661b7cac87cf4150bdf23b015d7b525b797cf6d533e9f6ad49a4c6de5e7089724c9cadf0adf13ee51b41be6713653fc1cb2c95a1d1b771cc7429189861d7a829f3"),

            // C.2.3
            (SHA1Hasher::factory(), "02ccc9ea07b8b71d25646b22b0e251362a3fa9e993042315dfcdec12c4cf1cb733a2a691ad945e124535e5fc10c70203b5", "8fbe0903fac2fa05df02278fe162708fb432f3cbf9bb14138d22be1d279f74bfb94f0843a153b708fcc8d9446c76f00e4ccabef85228195f732f4aedc5e48efcf2968c3a46f2df6f2afcbdf5ef79c958f233c6d208f3a7496e08f505d1c792b314b45ff647237b0aa186d0cdbab47a00fb4065d62cfc18f8a8d12c78ecbee3fd"),

            // C.6.2
            (SHA1Hasher::factory(), "032e45326fa859a72ec235acff929b15d1372e30b207255f0611b8f785d764374152e0ac009e509e7ba30cd2f1778e113b64e135cf4e2292c75efe5288edfda4", "5f8de105b5e96b2e490ddecbd147dd1def7e3b8e0e6a26eb7b956ccb8b3bdc1ca975bc57c3989e8fbad31a224655d800c46954840ff32052cdf0d640562bdfadfa263cfccf3c52b29f2af4a1869959bc77f854cf15bd7a25192985a842dbff8e13efee5b7e7e55bbe4d389647c686a9a9ab3fb889b2d7767d3837eea4e0a2f04"),

            // C.6.3
            (SHA256T20Hasher::factory(), "032e45326fa859a72ec235acff929b15d1372e30b207255f0611b8f785d764374152e0ac009e509e7ba30cd2f1778e113b64e135cf4e2292c75efe5288edfda4", "09e2decf2a6e1666c2f6071ff4298305e2643fd510a2403db42a8743cb989de86e668d168cbe604611ac179f819a3d18412e9eb45668f2923c087c12fee0c5a0d2a8aa70185401fbbd99379ec76c663e875a60b4aacb1319fa11c3365a8b79a44669f26fb555c80391847b05eca1cb5cf8c2d531448d33fbaca19f6410ee1fcb"),

            // C.5.3
            (SHA256T20Hasher::factory(), "09248da92dcf5ca8360ae7f18533a19c6ba8e99adf79665bc31dc5a62f70535e52c53015b9d37d412ff3c1193439599e1b628774c50d9ccb78d82c425e4521ee47b8c36a4bcffe8b8112a89312fc04432a6db6f05118f9946c80230cd9222e0146f2cbd5251cc388a62359", "6f0195f38eed2417aa6eb7a365245073e58711db")
        ];

        for (hasher_factory, seed, mask) in tests {
            let seed = hex::decode(*seed)?;
            let mask = hex::decode(*mask)?;

            let output = mgf1(&seed, mask.len(), hasher_factory);
            assert_eq!(output, mask);
        }

        Ok(())
    }


}