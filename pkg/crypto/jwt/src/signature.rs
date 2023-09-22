// Builds a JWS per https://datatracker.ietf.org/doc/html/rfc7515

use std::collections::HashMap;

use asn::encoding::DERReadable;
use common::errors::*;

use crate::{algorithm::JWTAlgorithm, generate_jwk};

pub struct JWSBuilder {
    protected: json::Value,
    // header: json::Value,
}

impl JWSBuilder {
    pub fn new() -> Self {
        let mut protected = json::Value::Object(HashMap::new());
        // let mut header = json::Value::Object(HashMap::new());
        let mut payload = json::Value::Object(HashMap::new());

        Self {
            protected,
            // header,
            // payload,
        }
    }

    pub fn set_protected_field<V: Into<json::Value>>(mut self, name: &str, value: V) -> Self {
        self.protected.set_field(name, value);
        self
    }

    /// Will build it in the flattened form.
    pub async fn build(
        mut self,
        payload: &[u8],
        algorithm: JWTAlgorithm,
        private_key: &crypto::x509::PrivateKey,
        key_id: Option<&str>,
    ) -> Result<String> {
        if let Some(kid) = key_id {
            self.protected.set_field("kid", kid);
        } else {
            self.protected
                .set_field("jwk", generate_jwk(&private_key.public_key()?)?);
        }

        self.protected.set_field("alg", algorithm.to_value());

        let protected = base_radix::base64url_encode(json::stringify(&self.protected)?.as_bytes());
        let payload = base_radix::base64url_encode(payload);

        let plaintext = format!("{}.{}", protected, payload);

        // TODO: Remove the unwrap and make the None algorithm behavior clear
        let (signature_algorithm, constraints) = algorithm.to_x509_signature_id().unwrap();

        let signature = private_key
            .create_signature(plaintext.as_bytes(), &signature_algorithm, &constraints)
            .await?;

        let mut output = json::Value::Object(HashMap::new());

        output.set_field("protected", protected);
        output.set_field("payload", payload);
        output.set_field("signature", base_radix::base64url_encode(&signature));

        Ok(json::stringify(&output)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[testcase]
    async fn check_ecdsa() -> Result<()> {
        let protected = "eyJhbGciOiJFUzI1NiJ9";
        let payload = "eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ";

        let x = base_radix::base64url_decode("f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU")?;
        let y = base_radix::base64url_decode("x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0")?;

        let mut key = vec![];
        key.push(4);
        key.extend_from_slice(&x);
        key.extend_from_slice(&y);

        Ok(())
    }
}
