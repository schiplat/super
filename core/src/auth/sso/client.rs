//! Minimal OIDC client: discovery, authorization URL, code exchange, JWKS,
//! and ID-token verification.
//!
//! Deliberate choices:
//! - no redirects are followed (a redirect on a discovery/token/JWKS fetch is
//!   a misconfiguration or an attack, not a convenience)
//! - every URL passes the outbound SSRF policy and every response is capped
//! - discovery is validated against the configured issuer (mix-up defence)
//! - an unknown `kid` triggers exactly one rate-limited JWKS refetch, so a
//!   forged kid cannot be used to hammer the IdP

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use common::auth_sso::{SsoConfig, TokenAuthMethod};
use common::security::{FetchUrlPolicy, validate_outbound_url};
use jsonwebtoken::jwk::JwkSet;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::idtoken::{self, ExpectedToken, VerifiedIdentity, VerifyError};

const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const DISCOVERY_TTL: Duration = Duration::from_secs(60 * 60);
const JWKS_TTL: Duration = Duration::from_secs(60 * 60);
const FORCED_JWKS_MIN_INTERVAL: Duration = Duration::from_secs(60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OidcError {
    Network(String),
    Protocol(String),
    InvalidToken(String),
    UnknownKid(String),
    Idp {
        error: String,
        description: Option<String>,
    },
    Misconfigured(String),
}

impl std::fmt::Display for OidcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Protocol(e) => write!(f, "protocol error: {e}"),
            Self::InvalidToken(e) => write!(f, "invalid ID token: {e}"),
            Self::UnknownKid(kid) => write!(f, "unknown signing key {kid:?}"),
            Self::Idp { error, description } => match description {
                Some(description) => write!(f, "IdP error {error}: {description}"),
                None => write!(f, "IdP error {error}"),
            },
            Self::Misconfigured(e) => write!(f, "misconfigured: {e}"),
        }
    }
}

impl std::error::Error for OidcError {}

#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    #[serde(default)]
    pub grant_types_supported: Option<Vec<String>>,
    #[serde(default)]
    pub response_types_supported: Option<Vec<String>>,
    #[serde(default)]
    pub code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct IdpErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

struct Cached<T> {
    value: T,
    at: Instant,
}

impl<T> Cached<T> {
    fn fresh(&self, ttl: Duration) -> bool {
        self.at.elapsed() < ttl
    }
}

/// One client per configuration snapshot. Rebuilt when the configuration
/// changes, which also discards cached discovery/JWKS — correct, because an
/// issuer change must not reuse the previous issuer's keys.
pub struct OidcClient {
    http: reqwest::Client,
    config: Arc<SsoConfig>,
    secret: Arc<str>,
    previous_secret: Option<Arc<str>>,
    discovery: Mutex<Option<Cached<Discovery>>>,
    jwks: Mutex<Option<Cached<JwkSet>>>,
    forced_jwks_at: Mutex<Option<Instant>>,
}

impl OidcClient {
    pub fn new(
        config: Arc<SsoConfig>,
        secret: impl Into<String>,
        previous_secret: Option<String>,
    ) -> Result<Self, OidcError> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(HTTP_TIMEOUT)
            .user_agent(concat!("superd/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| OidcError::Network(e.to_string()))?;
        Ok(Self {
            http,
            config,
            secret: Arc::from(secret.into()),
            previous_secret: previous_secret.map(Arc::from),
            discovery: Mutex::new(None),
            jwks: Mutex::new(None),
            forced_jwks_at: Mutex::new(None),
        })
    }

    /// Cached discovery document.
    pub async fn discovery(&self) -> Result<Discovery, OidcError> {
        self.discovery_with(false).await
    }

    /// Fresh discovery probe for `POST /sso/test` and the save-time check.
    pub async fn probe(&self) -> Result<Discovery, OidcError> {
        self.discovery_with(true).await
    }

    async fn discovery_with(&self, force: bool) -> Result<Discovery, OidcError> {
        let mut cache = self.discovery.lock().await;
        if !force
            && let Some(cached) = cache.as_ref()
            && cached.fresh(DISCOVERY_TTL)
        {
            return Ok(cached.value.clone());
        }

        let url = format!(
            "{}/.well-known/openid-configuration",
            self.config.normalized_issuer()
        );
        match self.get_json::<Discovery>(&url, "discovery").await {
            Ok(discovery) => {
                self.validate_discovery(&discovery)?;
                *cache = Some(Cached {
                    value: discovery.clone(),
                    at: Instant::now(),
                });
                Ok(discovery)
            }
            Err(error) => {
                // Endpoints rarely change; a stale document keeps logins
                // working through a transient discovery outage.
                if let Some(cached) = cache.as_ref() {
                    tracing::warn!(error = %error, "discovery fetch failed; using cached document");
                    return Ok(cached.value.clone());
                }
                Err(error)
            }
        }
    }

    fn validate_discovery(&self, discovery: &Discovery) -> Result<(), OidcError> {
        let advertised = discovery.issuer.trim().trim_end_matches('/');
        if advertised != self.config.normalized_issuer() {
            return Err(OidcError::Misconfigured(format!(
                "discovery issuer {advertised:?} does not match the configured issuer {:?}",
                self.config.normalized_issuer()
            )));
        }
        validate_browser_endpoint(&discovery.authorization_endpoint, "authorization_endpoint")?;
        validate_outbound_url(&discovery.token_endpoint, FetchUrlPolicy::OidcEndpoint)
            .map_err(|e| OidcError::Misconfigured(format!("token_endpoint: {e}")))?;
        validate_outbound_url(&discovery.jwks_uri, FetchUrlPolicy::OidcEndpoint)
            .map_err(|e| OidcError::Misconfigured(format!("jwks_uri: {e}")))?;

        if let Some(methods) = &discovery.code_challenge_methods_supported
            && !methods.iter().any(|m| m == "S256")
        {
            return Err(OidcError::Misconfigured(
                "IdP does not advertise PKCE S256".into(),
            ));
        }
        if let Some(grants) = &discovery.grant_types_supported
            && !grants.iter().any(|g| g == "authorization_code")
        {
            return Err(OidcError::Misconfigured(
                "IdP does not advertise the authorization_code grant".into(),
            ));
        }
        if let Some(responses) = &discovery.response_types_supported
            && !responses.iter().any(|r| r == "code")
        {
            return Err(OidcError::Misconfigured(
                "IdP does not advertise the code response type".into(),
            ));
        }
        if let Some(methods) = &discovery.token_endpoint_auth_methods_supported
            && !methods
                .iter()
                .any(|m| m == self.config.token_auth_method.as_str())
        {
            return Err(OidcError::Misconfigured(format!(
                "IdP does not advertise {}",
                self.config.token_auth_method.as_str()
            )));
        }
        Ok(())
    }

    /// Authorization request URL. `state`, `nonce`, and the PKCE challenge are
    /// generated by the caller and persisted before this is used.
    pub async fn authorization_url(
        &self,
        state: &str,
        nonce: &str,
        challenge: &str,
        redirect_uri: &str,
    ) -> Result<String, OidcError> {
        let discovery = self.discovery().await?;
        let mut url = url::Url::parse(&discovery.authorization_endpoint)
            .map_err(|e| OidcError::Misconfigured(format!("authorization_endpoint: {e}")))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", &self.config.scopes)
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.to_string())
    }

    /// Exchange an authorization code, retrying once with the previous client
    /// secret while the rotation grace window is open.
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse, OidcError> {
        let discovery = self.discovery().await?;
        match self
            .post_token(
                &discovery.token_endpoint,
                code,
                verifier,
                redirect_uri,
                &self.secret,
            )
            .await
        {
            Err(OidcError::Idp { error, description }) if error == "invalid_client" => {
                let Some(previous) = self.previous_secret.as_deref() else {
                    return Err(OidcError::Idp { error, description });
                };
                tracing::warn!(
                    description = ?description,
                    "token endpoint rejected the current client secret; retrying with the \
                     previous secret inside the rotation window"
                );
                let response = self
                    .post_token(
                        &discovery.token_endpoint,
                        code,
                        verifier,
                        redirect_uri,
                        previous,
                    )
                    .await?;
                tracing::warn!(
                    "token exchange succeeded with the previous client secret — finish the \
                     rotation at the IdP"
                );
                Ok(response)
            }
            other => other,
        }
    }

    async fn post_token(
        &self,
        endpoint: &str,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        secret: &str,
    ) -> Result<TokenResponse, OidcError> {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "authorization_code")
            .append_pair("code", code)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("code_verifier", verifier);

        let mut request = self.http.post(endpoint).header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        );
        match self.config.token_auth_method {
            TokenAuthMethod::ClientSecretBasic => {
                // RFC 6749 §2.3.1: form-encode both parts before base64.
                let credentials = format!(
                    "{}:{}",
                    form_encode(&self.config.client_id),
                    form_encode(secret)
                );
                request = request.header(
                    reqwest::header::AUTHORIZATION,
                    format!("Basic {}", BASE64_STANDARD.encode(credentials)),
                );
            }
            TokenAuthMethod::ClientSecretPost => {
                form.append_pair("client_id", &self.config.client_id)
                    .append_pair("client_secret", secret);
            }
        }

        let response = request
            .body(form.finish())
            .send()
            .await
            .map_err(|e| OidcError::Network(format!("token endpoint: {e}")))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| OidcError::Network(format!("token endpoint: {e}")))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(OidcError::Protocol("token response is too large".into()));
        }
        if status.is_success() {
            return serde_json::from_slice(&bytes)
                .map_err(|e| OidcError::Protocol(format!("token response: {e}")));
        }
        Err(match serde_json::from_slice::<IdpErrorBody>(&bytes) {
            Ok(body) if !body.error.is_empty() => OidcError::Idp {
                error: body.error,
                description: body.error_description,
            },
            _ => OidcError::Protocol(format!("token endpoint returned HTTP {status}")),
        })
    }

    /// Verify an ID token and return the identity facts we keep.
    pub async fn verify_id_token(
        &self,
        id_token: &str,
        nonce: &str,
    ) -> Result<VerifiedIdentity, OidcError> {
        let discovery = self.discovery().await?;
        let issuer = self.config.normalized_issuer();
        let expected = ExpectedToken {
            issuer: &issuer,
            client_id: &self.config.client_id,
            nonce,
            groups_claim: &self.config.groups_claim,
        };

        let keys = self.jwks(&discovery.jwks_uri, false).await?;
        match idtoken::verify(id_token, &expected, &keys) {
            Ok(identity) => Ok(identity),
            Err(VerifyError::UnknownKid(kid)) => {
                tracing::debug!(kid = %kid, "unknown signing key; refetching JWKS once");
                let keys = self.jwks(&discovery.jwks_uri, true).await?;
                idtoken::verify(id_token, &expected, &keys).map_err(verify_error)
            }
            Err(error) => Err(verify_error(error)),
        }
    }

    async fn jwks(&self, uri: &str, force: bool) -> Result<JwkSet, OidcError> {
        let mut cache = self.jwks.lock().await;
        if !force
            && let Some(cached) = cache.as_ref()
            && cached.fresh(JWKS_TTL)
        {
            return Ok(cached.value.clone());
        }

        if force {
            let mut last = self.forced_jwks_at.lock().await;
            if let Some(at) = *last
                && at.elapsed() < FORCED_JWKS_MIN_INTERVAL
            {
                // Forged kid: do not let it generate unbounded IdP traffic.
                return Err(OidcError::UnknownKid(
                    "JWKS was refetched less than a minute ago".into(),
                ));
            }
            *last = Some(Instant::now());
        }

        match self.get_json::<JwkSet>(uri, "JWKS").await {
            Ok(keys) => {
                *cache = Some(Cached {
                    value: keys.clone(),
                    at: Instant::now(),
                });
                Ok(keys)
            }
            Err(error) => {
                if force {
                    return Err(error);
                }
                match cache.as_ref() {
                    Some(cached) => {
                        tracing::warn!(error = %error, "JWKS fetch failed; using cached keys");
                        Ok(cached.value.clone())
                    }
                    None => Err(error),
                }
            }
        }
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        what: &str,
    ) -> Result<T, OidcError> {
        validate_outbound_url(url, FetchUrlPolicy::OidcEndpoint)
            .map_err(|e| OidcError::Misconfigured(format!("{what}: {e}")))?;
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| OidcError::Network(format!("{what}: {e}")))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| OidcError::Network(format!("{what}: {e}")))?;
        if !status.is_success() {
            return Err(OidcError::Protocol(format!("{what}: HTTP {status}")));
        }
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(OidcError::Protocol(format!(
                "{what}: response is too large"
            )));
        }
        serde_json::from_slice(&bytes)
            .map_err(|e| OidcError::Protocol(format!("{what}: invalid JSON: {e}")))
    }
}

fn verify_error(error: VerifyError) -> OidcError {
    match error {
        VerifyError::UnknownKid(kid) => OidcError::UnknownKid(kid),
        VerifyError::Invalid(reason) => OidcError::InvalidToken(reason),
    }
}

/// Endpoints the browser is sent to: https, or loopback http for a local IdP.
fn validate_browser_endpoint(raw: &str, what: &str) -> Result<(), OidcError> {
    let url = url::Url::parse(raw)
        .map_err(|e| OidcError::Misconfigured(format!("{what} is not a URL: {e}")))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" if common::is_loopback_url(raw) => Ok(()),
        other => Err(OidcError::Misconfigured(format!(
            "{what} must use https (got {other:?})"
        ))),
    }
}

fn form_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
