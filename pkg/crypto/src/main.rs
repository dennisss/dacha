#[macro_use]
extern crate common;
extern crate alloc;
extern crate crypto;

use alloc::boxed::Box;

use common::errors::*;
use std::num::Wrapping;

use math::big::BigUint;
use std::str::FromStr;
use std::string::String;
use std::string::ToString;

use asn::encoding::DERReadable;
use common::async_std::net::TcpStream;
use common::async_std::prelude::*;
use common::async_std::task;

use crypto::tls::handshake::*;
use crypto::tls::record::*;
use crypto::tls::*;

use common::io::*;
use std::io::Read;

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

fn main() -> Result<()> {
    return debug_pem();

    return task::block_on(tls_connect());

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
