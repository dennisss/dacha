#[macro_use]
extern crate common;
extern crate alloc;
extern crate crypto;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use alloc::boxed::Box;
use crypto::pem::PEMBuilder;
use crypto::pem::PEM_CERTIFICATE_REQUEST_LABEL;
use pkix::PKIX1Explicit88;
use std::io::Read;
use std::num::Wrapping;
use std::str::FromStr;
use std::string::String;
use std::string::ToString;

use common::errors::*;
use math::big::{BigUint, SecureBigUint};

use asn::encoding::DERReadable;
use common::io::*;
use crypto::tls::handshake::*;
use crypto::tls::record::*;
use crypto::tls::*;

/*
async fn tls_connect() -> Result<()> {
    let raw_stream = TcpStream::connect("google.com:443").await?;
    let reader = Box::new(raw_stream.clone());
    let writer = Box::new(raw_stream);

    let mut client_options = crypto::tls::options::ClientOptions::recommended();
    client_options.hostname = "google.com".into();
    client_options.alpn_ids.push("h2".into());
    client_options.alpn_ids.push("http/1.1".into());

    let mut client = crypto::tls::client::Client::new();
    let mut stream = client.connect(reader, writer, &client_options).await?;

    stream
        .writer
        .write_all(b"GET / HTTP/1.1\r\nHost: google.com\r\n\r\n")
        .await?;

    let mut buf = vec![];
    buf.resize(100, 0);
    stream.reader.read_exact(&mut buf).await?;
    println!("{}", String::from_utf8(buf).unwrap());

    Ok(())
}
*/

fn debug_pem() -> Result<()> {
    let path = project_path!("testdata/certificates/server-ec.key");

    let mut f = std::fs::File::open(path)?;

    let mut buf = vec![];
    f.read_to_end(&mut buf)?;

    let pem = crypto::pem::PEM::parse(buf.into())?;

    for entry in pem.entries {
        println!("{}", entry.label.as_ref());
        let data = entry.to_binary()?.into();

        let pkey_info = pkix::PKCS_8::PrivateKeyInfo::from_der(data)?;
        println!("{:#?}", pkey_info);

        let pkey = pkix::PKCS_1::RSAPrivateKey::from_der(pkey_info.privateKey.to_bytes())?;
        println!("{:#?}", pkey);

        // asn::debug::print_debug_string(data);
    }

    Ok(())
}

use crypto::rsa::*;
use math::integer::Integer;

/*
Current speed: 31.4 seconds.
*/

async fn rsa_bench() -> Result<()> {
    let file = crypto::nist::response::ResponseFile::open(project_path!(
        "testdata/nist/rsa/fips186_2/SigGen15_186-2.txt"
    ))
    .await?;

    let mut iter = file.iter();

    let mut private_key = None;
    let mut public_key = None;

    // TODO: This is very slow.
    loop {
        let mut block = match iter.next() {
            Some(v) => v?,
            None => break,
        };

        if block.new_attributes {
            // Modulus size in bits
            let modulus_size = block.attributes["MOD"].parse::<usize>()?;

            let modulus = block.binary_field("N")?;

            block = iter.next().unwrap()?;

            let public_exponent = block.binary_field("E")?;
            let private_exponent = block.binary_field("D")?;

            println!(
                "MOD: {}",
                SecureBigUint::from_be_bytes(&modulus).bit_width()
            );

            private_key = Some(RSAPrivateKey::new(
                SecureBigUint::from_be_bytes(&modulus),
                SecureBigUint::from_be_bytes(&private_exponent),
            ));

            public_key = Some(RSAPublicKey {
                modulus: SecureBigUint::from_be_bytes(&modulus),
                public_exponent: SecureBigUint::from_be_bytes(&public_exponent),
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
            _ => panic!("Unknown algorithm {}", hash_str),
        };

        let output = pkcs.create_signature(private_key.as_ref().unwrap(), &message)?;
        assert_eq!(output, signature);

        assert!(pkcs.verify_signature(public_key.as_ref().unwrap(), &signature, &message)?);
    }

    Ok(())
}

use crypto::aes::*;
use crypto::gcm::*;

fn gcm_bench() {
    let k = hex!("feffe9928665731c6d6a8f9467308308");
    let p = hex!(
        "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255");
    let iv = hex!("cafebabefacedbaddecaf888");

    // NOTE: Final 4d5c2af327cd64a62cf35abd2ba6fab4 is the tag.
    let cipher = hex!("42831ec2217774244b7221b784d0d49ce3aa212f2c02a4e035c17e2329aca12e21d514b25466931c7d8f6a5aac84aa051ba30b396a0aac973d58e091473f59854d5c2af327cd64a62cf35abd2ba6fab4");

    for i in 0..300000 {
        let mut out = vec![];
        let mut gcm = GaloisCounterMode::new(&iv, AESBlockCipher::create(&k).unwrap());
        gcm.encrypt(&p, &[], &mut out);

        assert_eq!(&out, &cipher);
    }
}

async fn debug_csr() -> Result<()> {
    {
        let key_data = common::bytes::Bytes::from(
            file::read(project_path!(
                "/home/dennis/workspace/dacha/testdata/certificates/server-ec.key"
            ))
            .await?,
        );

        let pkey = crypto::x509::PrivateKey::from_pem(key_data)?;

        println!("{:?}", pkey);

        // return Ok(());
    }

    let data = common::bytes::Bytes::from(
        file::read(project_path!("testdata/x509/csr/request.csr")).await?,
    );

    let pem = crypto::pem::PEM::parse(data)?;
    assert_eq!(pem.entries.len(), 1);
    assert_eq!(
        pem.entries[0].label.as_str(),
        crypto::pem::PEM_CERTIFICATE_REQUEST_LABEL
    );

    let csr = pkix::PKCS_10::CertificationRequest::from_der(pem.entries[0].to_binary()?.into())?;
    println!("{:#?}", csr);

    match &csr.certificationRequestInfo.subject {
        pkix::PKIX1Explicit88::Name::rdnSequence(seq) => {
            for rdn in &seq.items {
                for attr in &rdn.items {
                    if *attr.typ == *PKIX1Explicit88::ID_AT_COMMONNAME {
                        let cn = attr.value.parse_as::<PKIX1Explicit88::X520CommonName>()?;
                        println!("{:?}", cn);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn generate_csr() -> Result<()> {
    let private_key = crypto::x509::PrivateKey::generate_default().await?;
    println!("{}", private_key.to_pem());

    let mut csr = crypto::x509::CertificateRequestBuilder::default();
    csr.set_common_name("example.com")?;
    csr.set_subject_alt_names(&["foo.bar", "hello.world"])?;

    let csr = csr.build(&private_key).await?;

    println!("Valid: {}", csr.verify_signature()?);

    let csr_pem = csr.to_pem();
    println!("{}", csr_pem);

    file::write(project_path!("test.csr"), csr_pem).await?;
    /*
    Can verify with:

    openssl req -text -noout -verify -in test.csr

    */

    Ok(())
}

fn main() -> Result<()> {
    return executor::run_main(debug_csr())?;

    return executor::run_main(generate_csr())?;

    gcm_bench();
    return Ok(());

    return executor::run_main(rsa_bench())?;

    return debug_pem();

    // return task::block_on(tls_connect());

    let mut file = std::fs::File::open("testdata/google.der")?;

    let mut data = vec![];
    file.read_to_end(&mut data)?;

    // return crypto::x509::parse_ber(data.into());

    // 12193263135650053146912909516205414460041
    let a = BigUint::from_str("12345678912345678912345")?;
    let b = BigUint::from_str("987654321987654321")?;
    let out = a * b;

    println!("NUL: {:?}", out);

    return Ok(());

    println!("hi!");

    /*
    let mut n = 0;
    for i in 0..35 {
        if extended_gcd(i, 35) == 1 {
            n += 1;
        }
    }

    println!("(Z_35)* = {}", n);

    let mut v = 1;
    for i in 0..10001 {
        v = 2 * v % 11;
    }

    println!("mod 11 = {}", v);

    let mut v = 1;
    for i in 0..245 {
        v = 2 * v % 35;
    }

    println!("mod 35 = {}", v);

    // extended_gcd(7, 23);
    extended_gcd(3, 19);

    for i in 0..13 {
        if extended_gcd(i, 13) == 1 {
            println!("{}", i);
        }
    }

    for x in 0..23 {
        let y = (((x * x) % 23) + ((4 * x) % 23) + 1) % 23;
        if y == 0 {
            println!("x = {}", x);
        }
    }
    */

    Ok(())
}
