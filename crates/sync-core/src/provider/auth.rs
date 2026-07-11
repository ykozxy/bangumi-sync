use std::fmt;

use serde::Deserialize;

use crate::model::Provider;

use super::{ProviderReadRequest, ProviderWriteRequest};

const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

const CALLBACK_BOOTSTRAP_MODES: [ProviderAuthBootstrapMode; 2] = [
    ProviderAuthBootstrapMode::AuthBroker,
    ProviderAuthBootstrapMode::LocalCallback,
];

const ANILIST_BOOTSTRAP_MODES: [ProviderAuthBootstrapMode; 3] = [
    ProviderAuthBootstrapMode::AuthBroker,
    ProviderAuthBootstrapMode::LocalCallback,
    ProviderAuthBootstrapMode::ManualPin,
];

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderAccessToken {
    provider: Provider,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderAuthError {
    InvalidBearerToken {
        provider: Provider,
    },
    InvalidOAuthParameter {
        provider: Provider,
        field: &'static str,
    },
    ProviderMismatch {
        request_provider: Provider,
        token_provider: Provider,
    },
    UnsupportedOAuthRequest {
        provider: Provider,
        request: &'static str,
    },
    UnexpectedOAuthStatus {
        provider: Provider,
        status: u16,
    },
    InvalidOAuthResponse {
        provider: Provider,
        field: &'static str,
    },
    CredentialSecretStore {
        provider: Provider,
        reason: &'static str,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderHttpHeader {
    pub name: String,
    pub value: String,
    pub sensitive: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AuthorizedProviderReadRequest {
    pub request: ProviderReadRequest,
    pub headers: Vec<ProviderHttpHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedProviderWriteRequest {
    pub request: ProviderWriteRequest,
    pub headers: Vec<ProviderHttpHeader>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthFlow {
    AuthorizationCode,
    AuthorizationCodePkcePlain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthBootstrapMode {
    AuthBroker,
    LocalCallback,
    ManualPin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRefreshPolicy {
    RefreshToken,
    ReauthorizeOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRefreshTokenState {
    Absent,
    PresentWithUnknownExpiry,
    PresentExpiresAt { expires_at_unix_seconds: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCredentialCapability {
    pub provider: Provider,
    pub auth_flow: ProviderAuthFlow,
    pub access_token_lifetime_seconds: Option<i64>,
    pub refresh_token_lifetime_seconds: Option<i64>,
    pub refresh_policy: ProviderRefreshPolicy,
    pub refresh_before_expiry_seconds: i64,
    pub reauthorize_before_expiry_seconds: i64,
    pub bootstrap_modes: &'static [ProviderAuthBootstrapMode],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialState {
    Missing {
        provider: Provider,
    },
    Available {
        provider: Provider,
        access_token_expires_at_unix_seconds: i64,
        refresh_token: ProviderRefreshTokenState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialAction {
    UseStoredAccessToken,
    RefreshWithProvider,
    Reauthorize {
        preferred_mode: ProviderAuthBootstrapMode,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthAuthorizationInput {
    pub provider: Provider,
    pub client_id: String,
    pub redirect_uri: String,
    pub state: String,
    pub pkce_code_challenge: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthAuthorizationRequest {
    pub provider: Provider,
    pub url: String,
    pub sensitive_url: bool,
    pub endpoint_source: ProviderOAuthEndpointSource,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthTokenExchangeInput {
    pub provider: Provider,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub authorization_code: String,
    pub pkce_code_verifier: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthTokenRefreshInput {
    pub provider: Provider,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub refresh_token: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthFormField {
    pub name: String,
    pub value: String,
    pub sensitive: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthTokenRequest {
    pub provider: Provider,
    pub endpoint: String,
    pub endpoint_source: ProviderOAuthEndpointSource,
    pub content_type: &'static str,
    pub fields: Vec<ProviderOAuthFormField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderOAuthEndpointSource {
    OfficialDocs,
    LegacyRepoCode,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthTokenHttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderOAuthHttpRequestMethod {
    Post,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthHttpRequest {
    pub provider: Provider,
    pub method: ProviderOAuthHttpRequestMethod,
    pub url: String,
    pub headers: Vec<ProviderHttpHeader>,
    pub body: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderOAuthHttpResponse {
    pub status: u16,
    pub body: String,
}

pub trait ProviderOAuthHttpClient {
    fn send_provider_oauth_http_request(
        &mut self,
        request: ProviderOAuthHttpRequest,
    ) -> Result<ProviderOAuthHttpResponse, String>;
}

pub struct ProviderOAuthHttpTokenTransport<C> {
    client: C,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCredentialSecretStoreInput {
    pub provider: Provider,
    pub account_id: String,
    pub token_type: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCredentialSecretLookup {
    pub provider: Provider,
    pub account_id: String,
    pub credential_store_ref: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCredentialRefreshInput {
    pub provider: Provider,
    pub account_id: String,
    pub bootstrap_mode: ProviderAuthBootstrapMode,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub credential_store_ref: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCredentialRefreshPlanInput {
    pub provider: Provider,
    pub account_id: String,
    pub bootstrap_mode: ProviderAuthBootstrapMode,
    pub credential_store_ref: String,
    pub access_token_expires_at_epoch_secs: i64,
    pub refresh_token: ProviderRefreshTokenState,
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCredentialExchangeResult {
    pub provider: Provider,
    pub account_id: String,
    pub auth_flow: ProviderAuthFlow,
    pub bootstrap_mode: ProviderAuthBootstrapMode,
    pub credential_store_ref: String,
    pub access_token_expires_at_epoch_secs: i64,
    pub refresh_token: ProviderRefreshTokenState,
    pub last_refresh_at_epoch_secs: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCredentialRefreshOutcome {
    UseStoredAccessToken,
    Reauthorize {
        preferred_mode: ProviderAuthBootstrapMode,
    },
    Refreshed(ProviderCredentialExchangeResult),
}

pub trait ProviderOAuthTokenTransport {
    fn send_token_request(
        &mut self,
        request: &ProviderOAuthTokenRequest,
    ) -> Result<ProviderOAuthTokenHttpResponse, ProviderAuthError>;
}

impl<C> ProviderOAuthHttpTokenTransport<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub fn into_inner(self) -> C {
        self.client
    }
}

impl<C> ProviderOAuthTokenTransport for ProviderOAuthHttpTokenTransport<C>
where
    C: ProviderOAuthHttpClient,
{
    fn send_token_request(
        &mut self,
        request: &ProviderOAuthTokenRequest,
    ) -> Result<ProviderOAuthTokenHttpResponse, ProviderAuthError> {
        let provider = request.provider;
        let http_request = ProviderOAuthHttpRequest {
            provider,
            method: ProviderOAuthHttpRequestMethod::Post,
            url: request.endpoint.clone(),
            headers: vec![
                ProviderHttpHeader::public("Accept", "application/json"),
                ProviderHttpHeader::public("Content-Type", request.content_type),
            ],
            body: encode_oauth_form(&request.fields),
        };
        let response = self
            .client
            .send_provider_oauth_http_request(http_request)
            .map_err(|_| ProviderAuthError::InvalidOAuthResponse {
                provider,
                field: "transport",
            })?;

        Ok(ProviderOAuthTokenHttpResponse {
            status: response.status,
            body: response.body,
        })
    }
}

pub trait ProviderCredentialAccessTokenStore {
    fn get_provider_access_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<ProviderAccessToken, ProviderAuthError>;
}

pub trait ProviderCredentialSecretStore {
    fn get_provider_refresh_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<String, ProviderAuthError>;

    fn put_provider_tokens(
        &mut self,
        input: ProviderCredentialSecretStoreInput,
    ) -> Result<String, ProviderAuthError>;
}

#[derive(Deserialize)]
struct ProviderOAuthTokenResponseBody {
    token_type: String,
    expires_in: i64,
    access_token: String,
    refresh_token: Option<String>,
}

impl ProviderAccessToken {
    pub fn new(provider: Provider, value: impl Into<String>) -> Result<Self, ProviderAuthError> {
        let value = value.into();
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(ProviderAuthError::InvalidBearerToken { provider });
        }

        Ok(Self { provider, value })
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }
}

impl ProviderCredentialState {
    pub fn missing(provider: Provider) -> Self {
        Self::Missing { provider }
    }

    pub fn available(
        provider: Provider,
        access_token_expires_at_unix_seconds: i64,
        refresh_token: ProviderRefreshTokenState,
    ) -> Self {
        Self::Available {
            provider,
            access_token_expires_at_unix_seconds,
            refresh_token,
        }
    }

    pub fn provider(self) -> Provider {
        match self {
            Self::Missing { provider } | Self::Available { provider, .. } => provider,
        }
    }
}

impl ProviderRefreshTokenState {
    pub fn absent() -> Self {
        Self::Absent
    }

    pub fn present_with_unknown_expiry() -> Self {
        Self::PresentWithUnknownExpiry
    }

    pub fn present_expires_at(expires_at_unix_seconds: i64) -> Self {
        Self::PresentExpiresAt {
            expires_at_unix_seconds,
        }
    }

    pub fn can_refresh_at(self, now_unix_seconds: i64) -> bool {
        match self {
            Self::Absent => false,
            Self::PresentWithUnknownExpiry => true,
            Self::PresentExpiresAt {
                expires_at_unix_seconds,
            } => expires_at_unix_seconds > now_unix_seconds,
        }
    }
}

impl fmt::Debug for ProviderAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderAccessToken")
            .field("provider", &self.provider)
            .field("value", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for ProviderHttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderHttpHeader")
            .field("name", &self.name)
            .field(
                "value",
                if self.sensitive {
                    &"<redacted>"
                } else {
                    &self.value
                },
            )
            .field("sensitive", &self.sensitive)
            .finish()
    }
}

impl fmt::Debug for AuthorizedProviderReadRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthorizedProviderReadRequest")
            .field("request", &self.request)
            .field("headers", &self.headers)
            .finish()
    }
}

impl fmt::Debug for ProviderOAuthAuthorizationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderOAuthAuthorizationRequest")
            .field("provider", &self.provider)
            .field("url", &"<redacted>")
            .field("sensitive_url", &self.sensitive_url)
            .field("endpoint_source", &self.endpoint_source)
            .finish()
    }
}

impl fmt::Debug for ProviderOAuthFormField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redact_value = self.sensitive || is_sensitive_oauth_form_field(&self.name);
        formatter
            .debug_struct("ProviderOAuthFormField")
            .field("name", &self.name)
            .field(
                "value",
                if redact_value {
                    &"<redacted>"
                } else {
                    &self.value
                },
            )
            .field("sensitive", &self.sensitive)
            .finish()
    }
}

impl fmt::Debug for ProviderOAuthTokenRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderOAuthTokenRequest")
            .field("provider", &self.provider)
            .field("endpoint", &self.endpoint)
            .field("endpoint_source", &self.endpoint_source)
            .field("content_type", &self.content_type)
            .field("fields", &self.fields)
            .finish()
    }
}

impl fmt::Debug for ProviderOAuthTokenHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderOAuthTokenHttpResponse")
            .field("status", &self.status)
            .field("body", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for ProviderOAuthHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderOAuthHttpRequest")
            .field("provider", &self.provider)
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("body", &"<present>")
            .finish()
    }
}

impl fmt::Debug for ProviderOAuthHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderOAuthHttpResponse")
            .field("status", &self.status)
            .field("body", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for ProviderCredentialSecretStoreInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredentialSecretStoreInput")
            .field("provider", &self.provider)
            .field("account_id", &self.account_id)
            .field("token_type", &self.token_type)
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl fmt::Debug for ProviderCredentialSecretLookup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredentialSecretLookup")
            .field("provider", &self.provider)
            .field("account_id", &self.account_id)
            .field("credential_store_ref", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for ProviderCredentialRefreshInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredentialRefreshInput")
            .field("provider", &self.provider)
            .field("account_id", &self.account_id)
            .field("bootstrap_mode", &self.bootstrap_mode)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .field("credential_store_ref", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for ProviderCredentialRefreshPlanInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredentialRefreshPlanInput")
            .field("provider", &self.provider)
            .field("account_id", &self.account_id)
            .field("bootstrap_mode", &self.bootstrap_mode)
            .field("credential_store_ref", &"<redacted>")
            .field(
                "access_token_expires_at_epoch_secs",
                &self.access_token_expires_at_epoch_secs,
            )
            .field("refresh_token", &self.refresh_token)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl fmt::Debug for ProviderCredentialExchangeResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredentialExchangeResult")
            .field("provider", &self.provider)
            .field("account_id", &self.account_id)
            .field("auth_flow", &self.auth_flow)
            .field("bootstrap_mode", &self.bootstrap_mode)
            .field("credential_store_ref", &"<redacted>")
            .field(
                "access_token_expires_at_epoch_secs",
                &self.access_token_expires_at_epoch_secs,
            )
            .field("refresh_token", &self.refresh_token)
            .field(
                "last_refresh_at_epoch_secs",
                &self.last_refresh_at_epoch_secs,
            )
            .finish()
    }
}

impl ProviderOAuthTokenRequest {
    pub fn contains_field(&self, name: &str, value: &str) -> bool {
        self.fields
            .iter()
            .any(|field| field.name == name && field.value == value)
    }
}

impl ProviderHttpHeader {
    pub fn public(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            sensitive: false,
        }
    }

    pub fn sensitive(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            sensitive: true,
        }
    }
}

pub fn authorize_provider_read_request(
    request: &ProviderReadRequest,
    token: &ProviderAccessToken,
) -> Result<AuthorizedProviderReadRequest, ProviderAuthError> {
    if request.provider != token.provider {
        return Err(ProviderAuthError::ProviderMismatch {
            request_provider: request.provider,
            token_provider: token.provider,
        });
    }

    let mut headers = vec![
        ProviderHttpHeader::public("Accept", "application/json"),
        ProviderHttpHeader::sensitive("Authorization", format!("Bearer {}", token.value)),
    ];
    if let Some(content_type) = request.content_type {
        headers.push(ProviderHttpHeader::public("Content-Type", content_type));
    }

    Ok(AuthorizedProviderReadRequest {
        request: request.clone(),
        headers,
    })
}

pub fn authorize_provider_write_request(
    request: &ProviderWriteRequest,
    token: &ProviderAccessToken,
) -> Result<AuthorizedProviderWriteRequest, ProviderAuthError> {
    if request.provider != token.provider {
        return Err(ProviderAuthError::ProviderMismatch {
            request_provider: request.provider,
            token_provider: token.provider,
        });
    }

    Ok(AuthorizedProviderWriteRequest {
        request: request.clone(),
        headers: vec![
            ProviderHttpHeader::public("Accept", "application/json"),
            ProviderHttpHeader::sensitive("Authorization", format!("Bearer {}", token.value)),
            ProviderHttpHeader::public("Content-Type", request.content_type),
        ],
    })
}

pub fn provider_credential_capability(provider: Provider) -> ProviderCredentialCapability {
    match provider {
        Provider::Bangumi => ProviderCredentialCapability {
            provider,
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            access_token_lifetime_seconds: Some(7 * SECONDS_PER_DAY),
            refresh_token_lifetime_seconds: None,
            refresh_policy: ProviderRefreshPolicy::RefreshToken,
            refresh_before_expiry_seconds: SECONDS_PER_DAY,
            reauthorize_before_expiry_seconds: SECONDS_PER_DAY,
            bootstrap_modes: &CALLBACK_BOOTSTRAP_MODES,
        },
        Provider::AniList => ProviderCredentialCapability {
            provider,
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            access_token_lifetime_seconds: Some(365 * SECONDS_PER_DAY),
            refresh_token_lifetime_seconds: None,
            refresh_policy: ProviderRefreshPolicy::ReauthorizeOnly,
            refresh_before_expiry_seconds: 0,
            reauthorize_before_expiry_seconds: 30 * SECONDS_PER_DAY,
            bootstrap_modes: &ANILIST_BOOTSTRAP_MODES,
        },
        Provider::MyAnimeList => ProviderCredentialCapability {
            provider,
            auth_flow: ProviderAuthFlow::AuthorizationCodePkcePlain,
            access_token_lifetime_seconds: Some(SECONDS_PER_HOUR),
            refresh_token_lifetime_seconds: Some(30 * SECONDS_PER_DAY),
            refresh_policy: ProviderRefreshPolicy::RefreshToken,
            refresh_before_expiry_seconds: 10 * SECONDS_PER_MINUTE,
            reauthorize_before_expiry_seconds: SECONDS_PER_DAY,
            bootstrap_modes: &CALLBACK_BOOTSTRAP_MODES,
        },
    }
}

pub fn build_provider_authorization_request(
    input: ProviderOAuthAuthorizationInput,
) -> Result<ProviderOAuthAuthorizationRequest, ProviderAuthError> {
    validate_oauth_parameter(input.provider, "client_id", &input.client_id)?;
    validate_oauth_parameter(input.provider, "redirect_uri", &input.redirect_uri)?;
    validate_oauth_parameter(input.provider, "state", &input.state)?;

    let mut query = vec![
        ("response_type", "code".to_owned()),
        ("client_id", input.client_id),
        ("redirect_uri", input.redirect_uri),
        ("state", input.state),
    ];

    if input.provider == Provider::MyAnimeList {
        let Some(code_challenge) = input.pkce_code_challenge else {
            return Err(ProviderAuthError::InvalidOAuthParameter {
                provider: input.provider,
                field: "pkce_code_challenge",
            });
        };
        validate_pkce_plain(input.provider, "pkce_code_challenge", &code_challenge)?;
        query.push(("code_challenge", code_challenge));
        query.push(("code_challenge_method", "plain".to_owned()));
    }

    Ok(ProviderOAuthAuthorizationRequest {
        provider: input.provider,
        url: format!(
            "{}?{}",
            oauth_authorization_endpoint(input.provider),
            encode_query(query)
        ),
        sensitive_url: true,
        endpoint_source: oauth_endpoint_source(input.provider),
    })
}

pub fn build_provider_token_exchange_request(
    input: ProviderOAuthTokenExchangeInput,
) -> Result<ProviderOAuthTokenRequest, ProviderAuthError> {
    validate_oauth_parameter(input.provider, "client_id", &input.client_id)?;
    validate_oauth_parameter(input.provider, "redirect_uri", &input.redirect_uri)?;
    validate_oauth_parameter(
        input.provider,
        "authorization_code",
        &input.authorization_code,
    )?;
    if let Some(client_secret) = input.client_secret.as_deref() {
        validate_oauth_parameter(input.provider, "client_secret", client_secret)?;
    } else if input.provider == Provider::AniList {
        return Err(ProviderAuthError::InvalidOAuthParameter {
            provider: input.provider,
            field: "client_secret",
        });
    }

    let mut fields = vec![
        oauth_field("grant_type", "authorization_code", false),
        oauth_field("client_id", input.client_id, false),
        oauth_field("code", input.authorization_code, true),
        oauth_field("redirect_uri", input.redirect_uri, false),
    ];
    if let Some(client_secret) = input.client_secret {
        fields.push(oauth_field("client_secret", client_secret, true));
    }

    if input.provider == Provider::MyAnimeList {
        let Some(code_verifier) = input.pkce_code_verifier else {
            return Err(ProviderAuthError::InvalidOAuthParameter {
                provider: input.provider,
                field: "pkce_code_verifier",
            });
        };
        validate_pkce_plain(input.provider, "pkce_code_verifier", &code_verifier)?;
        fields.push(oauth_field("code_verifier", code_verifier, true));
    }

    Ok(ProviderOAuthTokenRequest {
        provider: input.provider,
        endpoint: oauth_token_endpoint(input.provider).to_owned(),
        endpoint_source: oauth_endpoint_source(input.provider),
        content_type: "application/x-www-form-urlencoded",
        fields,
    })
}

pub fn build_provider_refresh_token_request(
    input: ProviderOAuthTokenRefreshInput,
) -> Result<ProviderOAuthTokenRequest, ProviderAuthError> {
    if provider_credential_capability(input.provider).refresh_policy
        != ProviderRefreshPolicy::RefreshToken
    {
        return Err(ProviderAuthError::UnsupportedOAuthRequest {
            provider: input.provider,
            request: "refresh_token",
        });
    }

    validate_oauth_parameter(input.provider, "client_id", &input.client_id)?;
    validate_oauth_parameter(input.provider, "refresh_token", &input.refresh_token)?;
    if let Some(client_secret) = input.client_secret.as_deref() {
        validate_oauth_parameter(input.provider, "client_secret", client_secret)?;
    }

    let mut fields = vec![
        oauth_field("grant_type", "refresh_token", false),
        oauth_field("client_id", input.client_id, false),
        oauth_field("refresh_token", input.refresh_token, true),
    ];
    if let Some(client_secret) = input.client_secret {
        fields.push(oauth_field("client_secret", client_secret, true));
    }

    Ok(ProviderOAuthTokenRequest {
        provider: input.provider,
        endpoint: oauth_token_endpoint(input.provider).to_owned(),
        endpoint_source: oauth_endpoint_source(input.provider),
        content_type: "application/x-www-form-urlencoded",
        fields,
    })
}

pub fn exchange_provider_oauth_token<T, S>(
    request: &ProviderOAuthTokenRequest,
    account_id: &str,
    bootstrap_mode: ProviderAuthBootstrapMode,
    now_unix_seconds: i64,
    transport: &mut T,
    secret_store: &mut S,
) -> Result<ProviderCredentialExchangeResult, ProviderAuthError>
where
    T: ProviderOAuthTokenTransport,
    S: ProviderCredentialSecretStore,
{
    exchange_provider_oauth_token_with_refresh_fallback(
        request,
        account_id,
        bootstrap_mode,
        now_unix_seconds,
        None,
        None,
        transport,
        secret_store,
    )
}

pub fn refresh_provider_oauth_token<T, S>(
    input: ProviderCredentialRefreshInput,
    now_unix_seconds: i64,
    transport: &mut T,
    secret_store: &mut S,
) -> Result<ProviderCredentialExchangeResult, ProviderAuthError>
where
    T: ProviderOAuthTokenTransport,
    S: ProviderCredentialSecretStore,
{
    let capability = provider_credential_capability(input.provider);
    if capability.refresh_policy != ProviderRefreshPolicy::RefreshToken {
        return Err(ProviderAuthError::UnsupportedOAuthRequest {
            provider: input.provider,
            request: "refresh_token",
        });
    }
    if !capability.bootstrap_modes.contains(&input.bootstrap_mode) {
        return Err(ProviderAuthError::InvalidOAuthParameter {
            provider: input.provider,
            field: "bootstrap_mode",
        });
    }
    validate_oauth_parameter(input.provider, "account_id", &input.account_id)?;
    validate_oauth_parameter(
        input.provider,
        "credential_store_ref",
        &input.credential_store_ref,
    )?;
    validate_oauth_parameter(input.provider, "client_id", &input.client_id)?;
    if let Some(client_secret) = input.client_secret.as_deref() {
        validate_oauth_parameter(input.provider, "client_secret", client_secret)?;
    }

    let refresh_token =
        secret_store.get_provider_refresh_token(ProviderCredentialSecretLookup {
            provider: input.provider,
            account_id: input.account_id.clone(),
            credential_store_ref: input.credential_store_ref,
        })?;
    let request = build_provider_refresh_token_request(ProviderOAuthTokenRefreshInput {
        provider: input.provider,
        client_id: input.client_id,
        client_secret: input.client_secret,
        refresh_token: refresh_token.clone(),
    })?;

    exchange_provider_oauth_token_with_refresh_fallback(
        &request,
        &input.account_id,
        input.bootstrap_mode,
        now_unix_seconds,
        Some(refresh_token),
        Some(now_unix_seconds),
        transport,
        secret_store,
    )
}

pub fn refresh_provider_credential_if_needed<T, S>(
    input: ProviderCredentialRefreshPlanInput,
    now_unix_seconds: i64,
    transport: &mut T,
    secret_store: &mut S,
) -> Result<ProviderCredentialRefreshOutcome, ProviderAuthError>
where
    T: ProviderOAuthTokenTransport,
    S: ProviderCredentialSecretStore,
{
    validate_oauth_parameter(input.provider, "account_id", &input.account_id)?;
    validate_oauth_parameter(
        input.provider,
        "credential_store_ref",
        &input.credential_store_ref,
    )?;
    validate_oauth_parameter(input.provider, "client_id", &input.client_id)?;
    if let Some(client_secret) = input.client_secret.as_deref() {
        validate_oauth_parameter(input.provider, "client_secret", client_secret)?;
    }
    let capability = provider_credential_capability(input.provider);
    if !capability.bootstrap_modes.contains(&input.bootstrap_mode) {
        return Err(ProviderAuthError::InvalidOAuthParameter {
            provider: input.provider,
            field: "bootstrap_mode",
        });
    }

    let action = plan_provider_credential_action(
        ProviderCredentialState::available(
            input.provider,
            input.access_token_expires_at_epoch_secs,
            input.refresh_token,
        ),
        now_unix_seconds,
    );

    match action {
        ProviderCredentialAction::UseStoredAccessToken => {
            Ok(ProviderCredentialRefreshOutcome::UseStoredAccessToken)
        }
        ProviderCredentialAction::Reauthorize { preferred_mode } => {
            Ok(ProviderCredentialRefreshOutcome::Reauthorize { preferred_mode })
        }
        ProviderCredentialAction::RefreshWithProvider => {
            let credential = refresh_provider_oauth_token(
                ProviderCredentialRefreshInput {
                    provider: input.provider,
                    account_id: input.account_id,
                    bootstrap_mode: input.bootstrap_mode,
                    client_id: input.client_id,
                    client_secret: input.client_secret,
                    credential_store_ref: input.credential_store_ref,
                },
                now_unix_seconds,
                transport,
                secret_store,
            )?;
            Ok(ProviderCredentialRefreshOutcome::Refreshed(credential))
        }
    }
}

fn exchange_provider_oauth_token_with_refresh_fallback<T, S>(
    request: &ProviderOAuthTokenRequest,
    account_id: &str,
    bootstrap_mode: ProviderAuthBootstrapMode,
    now_unix_seconds: i64,
    refresh_token_fallback: Option<String>,
    last_refresh_at_epoch_secs: Option<i64>,
    transport: &mut T,
    secret_store: &mut S,
) -> Result<ProviderCredentialExchangeResult, ProviderAuthError>
where
    T: ProviderOAuthTokenTransport,
    S: ProviderCredentialSecretStore,
{
    validate_oauth_parameter(request.provider, "account_id", account_id)?;
    let capability = provider_credential_capability(request.provider);
    if !capability.bootstrap_modes.contains(&bootstrap_mode) {
        return Err(ProviderAuthError::InvalidOAuthParameter {
            provider: request.provider,
            field: "bootstrap_mode",
        });
    }

    let response = transport.send_token_request(request)?;
    if !(200..300).contains(&response.status) {
        return Err(ProviderAuthError::UnexpectedOAuthStatus {
            provider: request.provider,
            status: response.status,
        });
    }

    let body = parse_oauth_token_response_body(request.provider, &response.body)?;
    let token_type = normalize_bearer_token_type(request.provider, &body.token_type)?;
    ProviderAccessToken::new(request.provider, &body.access_token)?;
    if body.expires_in <= 0 {
        return Err(ProviderAuthError::InvalidOAuthResponse {
            provider: request.provider,
            field: "expires_in",
        });
    }
    let access_token_expires_at_epoch_secs = now_unix_seconds.checked_add(body.expires_in).ok_or(
        ProviderAuthError::InvalidOAuthResponse {
            provider: request.provider,
            field: "expires_in",
        },
    )?;

    let refresh_token_for_store =
        if capability.refresh_policy == ProviderRefreshPolicy::RefreshToken {
            body.refresh_token.clone().or(refresh_token_fallback)
        } else {
            None
        };
    if let Some(refresh_token) = refresh_token_for_store.as_deref() {
        validate_oauth_parameter(request.provider, "refresh_token", refresh_token)?;
    }
    let refresh_token = refresh_token_state_after_exchange(
        &capability,
        refresh_token_for_store.is_some(),
        now_unix_seconds,
    )?;
    let credential_store_ref =
        secret_store.put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: request.provider,
            account_id: account_id.to_owned(),
            token_type,
            access_token: body.access_token,
            refresh_token: refresh_token_for_store.clone(),
        })?;
    validate_oauth_parameter(
        request.provider,
        "credential_store_ref",
        &credential_store_ref,
    )?;

    Ok(ProviderCredentialExchangeResult {
        provider: request.provider,
        account_id: account_id.to_owned(),
        auth_flow: capability.auth_flow,
        bootstrap_mode,
        credential_store_ref,
        access_token_expires_at_epoch_secs,
        refresh_token,
        last_refresh_at_epoch_secs,
    })
}

pub fn plan_provider_credential_action(
    state: ProviderCredentialState,
    now_unix_seconds: i64,
) -> ProviderCredentialAction {
    let capability = provider_credential_capability(state.provider());
    let ProviderCredentialState::Available {
        access_token_expires_at_unix_seconds,
        refresh_token,
        ..
    } = state
    else {
        return reauthorize_with_auth_broker();
    };

    match capability.refresh_policy {
        ProviderRefreshPolicy::RefreshToken => {
            if !refresh_token.can_refresh_at(now_unix_seconds) {
                return reauthorize_with_auth_broker();
            }
            if let ProviderRefreshTokenState::PresentExpiresAt {
                expires_at_unix_seconds,
            } = refresh_token
            {
                if expires_at_unix_seconds - now_unix_seconds
                    <= capability.reauthorize_before_expiry_seconds
                {
                    return reauthorize_with_auth_broker();
                }
            }

            if access_token_expires_at_unix_seconds - now_unix_seconds
                <= capability.refresh_before_expiry_seconds
            {
                return ProviderCredentialAction::RefreshWithProvider;
            }

            ProviderCredentialAction::UseStoredAccessToken
        }
        ProviderRefreshPolicy::ReauthorizeOnly => {
            if access_token_expires_at_unix_seconds - now_unix_seconds
                <= capability.reauthorize_before_expiry_seconds
            {
                return reauthorize_with_auth_broker();
            }

            ProviderCredentialAction::UseStoredAccessToken
        }
    }
}

fn reauthorize_with_auth_broker() -> ProviderCredentialAction {
    ProviderCredentialAction::Reauthorize {
        preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
    }
}

fn parse_oauth_token_response_body(
    provider: Provider,
    body: &str,
) -> Result<ProviderOAuthTokenResponseBody, ProviderAuthError> {
    serde_json::from_str(body).map_err(|_| ProviderAuthError::InvalidOAuthResponse {
        provider,
        field: "body",
    })
}

fn normalize_bearer_token_type(
    provider: Provider,
    token_type: &str,
) -> Result<String, ProviderAuthError> {
    validate_oauth_parameter(provider, "token_type", token_type)?;
    if !token_type.eq_ignore_ascii_case("Bearer") {
        return Err(ProviderAuthError::InvalidOAuthResponse {
            provider,
            field: "token_type",
        });
    }

    Ok("Bearer".to_owned())
}

fn refresh_token_state_after_exchange(
    capability: &ProviderCredentialCapability,
    refresh_token_present: bool,
    now_unix_seconds: i64,
) -> Result<ProviderRefreshTokenState, ProviderAuthError> {
    if !refresh_token_present {
        return Ok(ProviderRefreshTokenState::Absent);
    }

    let refresh_token = match capability.refresh_token_lifetime_seconds {
        Some(lifetime_seconds) => ProviderRefreshTokenState::present_expires_at(
            now_unix_seconds.checked_add(lifetime_seconds).ok_or(
                ProviderAuthError::InvalidOAuthResponse {
                    provider: capability.provider,
                    field: "refresh_token_expires_in",
                },
            )?,
        ),
        None => ProviderRefreshTokenState::present_with_unknown_expiry(),
    };

    Ok(refresh_token)
}

fn oauth_authorization_endpoint(provider: Provider) -> &'static str {
    match provider {
        Provider::Bangumi => "https://bgm.tv/oauth/authorize",
        Provider::AniList => "https://anilist.co/api/v2/oauth/authorize",
        Provider::MyAnimeList => "https://myanimelist.net/v1/oauth2/authorize",
    }
}

fn oauth_token_endpoint(provider: Provider) -> &'static str {
    match provider {
        Provider::Bangumi => "https://bgm.tv/oauth/access_token",
        Provider::AniList => "https://anilist.co/api/v2/oauth/token",
        Provider::MyAnimeList => "https://myanimelist.net/v1/oauth2/token",
    }
}

fn oauth_endpoint_source(provider: Provider) -> ProviderOAuthEndpointSource {
    match provider {
        Provider::Bangumi => ProviderOAuthEndpointSource::LegacyRepoCode,
        Provider::AniList | Provider::MyAnimeList => ProviderOAuthEndpointSource::OfficialDocs,
    }
}

pub(crate) fn validate_oauth_parameter(
    provider: Provider,
    field: &'static str,
    value: &str,
) -> Result<(), ProviderAuthError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(ProviderAuthError::InvalidOAuthParameter { provider, field });
    }

    Ok(())
}

fn validate_pkce_plain(
    provider: Provider,
    field: &'static str,
    value: &str,
) -> Result<(), ProviderAuthError> {
    validate_oauth_parameter(provider, field, value)?;
    if !(43..=128).contains(&value.len()) {
        return Err(ProviderAuthError::InvalidOAuthParameter { provider, field });
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
    {
        return Err(ProviderAuthError::InvalidOAuthParameter { provider, field });
    }

    Ok(())
}

fn oauth_field(
    name: impl Into<String>,
    value: impl Into<String>,
    sensitive: bool,
) -> ProviderOAuthFormField {
    ProviderOAuthFormField {
        name: name.into(),
        value: value.into(),
        sensitive,
    }
}

fn is_sensitive_oauth_form_field(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "access_token"
            | "authorization_code"
            | "client_secret"
            | "code"
            | "code_verifier"
            | "refresh_token"
    )
}

fn encode_oauth_form(fields: &[ProviderOAuthFormField]) -> String {
    fields
        .iter()
        .map(|field| {
            format!(
                "{}={}",
                percent_encode(&field.name),
                percent_encode(&field.value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn encode_query(fields: Vec<(&'static str, String)>) -> String {
    fields
        .into_iter()
        .map(|(name, value)| format!("{}={}", percent_encode(name), percent_encode(&value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }

    output
}
