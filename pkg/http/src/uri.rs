use common::errors::*;
use super::ascii::*;
use super::uri_parser::*; // TODO: Cyclic reference
use bytes::Bytes;
use std::net::ToSocketAddrs;
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};


#[derive(Debug)]
pub struct Uri {
	pub scheme: Option<AsciiString>,
	pub authority: Option<Authority>,
	pub path: String,
	pub query: Option<AsciiString>,
	// NOTE: This will always be empty for absolute_uri
	pub fragment: Option<AsciiString>
}

impl Uri {
	// TODO: Encode any characters that we need to encode for this.
	pub fn to_string(&self) -> String {
		let mut out = String::new();
		if let Some(scheme) = &self.scheme {
			out += &format!("{}:", scheme.to_string());
		}

		// TODO: Authority

		out += &self.path;

		if let Some(query) = &self.query {
			out += &format!("?{}", query.to_string());
		}

		if let Some(fragment) = &self.fragment {
			out += &format!("#{}", fragment.to_string());
		}

		out
	}
}

impl std::str::FromStr for Uri {
	type Err = Error;
	fn from_str(s: &str) -> Result<Self> {
		let (v, rest) = parse_uri(Bytes::from(s))?;
		if rest.len() != 0 {
			let reststr = String::from_utf8(rest.to_vec()).unwrap();
			return Err(format!("Extra bytes after uri: '{}'.", reststr).into());
		}

		Ok(v)
	}
}


#[derive(Debug)]
pub struct Authority {
	pub user: Option<AsciiString>,
	pub host: Host,
	pub port: Option<usize>
}

#[derive(Debug)]
pub enum Host {
	Name(AsciiString),
	IP(IPAddress)
}

#[derive(Debug)]
pub enum IPAddress {
	V4(Vec<u8>),
	V6(Vec<u8>),
	VFuture(Vec<u8>)
}

impl std::convert::TryFrom<IPAddress> for IpAddr {
	type Error = Error;
	
	fn try_from(ip: IPAddress) -> Result<Self> {
		Ok(match ip {
			IPAddress::V4(v) => {
				IpAddr::V4(Ipv4Addr::new(v[0], v[1], v[2], v[3]))
			},
			IPAddress::V6(v) => {
				return Err("IPV6 not supported".into());
				// TODO: This is wrong. Must parse u16's
				// IpAddr::V6(Ipv6Addr::new(v[0], v[1], v[2], v[3],
				// 						 v[4], v[5], v[6], v[7]))
			},
			IPAddress::VFuture(_) => {
				return Err("Future ip address not supported".into());
			}
		})
	}
}


#[derive(Debug)]
pub enum UriPath {
	AbEmpty(Vec<AsciiString>),
	Absolute(Vec<AsciiString>),
	Rootless(Vec<AsciiString>),
	Empty
}

impl UriPath {
	pub fn to_string(&self) -> String {
		let join = |strs: &Vec<AsciiString>| {
			strs.iter().map(|s| s.to_string()).collect::<Vec<_>>().join("/")
		};

		match self {
			UriPath::AbEmpty(v) => format!("/{}", join(v)),
			UriPath::Absolute(v) => format!("/{}", join(v)),
			UriPath::Rootless(v) => join(v),
			UriPath::Empty => String::new(),
		}
	}
}


#[derive(Debug)]
pub enum Path {
	PathAbEmpty(Vec<AsciiString>),
	PathAbsolute(Vec<AsciiString>),
	PathNoScheme(Vec<AsciiString>),
	PathRootless(Vec<AsciiString>),
	PathEmpty
}