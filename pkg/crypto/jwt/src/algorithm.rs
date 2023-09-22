use asn::encoding::Any;
use asn::encoding::DERWriteable;
use common::errors::*;
use crypto::{elliptic::EllipticCurveGroup, x509::SignatureKeyConstraints};
use pkix::{PKIX1Algorithms2008, PKIX1Explicit88, PKIX1_PSS_OAEP_Algorithms, Safecurves_pkix_18};

// Most of these are defined in
// https://www.rfc-editor.org/rfc/rfc7518.html
enum_def!(JWTAlgorithm str =>
    // The signature is just an empty string.
    None = "none",

    // HMAC SHA-256
    HS256 = "HS256",

    // RSASSA-PKCS1-v1_5 with SHA-256
    RS256 = "RS256",

    // ECDSA using the P-256 curve and SHA-256
    ES256 = "ES256",

    EdDSA = "EdDSA"
);

impl JWTAlgorithm {
    pub fn to_x509_signature_id(
        &self,
        // key: &crypto::x509::PrivateKey,
    ) -> Option<(
        PKIX1Explicit88::AlgorithmIdentifier,
        SignatureKeyConstraints,
    )> {
        let mut constraints = SignatureKeyConstraints::default();

        let algo = match self {
            JWTAlgorithm::None => return None,
            JWTAlgorithm::HS256 => todo!(),
            JWTAlgorithm::RS256 => {
                // NOTE: The salt length should be the same the hash function length.
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1_PSS_OAEP_Algorithms::ID_RSASSA_PSS,
                    parameters: Some(asn_any!(PKIX1_PSS_OAEP_Algorithms::RSASSA_PSS_params {
                        hashAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::SHA256IDENTIFIER)
                            .clone()
                            .into(),
                        maskGenAlgorithm: (*PKIX1_PSS_OAEP_Algorithms::MGF1SHA256IDENTIFIER)
                            .clone()
                            .into(),
                        saltLength: (256 / 8).into(),
                        trailerField: 1.into(),
                    })),
                }
            }
            JWTAlgorithm::ES256 => {
                constraints.ecdsa_group = Some(EllipticCurveGroup::secp256r1());
                constraints.ecdsa_signature_format =
                    Some(crypto::elliptic::EllipticCurveSignatureFormat::Concatenated);
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: PKIX1Algorithms2008::ECDSA_WITH_SHA256,
                    parameters: None,
                }
            }
            JWTAlgorithm::EdDSA => {
                // TODO: Need to check which curve is in the key.
                PKIX1Explicit88::AlgorithmIdentifier {
                    algorithm: Safecurves_pkix_18::ID_ED25519,
                    parameters: None,
                }
            }
        };

        Some((algo, constraints))
    }
}
