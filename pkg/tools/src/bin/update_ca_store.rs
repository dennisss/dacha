// Updates the root CA store used in this repository by pulling it from the
// Chromium project.

#[macro_use]
extern crate common;
extern crate crypto;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::convert::TryInto;
use std::sync::Arc;

use asn::builtin::BitString;
use asn::encoding::DERWriteable;
use common::{bits::BitVector, bytes::Bytes, errors::*};

#[executor_main]
async fn main() -> Result<()> {
    let client = http::SimpleClient::new(http::SimpleClientOptions::default());

    let req = http::RequestBuilder::new()
        .method(http::Method::GET)
        .uri("https://chromium.googlesource.com/chromium/src/+/main/net/data/ssl/chrome_root_store/root_store.certs?format=TEXT".try_into()?)
        .build()?;

    let res = client
        .request(
            &req.head,
            Bytes::new(),
            &http::ClientRequestContext::default(),
        )
        .await?;

    let data = Bytes::from(base_radix::base64_decode(std::str::from_utf8(&res.body)?)?);
    let pem = crypto::pem::PEM::parse(data)?;

    let mut output_data = vec![];

    let mut cert_registry = crypto::x509::CertificateRegistry::new();
    for entry in pem.entries {
        let cert_original_data = Bytes::from(entry.to_binary()?);

        let cert = Arc::new(crypto::x509::Certificate::read(cert_original_data.clone())?);

        let reencoded = cert.raw.to_der();
        if &reencoded != &cert_original_data {
            return Err(err_msg("Non-lossy write"));
        }

        // TODO: Validate the certificate in its initial state.

        cert_registry.append(&[cert.clone()], true)?;

        let mut raw = cert.raw.clone();
        // Clear signatures as we don't need them for trusted certificates.
        // raw.signature = BitString::from(BitVector::new());

        let cert_data = raw.to_der();
        output_data.extend_from_slice(&(cert_data.len() as u32).to_le_bytes());
        output_data.extend_from_slice(&cert_data);
    }

    file::write(
        project_path!("third_party/chromium/root_store.bin"),
        &output_data,
    )
    .await?;

    println!("Wrote {} bytes", output_data.len());

    Ok(())
}
