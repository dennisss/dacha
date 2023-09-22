use std::collections::HashMap;

use common::errors::*;
use crypto::{elliptic::EllipticCurveGroup, hasher::Hasher};
use math::integer::Integer;

pub fn generate_jwk(key: &crypto::x509::PublicKey) -> Result<json::Value> {
    let mut obj = json::Value::Object(HashMap::new());

    // NOTE: We only include the minimal keys required to make thumbprint generation
    // easier: https://datatracker.ietf.org/doc/html/rfc7638
    match key {
        crypto::x509::PublicKey::RSA(_) => todo!(),
        crypto::x509::PublicKey::RSASSA_PSS(_, _) => todo!(),
        crypto::x509::PublicKey::EC(_, group, key) => {
            // See https://www.rfc-editor.org/rfc/rfc7518.html

            let curve = {
                if group == &EllipticCurveGroup::secp256r1() {
                    "P-256"
                } else {
                    return Err(err_msg("Unsupported curve"));
                }
            };

            obj.set_field("kty", "EC");
            obj.set_field("crv", curve);

            let point = group.decode_point(&key)?;

            obj.set_field("x", base_radix::base64url_encode(&point.x.to_be_bytes()));
            obj.set_field("y", base_radix::base64url_encode(&point.y.to_be_bytes()));
        }
        crypto::x509::PublicKey::Ed25519(key) => {
            obj.set_field("kty", "OKP");
            obj.set_field("crv", "Ed25519");
            obj.set_field("x", base_radix::base64url_encode(&key));
        }
    }

    Ok(obj)
}

pub fn generate_jwk_thumbprint(key: &crypto::x509::PublicKey) -> Result<String> {
    let value = generate_jwk(key)?;

    let serialized_value = {
        let mut options = json::StringifyOptions::default();
        options.sort_fields = true;
        json::Stringifier::run(&value, options)
    };

    let mut hasher = crypto::sha256::SHA256Hasher::default();
    hasher.update(serialized_value.as_bytes());
    let hash = hasher.finish();

    Ok(base_radix::base64url_encode(&hash))
}
