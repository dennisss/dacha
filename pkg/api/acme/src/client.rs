use std::{
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::Duration,
};

use asn::encoding::DERWriteable;
use common::{bytes::Bytes, errors::*};
use crypto_jwt::{generate_jwk_thumbprint, JWTAlgorithm};
use executor::bundle::TaskResultBundle;
use http::ResponseHead;
use reflection::{ParseFrom, SerializeTo};

use crate::ACMEChallengeSolver;

// TODO: Most of these still need to be implemented.
pub struct ACMEClientOptions {
    /// TODO: Implement this.
    pub preferred_algorithms: Vec<JWTAlgorithm>,
    /*
    /// Minimum lifetime (starting when the certificate is requested) that we
    /// want for a certificate.
    ///
    /// If there are already orders on the ACME server that could fulfill this,
    /// we will prefer to work on them rather than creating new orders.
    pub min_certificate_age: Duration,

    /// Upper bound on how long we want certificates to last.
    ///
    /// TODO: Implement this by setting notAfter in the newOrder payload.
    pub max_certificate_age: Duration,

    /// When an ACME order gets into the 'pending' state, we will only try to
    /// complete it if there is at least this much time left before it expires.
    ///
    /// (this is the time we think we'll need to setup any challenges like DNS
    /// record updates).
    pub min_pending_time: Duration,

    /// When an ACME order gets into the 'valid' state (just waiting to be
    /// finalized), the min time we require for sending the finalization
    /// request.
    pub min_valid_time: Duration,
    */
}

impl Default for ACMEClientOptions {
    fn default() -> Self {
        Self {
            preferred_algorithms: vec![],
            // min_certificate_age: Duration::from_secs(60 * 60 * 24 * 30), // 30 days
            // max_certificate_age: Duration::from_secs(60 * 60 * 24 * 90), // 90 days
            // min_pending_time: Duration::from_secs(60 * 60),              // 1 hour
            // min_valid_time: Duration::from_secs(10 * 60),                // 10 minutes
        }
    }
}

/// One instance of this is meant to be used per account.
pub struct ACMEClient {
    client: http::SimpleClient,
    directory: DirectoryUrls,
    options: ACMEClientOptions,
    solvers: Vec<Arc<dyn ACMEChallengeSolver>>,
    account_private_key: crypto::x509::PrivateKey,
}

struct Session {
    signature_algorithm: crypto_jwt::JWTAlgorithm,

    next_nonce: Option<String>,
    account_url: Option<String>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
struct DirectoryUrls {
    keyChange: String,
    newAccount: String,
    newNonce: String,
    newOrder: String,
    renewalInfo: String,
    revokeCert: String,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
struct NewAccountPayload {
    termsOfServiceAgreed: Option<bool>,
    onlyReturnExisting: Option<bool>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
struct AccountObject {
    status: String,
    orders: Option<String>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
struct OrdersList {
    orders: Vec<String>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
struct NewOrderPayload {
    identifiers: Vec<Identifier>,
    // notBefore: String,
    // notAfter: String,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
struct OrderObject {
    // "pending", "ready", "processing", "valid", and "invalid"
    status: String,
    expires: String,
    notBefore: Option<String>,
    notAfter: Option<String>,
    identifiers: Vec<Identifier>,

    authorizations: Vec<String>,

    finalize: String,
    certificate: Option<String>,
}

#[derive(Clone, Parseable, Debug, Hash, PartialEq, Eq)]
#[parse(allow_unknown = true)]
struct Identifier {
    #[parse(name = "type")]
    typ: String,
    value: String,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct AuthorizationObject {
    status: String,
    identifier: Identifier,
    expires: String,
    challenges: Vec<ChallengeObject>,
    wildcard: Option<bool>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct ChallengeObject {
    #[parse(name = "type")]
    typ: String,

    url: String,

    token: String,

    status: Option<String>,
}

impl ACMEClient {
    /// NOTE: The account private key is only used for authentication with the
    /// ACME server and MUST be different than the key used for signing CSRs.
    pub async fn create(
        directory_url: &str,
        solvers: Vec<Arc<dyn ACMEChallengeSolver>>,
        account_private_key: &crypto::x509::PrivateKey,
        options: ACMEClientOptions,
    ) -> Result<Self> {
        let client = http::SimpleClient::new(http::SimpleClientOptions::default());

        let dir_request = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri(directory_url)
            .build()?;

        let dir_response = client
            .request(
                &dir_request.head,
                Bytes::new(),
                &http::ClientRequestContext::default(),
            )
            .await?;

        let dir_json = json::parse(std::str::from_utf8(&dir_response.body)?)?;

        let directory = DirectoryUrls::parse_from(json::ValueParser::new(&dir_json))?;

        Ok(Self {
            client,
            directory,
            options,
            solvers,
            account_private_key: account_private_key.clone(),
        })
    }

    /// Returns the generated certificate as a full PEM encoded chain.
    pub async fn request_certificate(
        &self,
        csr: &crypto::x509::CertificateRequest,
    ) -> Result<String> {
        let mut session = Session {
            // TOOD: Derive the default from the key type and allow some user customization
            signature_algorithm: crypto_jwt::JWTAlgorithm::ES256,
            next_nonce: None,
            account_url: None,
        };

        let identifiers = {
            let mut idents = HashSet::new();

            idents.insert(Identifier {
                typ: "dns".to_string(),
                value: csr
                    .common_name()?
                    .ok_or_else(|| err_msg("CSR missing a common name"))?
                    .to_string(),
            });

            if let Some(san) = csr.subject_alt_name()? {
                for name in &san.items {
                    if let pkix::PKIX1Implicit88::GeneralName::dNSName(s) = name {
                        idents.insert(Identifier {
                            typ: "dns".to_string(),
                            value: s.as_str().to_string(),
                        });
                    } else {
                        return Err(err_msg(
                            "ACME client doesn't supprot non-DNS identifiers in SubjectAltName",
                        ));
                    }
                }
            }

            idents
        };

        // Find or create an account.
        let (new_account_head, account) = self
            .send_authenticated_json_request::<_, AccountObject>(
                &self.directory.newAccount,
                NewAccountPayload {
                    termsOfServiceAgreed: Some(true),
                    onlyReturnExisting: None,
                },
                &mut session,
            )
            .await?;

        let account_url = new_account_head
            .headers
            .get_one("Location")?
            .ok_or_else(|| err_msg("newAccount response missing account location"))?
            .value
            .to_ascii_str()?
            .to_string();
        session.account_url = Some(account_url.clone());

        // Re-fetch account object.
        let (_, account) = self
            .send_authenticated_json_request::<_, AccountObject>(
                account_url.as_str(),
                (),
                &mut session,
            )
            .await?;

        // NOTE: Let's Encrypt doesn't seem to support querying the list of orders but
        // is smart enough to return the same order if a duplicate is requested.
        //
        // See https://github.com/letsencrypt/boulder/issues/3335
        // println!("Account: {:#?}", account);

        // TODO: Implement 'orders' lookup and re-use to support other less ACME
        // servers more gracefully.

        /*
        let now = common::chrono::Utc::now();
        let not_after =
            now + common::chrono::Duration::from_std(self.options.max_certificate_age).unwrap();
        */

        let (mut order_head, mut order) = self
            .send_authenticated_json_request::<_, OrderObject>(
                &self.directory.newOrder,
                NewOrderPayload {
                    identifiers: identifiers.iter().cloned().collect::<Vec<_>>(),
                    // notBefore: now.to_rfc3339(),
                    // notAfter: not_after.to_rfc3339(),
                },
                &mut session,
            )
            .await?;
        let order_url = order_head
            .headers
            .get_one("Location")?
            .ok_or_else(|| err_msg("newOrder response missing account location"))?
            .value
            .to_ascii_str()?
            .to_string();

        // println!("Order URL: {:#?}", order_url);
        // println!("Order: {:#?}", order);

        let mut have_solved_challenges = false;

        // Normally should take:
        // - 1 round to solve challenges
        // - 1 round to finalize
        // - 1 round for processing
        // - 1 round to notice that it is 'valid'
        for _ in 0..12 {
            // Re-check the order state.
            (order_head, order) = self
                .send_authenticated_json_request(&order_url, (), &mut session)
                .await?;

            // println!("Now: {:?}", std::time::SystemTime::now());
            // println!("{:?}", order_head);
            // println!("Now Order: {:?}", order);

            match order.status.as_str() {
                "pending" => {
                    if have_solved_challenges {
                        eprintln!("Already solved challenges but order is still pending. Waiting a bit more...");
                        executor::sleep(Duration::from_secs(10)).await?;
                    } else {
                        self.solve_challenges(&order, &identifiers, &mut session)
                            .await?;

                        have_solved_challenges = true;
                    }
                }
                "ready" => {
                    let mut req = json::Value::Object(HashMap::new());
                    req.set_field("csr", base_radix::base64url_encode(&csr.raw().to_der()));

                    order = self
                        .send_authenticated_json_request(&order.finalize, req, &mut session)
                        .await?
                        .1;
                }
                "processing" => {
                    // TODO: Sleep for the Retry-After response header period.
                    executor::sleep(Duration::from_secs(5)).await?;
                }
                "valid" => {
                    break;
                }
                "invalid" => {}
                s @ _ => {
                    return Err(format_err!("Unknown order status: {}", s));
                }
            }
        }

        if order.status != "valid" {
            return Err(format_err!(
                "Order not yet valid after all attempts. At: {}",
                order.status
            ));
        }

        let certificate_url = order
            .certificate
            .ok_or_else(|| err_msg("Order valid but has no certicate url"))?;

        let res = self
            .send_authenticated_request(&certificate_url, b"", &mut session)
            .await?;
        // TODO: Expected content type is "application/pem-certificate-chain"

        Ok(String::from_utf8(res.body.to_vec())?)
    }

    async fn solve_challenges(
        &self,
        order: &OrderObject,
        requested_identifiers: &HashSet<Identifier>,
        session: &mut Session,
    ) -> Result<()> {
        let public_key_thumbnail =
            generate_jwk_thumbprint(&self.account_private_key.public_key()?)?;

        let mut solver_tasks = TaskResultBundle::new();
        let mut challenge_urls = vec![];

        for auth in &order.authorizations {
            let (_, obj) = self
                .send_authenticated_json_request::<_, AuthorizationObject>(
                    auth.as_str(),
                    (),
                    session,
                )
                .await?;

            if obj.identifier.typ != "dns" {
                return Err(err_msg("Expected to only be validating DNS identifiers"));
            }

            if obj.identifier.value.contains("*") {
                return Err(err_msg(
                    "Server returned identifiers should not include wildcards",
                ));
            }

            let dns_name = obj.identifier.value.clone();

            {
                let full_value = if obj.wildcard == Some(true) {
                    format!("*.{}", dns_name)
                } else {
                    dns_name.to_string()
                };

                let full_ident = Identifier {
                    typ: "dns".to_string(),
                    value: full_value,
                };
                if !requested_identifiers.contains(&full_ident) {
                    return Err(format_err!(
                        "Server requested authorize non-requested identifier: {:?}",
                        full_ident
                    ));
                }
            }

            // NOTE: The expectation is that if any individual challenge fails, that should
            // trigger the entire authorization and then the entire order to get into a
            // failing state. So newOrder should always return a non-failing order.
            match obj.status.as_str() {
                "pending" => {}
                "valid" => {
                    continue;
                }
                "invalid" | "revoked" | "expired" => {
                    return Err(format_err!(
                        "Authorization in bad terminal state: {}",
                        obj.status
                    ));
                }
                s @ _ => {
                    return Err(format_err!("Unknown authorization status: {}", s));
                }
            }

            let mut found = false;

            for challenge in &obj.challenges {
                let key_authorization = format!("{}.{}", challenge.token, public_key_thumbnail);

                for solver in &self.solvers {
                    if solver.challenge_type() == challenge.typ {
                        let dns_name = dns_name.clone();
                        let solver = solver.clone();
                        solver_tasks.add("Solver", async move {
                            solver.solve_challenge(&dns_name, &key_authorization).await
                        });

                        challenge_urls.push(challenge.url.clone());

                        found = true;
                        break;
                    }
                }

                if found {
                    break;
                }
            }

            if !found {
                return Err(format_err!("No challenge solver found for type: {:?}", obj));
            }

            println!("{:#?}", obj);
        }

        solver_tasks.join().await?;

        for url in challenge_urls {
            self.send_authenticated_request(&url, b"{}", session)
                .await?;
        }

        Ok(())
    }

    async fn send_authenticated_json_request<
        Request: SerializeTo,
        Response: for<'a> ParseFrom<'a>,
    >(
        &self,
        url: &str,
        payload: Request,
        session: &mut Session,
    ) -> Result<(http::ResponseHead, Response)> {
        let payload = json::stringify(&payload)?;

        let res = self
            .send_authenticated_request(url, payload.as_bytes(), session)
            .await?;

        let value = json::parse(std::str::from_utf8(&res.body)?)?;

        let object = Response::parse_from(json::ValueParser::new(&value))?;

        Ok((res.head, object))
    }

    /// NOTE: Any http errors will be converted into an Error in the return
    /// value.
    async fn send_authenticated_request(
        &self,
        url: &str,
        payload: &[u8],
        session: &mut Session,
    ) -> Result<http::BufferedResponse> {
        let nonce = match session.next_nonce.take() {
            Some(v) => v,
            None => self.new_nonce().await?,
        };

        let req = http::RequestBuilder::new()
            .method(http::Method::POST)
            .uri(url)
            .header("Content-Type", "application/jose+json")
            .build()?;

        let body = crypto_jwt::JWSBuilder::new()
            .set_protected_field("nonce", nonce)
            .set_protected_field("url", url)
            .build(
                payload,
                session.signature_algorithm,
                &self.account_private_key,
                session.account_url.as_ref().map(|s| s.as_str()),
            )
            .await?;

        let res = self
            .client
            .request(
                &req.head,
                body.into(),
                &http::ClientRequestContext::default(),
            )
            .await?;

        // TODO: Handle responses with a "Retry-After" header
        // (backoff based on the requested duration up to some max limit and fully retry
        // rate limit errors).

        session.next_nonce = self.get_nonce_header(&res)?;

        if res.head.status_code != http::status_code::OK
            && res.head.status_code != http::status_code::CREATED
        {
            // Error responses are supported to be JSON.
            // TODO: Check the content type first.
            let body = std::str::from_utf8(&res.body)?;

            return Err(format_err!(
                "ACME request failed [{:?}]: {}",
                res.head.status_code,
                body
            ));
        }

        Ok(res)
    }

    async fn new_nonce(&self) -> Result<String> {
        // NOTE: This is the only ACME request which doesn't use POST and doesn't need
        // to be authenticated.

        let req = http::RequestBuilder::new()
            .method(http::Method::HEAD)
            .uri(self.directory.newNonce.as_str())
            .build()?;

        let res = self
            .client
            .request(
                &req.head,
                Bytes::new(),
                &http::ClientRequestContext::default(),
            )
            .await?;

        if res.head.status_code != http::status_code::OK {
            return Err(format_err!(
                "newNonce returned non-OK status: {:?}",
                res.head.status_code
            ));
        }

        self.get_nonce_header(&res)
            .and_then(|v| v.ok_or_else(|| err_msg("newNonce response missing nonce")))
    }

    fn get_nonce_header(&self, response: &http::BufferedResponse) -> Result<Option<String>> {
        match response.head.headers.get_one("Replay-Nonce")? {
            Some(v) => Ok(Some(v.value.to_ascii_str()?.to_string())),
            None => Ok(None),
        }
    }
}
