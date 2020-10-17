extern crate crypto;

use common::errors::*;
use std::num::Wrapping;

// https://en.wikipedia.org/wiki/Extended_Euclidean_algorithm#Pseudocode
fn extended_gcd(a: isize, b: isize) -> isize {
	let mut s = 0;
	let mut old_s = 1;
	let mut t = 1;
	let mut old_t = 0;
	let mut r = b;
	let mut old_r = a;

	while r != 0 {
		let quotient = old_r / r;

		let tmp_r = r;
		r = old_r - quotient * r;
		old_r = tmp_r;

		let tmp_s = s;
		s = old_s - quotient * s;
		old_s = tmp_s;

		let tmp_t = t;
		t = old_t - quotient * t;
		old_t = tmp_t;
	}

	// println!("Bezout coefficients: {} {}", old_s, old_t);
	// println!("greatest common divisor: {}", old_r);
	// println!("quotients by the gcd: {} {}", t, s);
	old_r
}

use math::big::BigUint;
use std::str::FromStr;
use std::string::ToString;

use common::async_std::net::TcpStream;
use common::async_std::prelude::*;
use common::async_std::task;

use crypto::tls::handshake::*;
use crypto::tls::record::*;
use crypto::tls::*;

use common::io::*;
use std::io::Read;

async fn tls_connect() -> Result<()> {
	let input = Box::new(TcpStream::connect("google.com:443").await?);

	let mut client = crypto::tls::client::Client::new();
	let stream = client.connect(input, "google.com").await?;

	stream
		.write_all(b"GET / HTTP/1.1\r\nHost: google.com\r\n\r\n")
		.await?;

	let mut buf = vec![];
	buf.resize(100, 0);
	stream.read_exact(&mut buf).await?;
	println!("{}", String::from_utf8(buf).unwrap());

	Ok(())
}

fn debug_pem() -> Result<()> {
	//	let path = "/home/dennis/workspace/dacha/server-key.pem";
	let path = "/home/dennis/workspace/insight/config/server.key";

	let mut f = std::fs::File::open(path)?;

	let mut buf = vec![];
	f.read_to_end(&mut buf)?;

	let pem = crypto::pem::PEM::parse(buf.into())?;

	for entry in pem.entries {
		println!("{}", entry.label.as_ref());
		let data = entry.to_binary()?.into();
		asn::debug::print_debug_string(data);
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

	Ok(())
}
