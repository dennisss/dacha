use common::errors::*;
use crypto::{rsa::RSAPrivateKey, x509::PrivateKey};
use reflection::ParseFrom;

pub struct GoogleServiceAccount {
    pub(crate) data: GoogleServiceAccountData,
    pub(crate) private_key: RSAPrivateKey,
}

#[derive(Parseable)]
#[parse(allow_unknown = true)]
pub(crate) struct GoogleServiceAccountData {
    pub(crate) project_id: String,

    pub(crate) private_key_id: String,

    pub(crate) private_key: String,

    pub(crate) client_id: String,

    pub(crate) client_email: String,
}

impl GoogleServiceAccount {
    pub async fn load_from_environment() -> Result<Self> {
        // TODO: Try to find a file in the GOOGLE_APPLICATION_CREDENTIALS environment
        // variable.

        todo!()
    }

    pub fn parse_json(service_account_json: &str) -> Result<Self> {
        let json_object = json::parse(service_account_json)?;
        if json_object.get_field("type").and_then(|v| v.get_string()) != Some("service_account") {
            return Err(err_msg("Unknown type of service account json file"));
        }

        let data = GoogleServiceAccountData::parse_from(json::ValueParser::new(&json_object))?;

        let private_key = match crypto::x509::PrivateKey::from_pem(data.private_key.clone().into())?
        {
            PrivateKey::RSA(x) => x,
            _ => {
                return Err(err_msg(
                    "Expected an RSA private key with the service account",
                ))
            }
        };

        Ok(Self { data, private_key })
    }
}
