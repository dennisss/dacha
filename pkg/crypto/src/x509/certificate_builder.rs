/*
TODO: For gneeral internet connectivity, I need to support CRLs
^ Eventually also check for certificate transparent

I want to be able to:

Support

- Specify:
    - Validity duration
    - DNS Name
    - DNS Name constraint (for children)
    - Public/Private Key (eliptic curve)
        - From the public
    - Whether or not it is allowed to be a CA
    - Either:
        - Self sign or provide another certificate to use for signing



*/

/*
Types of certificates we want to create:
- Self signed root CA
- Child CA possibly with DNS name constraints
- Leaf most non-CA with a SAN

Issuer:
- Should be unique for all our entities.
- Will be provided with just a 'CN' set to the DNS name

Extensions to use:
- SubjectKeyIdentifier
    - Use in ALL certified as SHA-1 hash of subjectPublicKey
    - MUST be not critical
- Key Usage
    - Must be present for CAs
    - Must be critical
- SubjectAlternativeName
    - Will contain a DNS name


[
    Certificate {
        validity: Validity {
            not_before: 2021-11-25T22:07:20Z,
            not_after: 2025-11-24T22:07:20Z,
        },
        plaintext: b"0\x82\x02!\xa0\x03\x02\x01\x02\x02\x14k\xb3S\xdf\xe2\x94y\xc1(\x01\xd3n\x9b\xfaI$\x89\xde\0\x190\r\x06\t*\x86H\x86\xf7\r\x01\x01\x0b\x05\00!1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x120\x10\x06\x03U\x04\x03\x0c\tlocalhost0\x1e\x17\r211125220720Z\x17\r251124220720Z0!1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x120\x10\x06\x03U\x04\x03\x0c\tlocalhost0\x82\x01\"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\0\x03\x82\x01\x0f\00\x82\x01\n\x02\x82\x01\x01\0\xc8\x7f\xcd;\x8e\x8f\xaaS\xfc\xb4\xc1\x80_\xa7\xfa/\x02\xf5\xe1G\xfb\xf9\xf5\x8b5\xdf\xc1\xd8\x17\x8da\xb6\x120\x011\xb1\xc4\x1dw\x1b\xf7\xb6\xf8Z\xb3R\x99|?\x87\xc5\xe8L\xbf\x98\xb7\xdco{WJ\x95B\xed\01\x1a\xc8\x8e \x81\xc2\xc3\x01\x7f\x97\x8d\xc3y]\xb0k\x94\xb6\xaf\xa1\xf0,)\xdc\x90\x05<\xe3~\xe9\x9d\x99!.FQ\xce\x83\xa7\x08\\\xf9H\xcaq\xcaQ:\xde\xa3\x9fF\xa3\xe5\x82=\x943\xe6 h\x9c\xe0\xce\x97&\xb2\xf18DF\x01$\x14L{\xa7oX<c\x99z\x14G\xba\xa9\x15\xecK2\xf6xStz\xe9+\x0c\xef^\t\x15\xd1?\x84,/G\x9dP\xda\x98\xab\xfd\xb5\\{S\xc2\x10_v!\xd1n\xa7Z\xa9p\xe9I\xd3\xcc\xc0G\\\xa6$\xf0GR_\x9ad\xbf1\x1e\xcfZ\xbc\x96\xb4E\xdcx|\xd2\xf5\x9b'\xf9;\xca1\x99\xf9s3e)\x8c!\xac!@3\xcc\x9dP\xd1\x1a\xa0\x9f\x90\x95\x8e\xf5=\x02\x03\x01\0\x01\xa3i0g0\x1d\x06\x03U\x1d\x0e\x04\x16\x04\x14\xb13+\xd6\x8a7>\x9d\x91\x1d\x92q/\xe3D\x88\x9b?\xb1.0\x1f\x06\x03U\x1d#\x04\x180\x16\x80\x14\xb13+\xd6\x8a7>\x9d\x91\x1d\x92q/\xe3D\x88\x9b?\xb1.0\x0f\x06\x03U\x1d\x13\x01\x01\xff\x04\x050\x03\x01\x01\xff0\x14\x06\x03U\x1d\x11\x04\r0\x0b\x82\tlocalhost",
        subject_key_id: b"\xb13+\xd6\x8a7>\x9d\x91\x1d\x92q/\xe3D\x88\x9b?\xb1.",
        extensions: CertificateExtensions {
            map: {
                [2.5.29.14]: b"\x04\x14\xb13+\xd6\x8a7>\x9d\x91\x1d\x92q/\xe3D\x88\x9b?\xb1.",
                [2.5.29.35]: b"0\x16\x80\x14\xb13+\xd6\x8a7>\x9d\x91\x1d\x92q/\xe3D\x88\x9b?\xb1.",
                [2.5.29.17]: b"0\x0b\x82\tlocalhost",
                [2.5.29.19]: b"0\x03\x01\x01\xff",
            },
        },
        raw: Certificate {
            tbsCertificate: TBSCertificate {
                version: v3,
                serialNumber: CertificateSerialNumber {
                    value: 614861152372564426562463467260188032050818580505,
                },
                signature: AlgorithmIdentifier {
                    algorithm: [1.2.840.113549.1.1.11],
                    parameters: Some(
                        Any(Element { ident: Identifier { tag: Tag { class: Universal, number: 5 }, constructed: false }, len: Short(0), data: b"", outer: b"\x05\0" }),
                    ),
                },
                issuer: rdnSequence(
                    RDNSequence {
                        value: SequenceOf {
                            items: [
                                RelativeDistinguishedName {
                                    value: SetOf {
                                        items: [
                                            AttributeTypeAndValue {
                                                typ: AttributeType {
                                                    value: [2.5.4.6],
                                                },
                                                value: AttributeValue {
                                                    value: Any(Element { ident: Identifier { tag: Tag { class: Universal, number: 19 }, constructed: false }, len: Short(2), data: b"US", outer: b"\x13\x02US" }),
                                                },
                                            },
                                        ],
                                    },
                                },
                                RelativeDistinguishedName {
                                    value: SetOf {
                                        items: [
                                            AttributeTypeAndValue {
                                                typ: AttributeType {
                                                    value: [2.5.4.3],
                                                },
                                                value: AttributeValue {
                                                    value: Any(Element { ident: Identifier { tag: Tag { class: Universal, number: 12 }, constructed: false }, len: Short(9), data: b"localhost", outer: b"\x0c\tlocalhost" }),
                                                },
                                            },
                                        ],
                                    },
                                },
                            ],
                        },
                    },
                ),
                validity: Validity {
                    notBefore: utcTime(
                        UTCTime { ... },
                    ),
                    notAfter: utcTime(
                        UTCTime { ... },
                    ),
                },
                subject: rdnSequence(
                    RDNSequence {
                        value: SequenceOf {
                            items: [
                                RelativeDistinguishedName {
                                    value: SetOf {
                                        items: [
                                            AttributeTypeAndValue {
                                                typ: AttributeType {
                                                    value: [2.5.4.6],
                                                },
                                                value: AttributeValue {
                                                    value: Any(Element { ident: Identifier { tag: Tag { class: Universal, number: 19 }, constructed: false }, len: Short(2), data: b"US", outer: b"\x13\x02US" }),
                                                },
                                            },
                                        ],
                                    },
                                },
                                RelativeDistinguishedName {
                                    value: SetOf {
                                        items: [
                                            AttributeTypeAndValue {
                                                typ: AttributeType {
                                                    value: [2.5.4.3],
                                                },
                                                value: AttributeValue {
                                                    value: Any(Element { ident: Identifier { tag: Tag { class: Universal, number: 12 }, constructed: false }, len: Short(9), data: b"localhost", outer: b"\x0c\tlocalhost" }),
                                                },
                                            },
                                        ],
                                    },
                                },
                            ],
                        },
                    },
                ),
                subjectPublicKeyInfo: SubjectPublicKeyInfo {
                    algorithm: AlgorithmIdentifier {
                        algorithm: [1.2.840.113549.1.1.1],
                        parameters: Some(
                            Any(Element { ident: Identifier { tag: Tag { class: Universal, number: 5 }, constructed: false }, len: Short(0), data: b"", outer: b"\x05\0" }),
                        ),
                    },
                    subjectPublicKey: BitString {
                        data: '...',
                    },
                },
                issuerUniqueID: None,
                subjectUniqueID: None,
                extensions: Some(
                    Extensions {
                        value: SequenceOf {
                            items: [
                                Extension {
                                    extnID: [2.5.29.14], // Subject Key Identifier
                                    critical: false,
                                    extnValue: OctetString(
                                        b"\x04\x14\xb13+\xd6\x8a7>\x9d\x91\x1d\x92q/\xe3D\x88\x9b?\xb1.",
                                    ),
                                },
                                Extension {
                                    extnID: [2.5.29.35], // Authority key identity.
                                    critical: false,
                                    extnValue: OctetString(
                                        b"0\x16\x80\x14\xb13+\xd6\x8a7>\x9d\x91\x1d\x92q/\xe3D\x88\x9b?\xb1.",
                                    ),
                                },
                                Extension {
                                    extnID: [2.5.29.19], // Basic constraints. ()
                                    critical: true,
                                    extnValue: OctetString(
                                        b"0\x03\x01\x01\xff",
                                    ),
                                },
                                Extension { // SAN
                                    extnID: [2.5.29.17],
                                    critical: false,
                                    extnValue: OctetString(
                                        b"0\x0b\x82\tlocalhost",
                                    ),
                                },
                            ],
                        },
                    },
                ),
            },
            signatureAlgorithm: AlgorithmIdentifier {
                algorithm: [1.2.840.113549.1.1.11],
                parameters: Some(
                    Any(Element { ident: Identifier { tag: Tag { class: Universal, number: 5 }, constructed: false }, len: Short(0), data: b"", outer: b"\x05\0" }),
                ),
            },
            signature: BitString {
                data: '...',
            },
        },
    },
]

*/
