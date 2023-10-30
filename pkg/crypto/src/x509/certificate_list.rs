use asn::encoding::{der_eq, DERReadable, DERReader};
use common::bytes::Bytes;
use common::errors::*;

pub struct CertificateList {
    raw: pkix::PKIX1Explicit88::CertificateList,

    plaintext: Bytes,
}

impl CertificateList {
    fn new(raw: pkix::PKIX1Explicit88::CertificateList, plaintext: Bytes) -> Result<Self> {
        if !der_eq(&raw.signatureAlgorithm, &raw.tbsCertList.signature) {
            return Err(err_msg("Mismatching signature algorithms"));
        }

        Ok(Self { raw, plaintext })
    }

    /// Reads a certficate list from DER encoded data.
    pub fn read(buf: Bytes) -> Result<Self> {
        // TODO: Ensure the buffer is read till completion.
        let mut r = DERReader::new(buf);
        let raw = pkix::PKIX1Explicit88::CertificateList::read_der(&mut r)?;

        Self::new(raw, r.slices[1].clone())
    }
}

/*
Conforming CAs must:
- Make it V2
- Include CRL number extensoin
- Include AuthorityKeyIdentifier extension


- Issuer is who siegned it.

InvalidityDate extension on entries is very useful for invalidating stuff.
CRLReason extension on entries (non-critical)

*/
