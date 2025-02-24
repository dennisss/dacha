mod certificate;
mod certificate_builder;
mod certificate_list;
mod certificate_registry;
mod certificate_request;
mod certificate_request_builder;
mod certificate_verified;
mod name_constraints;
mod private_key;
mod public_key;
mod signature_algorithm;
mod signature_key;

pub use certificate::*;
pub use certificate_builder::*;
pub use certificate_registry::*;
pub use certificate_request::*;
pub use certificate_request_builder::*;
pub use certificate_verified::*;
pub use private_key::*;
pub use public_key::*;
pub use signature_key::SignatureKeyConstraints;

// NOTE: This field MUST contain the same algorithm identifier as the
//    signature field in the sequence tbsCertificate

/*
Wrapper for reading a certificate
- Need map to know about unknown extensions

*/

/*
TODO: We can get root certificates from
https://android.googlesource.com/platform/system/ca-certificates/+/master/
*/

// TODO: Also verify that we can't use a duplicate key id to bypass the
// signature check.

// TODO: Must also deal with possible cycles.
// Just limit max chain length.

// NOTE: Here is how OpenSSL does Name hashing:
// https://github.com/openssl/openssl/blob/47b4ccea9cb9b924d058fd5a8583f073b7a41656/crypto/x509/x509_cmp.c#L184

// TODO: Read https://tools.ietf.org/html/rfc5280#section-7.1 for the exact
// rules for comparing names for the purpose of

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Read;
    use std::sync::Arc;

    use common::bytes::Bytes;
    use common::errors::*;

    #[testcase]
    async fn x509_google_cert_test() -> Result<()> {
        let read_file = |path| -> Result<Arc<Certificate>> {
            let mut f = std::fs::File::open(path)?;

            let mut data = vec![];
            f.read_to_end(&mut data)?;

            let buf = Bytes::from(data);
            let cert = Certificate::read(buf)?;
            Ok(Arc::new(cert))
        };

        let cert = read_file(project_path!("testdata/x509/google.der")).unwrap();
        let cert2 = read_file(project_path!("testdata/x509/gts.der")).unwrap();

        let mut reg = CertificateRegistry::public_roots().await?;
        reg.append(&[cert, cert2], false)?;

        // let san = cert.subject_alt_name().unwrap().unwrap();

        // println!("{:#?}", cert);
        // println!("Authority: {:?}", cert.authority_key_id().unwrap());
        // println!("Subject: {:?}", cert.subject_key_id());
        // println!("{}", cert.issuer().to_string().unwrap());
        // println!("{}", cert.subject().to_string().unwrap());

        Ok(())
    }

    #[testcase]
    async fn x509_pem_test() -> Result<()> {
        let mut buf = file::read(project_path!("testdata/certificates/server.crt")).await?;

        let certs = Certificate::from_pem(buf.into())?;
        println!("{:#?}", certs);

        Ok(())
    }

    #[testcase]
    async fn x509_registry() -> Result<()> {
        CertificateRegistry::public_roots().await?;
        Ok(())
    }
}
