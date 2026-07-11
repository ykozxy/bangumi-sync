use sync_core::model::{MediaKind, Provider};
use sync_core::provider::{
    authorize_provider_read_request, build_collection_read_request,
    build_provider_authorization_request, build_provider_refresh_token_request,
    build_provider_token_exchange_request, exchange_provider_oauth_token,
    parse_provider_rate_limit, plan_provider_credential_action, provider_credential_capability,
    refresh_provider_oauth_token, ProviderAccessToken, ProviderAuthBootstrapMode,
    ProviderAuthError, ProviderCredentialAction, ProviderCredentialRefreshInput,
    ProviderCredentialRefreshOutcome, ProviderCredentialRefreshPlanInput,
    ProviderCredentialSecretLookup, ProviderCredentialSecretStore,
    ProviderCredentialSecretStoreInput, ProviderCredentialState, ProviderHttpHeader,
    ProviderOAuthAuthorizationInput, ProviderOAuthEndpointSource, ProviderOAuthHttpClient,
    ProviderOAuthHttpRequest, ProviderOAuthHttpRequestMethod, ProviderOAuthHttpResponse,
    ProviderOAuthHttpTokenTransport, ProviderOAuthTokenExchangeInput,
    ProviderOAuthTokenHttpResponse, ProviderOAuthTokenRefreshInput, ProviderOAuthTokenRequest,
    ProviderOAuthTokenTransport, ProviderRefreshPolicy, ProviderRefreshTokenState,
};

#[derive(Default)]
struct FakeTokenTransport {
    requests: Vec<ProviderOAuthTokenRequest>,
    response: Option<ProviderOAuthTokenHttpResponse>,
}

impl FakeTokenTransport {
    fn ok(body: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Some(ProviderOAuthTokenHttpResponse {
                status: 200,
                body: body.to_owned(),
            }),
        }
    }

    fn with_status(status: u16, body: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Some(ProviderOAuthTokenHttpResponse {
                status,
                body: body.to_owned(),
            }),
        }
    }
}

impl ProviderOAuthTokenTransport for FakeTokenTransport {
    fn send_token_request(
        &mut self,
        request: &ProviderOAuthTokenRequest,
    ) -> Result<ProviderOAuthTokenHttpResponse, ProviderAuthError> {
        self.requests.push(request.clone());
        Ok(self.response.take().expect("fake response should be set"))
    }
}

struct FakeSecretStore {
    stored: Vec<ProviderCredentialSecretStoreInput>,
    lookups: Vec<ProviderCredentialSecretLookup>,
    next_ref: String,
    refresh_token: Option<String>,
}

impl FakeSecretStore {
    fn new(next_ref: &str) -> Self {
        Self {
            stored: Vec::new(),
            lookups: Vec::new(),
            next_ref: next_ref.to_owned(),
            refresh_token: None,
        }
    }

    fn with_refresh_token(mut self, refresh_token: &str) -> Self {
        self.refresh_token = Some(refresh_token.to_owned());
        self
    }
}

impl ProviderCredentialSecretStore for FakeSecretStore {
    fn get_provider_refresh_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<String, ProviderAuthError> {
        self.lookups.push(lookup);
        self.refresh_token
            .clone()
            .ok_or(ProviderAuthError::InvalidOAuthParameter {
                provider: Provider::MyAnimeList,
                field: "credential_store_ref",
            })
    }

    fn put_provider_tokens(
        &mut self,
        input: ProviderCredentialSecretStoreInput,
    ) -> Result<String, ProviderAuthError> {
        self.stored.push(input);
        Ok(self.next_ref.clone())
    }
}

#[derive(Default)]
struct RecordingOAuthHttpClient {
    requests: Vec<ProviderOAuthHttpRequest>,
    response: Option<Result<ProviderOAuthHttpResponse, String>>,
}

impl RecordingOAuthHttpClient {
    fn ok(status: u16, body: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Some(Ok(ProviderOAuthHttpResponse {
                status,
                body: body.to_owned(),
            })),
        }
    }

    fn err(message: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Some(Err(message.to_owned())),
        }
    }
}

impl ProviderOAuthHttpClient for RecordingOAuthHttpClient {
    fn send_provider_oauth_http_request(
        &mut self,
        request: ProviderOAuthHttpRequest,
    ) -> Result<ProviderOAuthHttpResponse, String> {
        self.requests.push(request);
        self.response
            .take()
            .expect("fake response should be configured")
    }
}

#[test]
fn provider_access_token_redacts_debug_output() {
    let token = ProviderAccessToken::new(Provider::AniList, "super-secret-token")
        .expect("token should be valid");

    let debug = format!("{token:?}");

    assert!(debug.contains("AniList"));
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("super-secret-token"));
}

#[test]
fn authorize_read_request_adds_sensitive_bearer_header_without_mutating_request() {
    let request =
        build_collection_read_request(Provider::AniList, MediaKind::Anime, "alice", 500, 0)
            .expect("request should build");
    let token =
        ProviderAccessToken::new(Provider::AniList, "fake-token").expect("token should be valid");

    let authorized =
        authorize_provider_read_request(&request, &token).expect("request should authorize");

    assert_eq!(request.provider, Provider::AniList);
    assert!(!request.url.contains("fake-token"));
    assert!(!request
        .body
        .as_deref()
        .unwrap_or_default()
        .contains("fake-token"));
    assert!(authorized
        .headers
        .iter()
        .any(|header| header.name == "Authorization"
            && header.value == "Bearer fake-token"
            && header.sensitive));
    assert!(authorized
        .headers
        .iter()
        .any(|header| header.name == "Content-Type"
            && header.value == "application/json"
            && !header.sensitive));
}

#[test]
fn authorize_read_request_rejects_wrong_provider_token() {
    let request =
        build_collection_read_request(Provider::MyAnimeList, MediaKind::Anime, "@me", 1000, 0)
            .expect("request should build");
    let token =
        ProviderAccessToken::new(Provider::AniList, "fake-token").expect("token should be valid");

    assert_eq!(
        authorize_provider_read_request(&request, &token),
        Err(ProviderAuthError::ProviderMismatch {
            request_provider: Provider::MyAnimeList,
            token_provider: Provider::AniList,
        })
    );
}

#[test]
fn provider_access_token_rejects_empty_and_control_character_values() {
    assert_eq!(
        ProviderAccessToken::new(Provider::Bangumi, " "),
        Err(ProviderAuthError::InvalidBearerToken {
            provider: Provider::Bangumi,
        })
    );
    assert_eq!(
        ProviderAccessToken::new(Provider::Bangumi, "bad\nvalue"),
        Err(ProviderAuthError::InvalidBearerToken {
            provider: Provider::Bangumi,
        })
    );
}

#[test]
fn oauth_http_token_transport_sends_form_encoded_post_without_debug_secret_leaks() {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: Provider::MyAnimeList,
        client_id: "mal-client".to_owned(),
        client_secret: Some("mal-client-secret".to_owned()),
        redirect_uri: "http://localhost:3500/callback".to_owned(),
        authorization_code: "authorization-code-secret".to_owned(),
        pkce_code_verifier: Some("plain-code-verifier-123456789012345678901234567890".to_owned()),
    })
    .expect("MAL token exchange request should build");
    let client = RecordingOAuthHttpClient::ok(
        200,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-access-token-secret",
            "refresh_token": "new-refresh-token-secret"
        }"#,
    );
    let mut transport = ProviderOAuthHttpTokenTransport::new(client);

    let response = transport
        .send_token_request(&request)
        .expect("HTTP token transport should return fixture response");

    assert_eq!(response.status, 200);
    assert!(!format!("{response:?}").contains("new-access-token-secret"));
    let client = transport.into_inner();
    assert_eq!(client.requests.len(), 1);
    let http_request = &client.requests[0];
    assert_eq!(http_request.provider, Provider::MyAnimeList);
    assert_eq!(http_request.method, ProviderOAuthHttpRequestMethod::Post);
    assert_eq!(http_request.url, "https://myanimelist.net/v1/oauth2/token");
    assert!(http_request.headers.iter().any(|header| {
        header.name == "Accept" && header.value == "application/json" && !header.sensitive
    }));
    assert!(http_request.headers.iter().any(|header| {
        header.name == "Content-Type"
            && header.value == "application/x-www-form-urlencoded"
            && !header.sensitive
    }));
    assert!(http_request.body.contains("grant_type=authorization_code"));
    assert!(http_request.body.contains("client_id=mal-client"));
    assert!(http_request
        .body
        .contains("client_secret=mal-client-secret"));
    assert!(http_request.body.contains("code=authorization-code-secret"));
    assert!(http_request
        .body
        .contains("redirect_uri=http%3A%2F%2Flocalhost%3A3500%2Fcallback"));
    assert!(http_request
        .body
        .contains("code_verifier=plain-code-verifier-123456789012345678901234567890"));

    let debug = format!("{http_request:?}");
    assert!(debug.contains("body"));
    assert!(!debug.contains("authorization-code-secret"));
    assert!(!debug.contains("plain-code-verifier"));
    assert!(!debug.contains("mal-client-secret"));
}

#[test]
fn oauth_http_token_transport_sanitizes_client_errors() {
    let request = build_provider_refresh_token_request(ProviderOAuthTokenRefreshInput {
        provider: Provider::Bangumi,
        client_id: "bangumi-client".to_owned(),
        client_secret: Some("bangumi-secret".to_owned()),
        refresh_token: "refresh-token-secret".to_owned(),
    })
    .expect("Bangumi refresh request should build");
    let client = RecordingOAuthHttpClient::err("socket failed refresh-token-secret bangumi-secret");
    let mut transport = ProviderOAuthHttpTokenTransport::new(client);

    let error = transport
        .send_token_request(&request)
        .expect_err("client transport failure should be sanitized");

    assert_eq!(
        error,
        ProviderAuthError::InvalidOAuthResponse {
            provider: Provider::Bangumi,
            field: "transport",
        }
    );
    assert!(!format!("{error:?}").contains("refresh-token-secret"));
    assert!(!format!("{error:?}").contains("bangumi-secret"));
    let client = transport.into_inner();
    assert_eq!(client.requests.len(), 1);
    assert!(client.requests[0]
        .body
        .contains("refresh_token=refresh-token-secret"));
}

#[test]
fn oauth_token_request_debug_redacts_sensitive_field_names_even_when_marked_public() {
    let request = ProviderOAuthTokenRequest {
        provider: Provider::Bangumi,
        endpoint: "https://bgm.tv/oauth/access_token".to_owned(),
        endpoint_source: ProviderOAuthEndpointSource::LegacyRepoCode,
        content_type: "application/x-www-form-urlencoded",
        fields: vec![
            sync_core::provider::ProviderOAuthFormField {
                name: "client_secret".to_owned(),
                value: "public-client-secret-leak".to_owned(),
                sensitive: false,
            },
            sync_core::provider::ProviderOAuthFormField {
                name: "code".to_owned(),
                value: "public-code-leak".to_owned(),
                sensitive: false,
            },
            sync_core::provider::ProviderOAuthFormField {
                name: "refresh_token".to_owned(),
                value: "public-refresh-token-leak".to_owned(),
                sensitive: false,
            },
            sync_core::provider::ProviderOAuthFormField {
                name: "code_verifier".to_owned(),
                value: "public-code-verifier-leak".to_owned(),
                sensitive: false,
            },
            sync_core::provider::ProviderOAuthFormField {
                name: "access_token".to_owned(),
                value: "public-access-token-leak".to_owned(),
                sensitive: false,
            },
        ],
    };

    let debug = format!("{request:?}");

    assert!(!debug.contains("public-client-secret-leak"));
    assert!(!debug.contains("public-code-leak"));
    assert!(!debug.contains("public-refresh-token-leak"));
    assert!(!debug.contains("public-code-verifier-leak"));
    assert!(!debug.contains("public-access-token-leak"));
}

#[test]
fn anilist_authorization_code_request_matches_official_shape_without_secret_values() {
    let request = build_provider_authorization_request(ProviderOAuthAuthorizationInput {
        provider: Provider::AniList,
        client_id: "client-123".to_owned(),
        redirect_uri: "http://localhost:3499/callback".to_owned(),
        state: "state-value".to_owned(),
        pkce_code_challenge: None,
    })
    .expect("AniList auth URL should build");

    assert_eq!(request.provider, Provider::AniList);
    assert!(request
        .url
        .starts_with("https://anilist.co/api/v2/oauth/authorize?"));
    assert!(request.url.contains("response_type=code"));
    assert!(request.url.contains("client_id=client-123"));
    assert!(request
        .url
        .contains("redirect_uri=http%3A%2F%2Flocalhost%3A3499%2Fcallback"));
    assert!(request.url.contains("state=state-value"));
    assert!(!request.url.contains("client_secret"));
    assert!(!format!("{request:?}").contains("state-value"));
}

#[test]
fn myanimelist_authorization_request_uses_plain_pkce_challenge() {
    let request = build_provider_authorization_request(ProviderOAuthAuthorizationInput {
        provider: Provider::MyAnimeList,
        client_id: "mal-client".to_owned(),
        redirect_uri: "http://localhost:3500/callback".to_owned(),
        state: "csrf-state".to_owned(),
        pkce_code_challenge: Some("plain-code-verifier-123456789012345678901234567890".to_owned()),
    })
    .expect("MAL auth URL should build");

    assert_eq!(request.provider, Provider::MyAnimeList);
    assert!(request
        .url
        .starts_with("https://myanimelist.net/v1/oauth2/authorize?"));
    assert!(request.url.contains("response_type=code"));
    assert!(request.url.contains("client_id=mal-client"));
    assert!(request.url.contains("code_challenge_method=plain"));
    assert!(request
        .url
        .contains("code_challenge=plain-code-verifier-123456789012345678901234567890"));
    assert!(!format!("{request:?}").contains("plain-code-verifier"));
}

#[test]
fn token_exchange_requests_redact_codes_and_verifiers() {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: Provider::MyAnimeList,
        client_id: "mal-client".to_owned(),
        client_secret: Some("mal-secret".to_owned()),
        redirect_uri: "http://localhost:3500/callback".to_owned(),
        authorization_code: "auth-code-secret".to_owned(),
        pkce_code_verifier: Some("plain-code-verifier-123456789012345678901234567890".to_owned()),
    })
    .expect("MAL token exchange request should build");

    assert_eq!(request.provider, Provider::MyAnimeList);
    assert_eq!(request.endpoint, "https://myanimelist.net/v1/oauth2/token");
    assert!(request.contains_field("grant_type", "authorization_code"));
    assert!(request.contains_field("client_id", "mal-client"));
    assert!(request.contains_field("client_secret", "mal-secret"));
    assert!(request.contains_field("code", "auth-code-secret"));
    assert!(request.contains_field(
        "code_verifier",
        "plain-code-verifier-123456789012345678901234567890"
    ));
    let debug = format!("{request:?}");
    assert!(!debug.contains("mal-secret"));
    assert!(!debug.contains("auth-code-secret"));
    assert!(!debug.contains("plain-code-verifier"));
}

#[test]
fn myanimelist_pkce_plain_rejects_non_unreserved_characters() {
    let invalid_verifier = "plain-code-verifier-1234567890123456789012!";
    assert_eq!(invalid_verifier.len(), 43);

    assert_eq!(
        build_provider_authorization_request(ProviderOAuthAuthorizationInput {
            provider: Provider::MyAnimeList,
            client_id: "mal-client".to_owned(),
            redirect_uri: "http://localhost:3500/callback".to_owned(),
            state: "csrf-state".to_owned(),
            pkce_code_challenge: Some(invalid_verifier.to_owned()),
        }),
        Err(ProviderAuthError::InvalidOAuthParameter {
            provider: Provider::MyAnimeList,
            field: "pkce_code_challenge",
        })
    );
    assert_eq!(
        build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
            provider: Provider::MyAnimeList,
            client_id: "mal-client".to_owned(),
            client_secret: Some("mal-secret".to_owned()),
            redirect_uri: "http://localhost:3500/callback".to_owned(),
            authorization_code: "auth-code-secret".to_owned(),
            pkce_code_verifier: Some(invalid_verifier.to_owned()),
        }),
        Err(ProviderAuthError::InvalidOAuthParameter {
            provider: Provider::MyAnimeList,
            field: "pkce_code_verifier",
        })
    );
}

#[test]
fn anilist_token_exchange_requires_client_secret() {
    assert_eq!(
        build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
            provider: Provider::AniList,
            client_id: "anilist-client".to_owned(),
            client_secret: None,
            redirect_uri: "http://localhost:3499/callback".to_owned(),
            authorization_code: "auth-code-secret".to_owned(),
            pkce_code_verifier: None,
        }),
        Err(ProviderAuthError::InvalidOAuthParameter {
            provider: Provider::AniList,
            field: "client_secret",
        })
    );
}

#[test]
fn bangumi_oauth_request_shapes_are_marked_legacy_repo_derived() {
    let authorization_request =
        build_provider_authorization_request(ProviderOAuthAuthorizationInput {
            provider: Provider::Bangumi,
            client_id: "bangumi-client".to_owned(),
            redirect_uri: "http://localhost:3498/callback".to_owned(),
            state: "csrf-state".to_owned(),
            pkce_code_challenge: None,
        })
        .expect("legacy-derived Bangumi authorization URL should build");

    assert_eq!(
        authorization_request.endpoint_source,
        ProviderOAuthEndpointSource::LegacyRepoCode
    );
    assert!(authorization_request
        .url
        .starts_with("https://bgm.tv/oauth/authorize?"));

    let refresh_request = build_provider_refresh_token_request(ProviderOAuthTokenRefreshInput {
        provider: Provider::Bangumi,
        client_id: "bangumi-client".to_owned(),
        client_secret: Some("bangumi-secret".to_owned()),
        refresh_token: "refresh-token-secret".to_owned(),
    })
    .expect("legacy-derived Bangumi refresh request should build");
    assert_eq!(
        refresh_request.endpoint_source,
        ProviderOAuthEndpointSource::LegacyRepoCode
    );
    assert_eq!(
        refresh_request.endpoint,
        "https://bgm.tv/oauth/access_token"
    );
    assert!(!format!("{refresh_request:?}").contains("bangumi-secret"));
    assert!(!format!("{refresh_request:?}").contains("refresh-token-secret"));
}

#[test]
fn refresh_token_requests_follow_provider_refresh_policy() {
    let mal_request = build_provider_refresh_token_request(ProviderOAuthTokenRefreshInput {
        provider: Provider::MyAnimeList,
        client_id: "mal-client".to_owned(),
        client_secret: Some("mal-secret".to_owned()),
        refresh_token: "refresh-token-secret".to_owned(),
    })
    .expect("MAL refresh request should build");

    assert_eq!(mal_request.provider, Provider::MyAnimeList);
    assert_eq!(
        mal_request.endpoint,
        "https://myanimelist.net/v1/oauth2/token"
    );
    assert!(mal_request.contains_field("grant_type", "refresh_token"));
    assert!(mal_request.contains_field("refresh_token", "refresh-token-secret"));
    assert!(!format!("{mal_request:?}").contains("refresh-token-secret"));

    assert_eq!(
        build_provider_refresh_token_request(ProviderOAuthTokenRefreshInput {
            provider: Provider::AniList,
            client_id: "anilist-client".to_owned(),
            client_secret: Some("anilist-secret".to_owned()),
            refresh_token: "refresh-token-secret".to_owned(),
        }),
        Err(ProviderAuthError::UnsupportedOAuthRequest {
            provider: Provider::AniList,
            request: "refresh_token",
        })
    );
}

#[test]
fn token_exchange_uses_injected_transport_and_secret_store_without_returning_tokens() {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: Provider::MyAnimeList,
        client_id: "mal-client".to_owned(),
        client_secret: Some("mal-secret".to_owned()),
        redirect_uri: "http://localhost:3500/callback".to_owned(),
        authorization_code: "auth-code-secret".to_owned(),
        pkce_code_verifier: Some("plain-code-verifier-123456789012345678901234567890".to_owned()),
    })
    .expect("MAL token exchange request should build");
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "access-token-secret",
            "refresh_token": "refresh-token-secret"
        }"#,
    );
    let mut secret_store = FakeSecretStore::new("secret-store:mal-user-1");

    let credential = exchange_provider_oauth_token(
        &request,
        "mal-user-1",
        ProviderAuthBootstrapMode::AuthBroker,
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("token exchange should normalize and store secret material");

    assert_eq!(transport.requests.len(), 1);
    assert_eq!(transport.requests[0].endpoint, request.endpoint);
    assert_eq!(secret_store.stored.len(), 1);
    assert_eq!(secret_store.stored[0].provider, Provider::MyAnimeList);
    assert_eq!(secret_store.stored[0].account_id, "mal-user-1");
    assert_eq!(secret_store.stored[0].token_type, "Bearer");
    assert_eq!(secret_store.stored[0].access_token, "access-token-secret");
    assert_eq!(
        secret_store.stored[0].refresh_token.as_deref(),
        Some("refresh-token-secret")
    );
    assert_eq!(credential.provider, Provider::MyAnimeList);
    assert_eq!(credential.account_id, "mal-user-1");
    assert_eq!(
        credential.bootstrap_mode,
        ProviderAuthBootstrapMode::AuthBroker
    );
    assert_eq!(credential.credential_store_ref, "secret-store:mal-user-1");
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_003_600);
    assert_eq!(
        credential.refresh_token,
        ProviderRefreshTokenState::present_expires_at(1_702_592_000)
    );
    let credential_debug = format!("{credential:?}");
    assert!(!credential_debug.contains("access-token-secret"));
    assert!(!credential_debug.contains("refresh-token-secret"));
    let stored_debug = format!("{:?}", secret_store.stored[0]);
    assert!(!stored_debug.contains("access-token-secret"));
    assert!(!stored_debug.contains("refresh-token-secret"));
}

#[test]
fn token_exchange_failures_do_not_write_secret_store() {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: Provider::MyAnimeList,
        client_id: "mal-client".to_owned(),
        client_secret: Some("mal-secret".to_owned()),
        redirect_uri: "http://localhost:3500/callback".to_owned(),
        authorization_code: "auth-code-secret".to_owned(),
        pkce_code_verifier: Some("plain-code-verifier-123456789012345678901234567890".to_owned()),
    })
    .expect("MAL token exchange request should build");
    let mut transport = FakeTokenTransport::with_status(
        401,
        r#"{"error":"invalid_grant","access_token":"must-not-store"}"#,
    );
    let mut secret_store = FakeSecretStore::new("secret-store:mal-user-1");

    let error = exchange_provider_oauth_token(
        &request,
        "mal-user-1",
        ProviderAuthBootstrapMode::AuthBroker,
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("non-success token response should fail");

    assert_eq!(
        error,
        ProviderAuthError::UnexpectedOAuthStatus {
            provider: Provider::MyAnimeList,
            status: 401,
        }
    );
    assert!(secret_store.stored.is_empty());
    assert!(!format!("{error:?}").contains("must-not-store"));
}

#[test]
fn token_exchange_drops_refresh_token_for_reauthorize_only_provider() {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: Provider::AniList,
        client_id: "anilist-client".to_owned(),
        client_secret: Some("anilist-secret".to_owned()),
        redirect_uri: "http://localhost:3499/callback".to_owned(),
        authorization_code: "auth-code-secret".to_owned(),
        pkce_code_verifier: None,
    })
    .expect("AniList token exchange request should build");
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 31536000,
            "access_token": "anilist-access-token-secret",
            "refresh_token": "unexpected-refresh-token-secret"
        }"#,
    );
    let mut secret_store = FakeSecretStore::new("secret-store:anilist-user-1");

    let credential = exchange_provider_oauth_token(
        &request,
        "anilist-user-1",
        ProviderAuthBootstrapMode::AuthBroker,
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("AniList token exchange should normalize metadata");

    assert_eq!(secret_store.stored.len(), 1);
    assert_eq!(secret_store.stored[0].refresh_token, None);
    assert_eq!(credential.refresh_token, ProviderRefreshTokenState::Absent);
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_731_536_000);
}

#[test]
fn token_exchange_drops_malformed_refresh_token_for_reauthorize_only_provider() {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: Provider::AniList,
        client_id: "anilist-client".to_owned(),
        client_secret: Some("anilist-secret".to_owned()),
        redirect_uri: "http://localhost:3499/callback".to_owned(),
        authorization_code: "auth-code-secret".to_owned(),
        pkce_code_verifier: None,
    })
    .expect("AniList token exchange request should build");
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 31536000,
            "access_token": "anilist-access-token-secret",
            "refresh_token": "bad\nrefresh-token"
        }"#,
    );
    let mut secret_store = FakeSecretStore::new("secret-store:anilist-user-1");

    let credential = exchange_provider_oauth_token(
        &request,
        "anilist-user-1",
        ProviderAuthBootstrapMode::AuthBroker,
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("AniList token exchange should ignore refresh-token material");

    assert_eq!(secret_store.stored[0].refresh_token, None);
    assert_eq!(credential.refresh_token, ProviderRefreshTokenState::Absent);
}

#[test]
fn token_exchange_rejects_expiry_overflow_without_secret_store_write() {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: Provider::MyAnimeList,
        client_id: "mal-client".to_owned(),
        client_secret: Some("mal-secret".to_owned()),
        redirect_uri: "http://localhost:3500/callback".to_owned(),
        authorization_code: "auth-code-secret".to_owned(),
        pkce_code_verifier: Some("plain-code-verifier-123456789012345678901234567890".to_owned()),
    })
    .expect("MAL token exchange request should build");
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "access-token-secret",
            "refresh_token": "refresh-token-secret"
        }"#,
    );
    let mut secret_store = FakeSecretStore::new("secret-store:mal-user-1");

    let error = exchange_provider_oauth_token(
        &request,
        "mal-user-1",
        ProviderAuthBootstrapMode::AuthBroker,
        i64::MAX - 10,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("overflowing expiry should fail");

    assert_eq!(
        error,
        ProviderAuthError::InvalidOAuthResponse {
            provider: Provider::MyAnimeList,
            field: "expires_in",
        }
    );
    assert!(secret_store.stored.is_empty());
}

#[test]
fn token_exchange_rejects_non_bearer_token_type() {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: Provider::MyAnimeList,
        client_id: "mal-client".to_owned(),
        client_secret: Some("mal-secret".to_owned()),
        redirect_uri: "http://localhost:3500/callback".to_owned(),
        authorization_code: "auth-code-secret".to_owned(),
        pkce_code_verifier: Some("plain-code-verifier-123456789012345678901234567890".to_owned()),
    })
    .expect("MAL token exchange request should build");
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "MAC",
            "expires_in": 3600,
            "access_token": "access-token-secret",
            "refresh_token": "refresh-token-secret"
        }"#,
    );
    let mut secret_store = FakeSecretStore::new("secret-store:mal-user-1");

    let error = exchange_provider_oauth_token(
        &request,
        "mal-user-1",
        ProviderAuthBootstrapMode::AuthBroker,
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("non-bearer token type should fail");

    assert_eq!(
        error,
        ProviderAuthError::InvalidOAuthResponse {
            provider: Provider::MyAnimeList,
            field: "token_type",
        }
    );
    assert!(secret_store.stored.is_empty());
}

#[test]
fn refresh_provider_oauth_token_reads_secret_and_retains_refresh_token_when_not_rotated() {
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-access-token-secret"
        }"#,
    );
    let mut secret_store =
        FakeSecretStore::new("secret-store:mal-user-1:v2").with_refresh_token("old-refresh-token");

    let credential = refresh_provider_oauth_token(
        ProviderCredentialRefreshInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            client_id: "mal-client".to_owned(),
            client_secret: Some("mal-secret".to_owned()),
            credential_store_ref: "secret-store:mal-user-1:v1".to_owned(),
        },
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("refresh should read stored refresh token and store new access token");

    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(secret_store.lookups[0].provider, Provider::MyAnimeList);
    assert_eq!(secret_store.lookups[0].account_id, "mal-user-1");
    assert_eq!(
        secret_store.lookups[0].credential_store_ref,
        "secret-store:mal-user-1:v1"
    );
    assert_eq!(transport.requests.len(), 1);
    assert_eq!(
        transport.requests[0].endpoint,
        "https://myanimelist.net/v1/oauth2/token"
    );
    assert!(transport.requests[0].contains_field("grant_type", "refresh_token"));
    assert!(transport.requests[0].contains_field("client_id", "mal-client"));
    assert!(transport.requests[0].contains_field("client_secret", "mal-secret"));
    assert!(transport.requests[0].contains_field("refresh_token", "old-refresh-token"));
    assert!(!format!("{:?}", transport.requests[0]).contains("old-refresh-token"));

    assert_eq!(secret_store.stored.len(), 1);
    assert_eq!(secret_store.stored[0].provider, Provider::MyAnimeList);
    assert_eq!(secret_store.stored[0].account_id, "mal-user-1");
    assert_eq!(secret_store.stored[0].token_type, "Bearer");
    assert_eq!(
        secret_store.stored[0].access_token,
        "new-access-token-secret"
    );
    assert_eq!(
        secret_store.stored[0].refresh_token.as_deref(),
        Some("old-refresh-token")
    );
    assert_eq!(credential.provider, Provider::MyAnimeList);
    assert_eq!(credential.account_id, "mal-user-1");
    assert_eq!(
        credential.bootstrap_mode,
        ProviderAuthBootstrapMode::AuthBroker
    );
    assert_eq!(
        credential.credential_store_ref,
        "secret-store:mal-user-1:v2"
    );
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_003_600);
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));
    assert_eq!(
        credential.refresh_token,
        ProviderRefreshTokenState::present_expires_at(1_702_592_000)
    );
    let credential_debug = format!("{credential:?}");
    assert!(!credential_debug.contains("new-access-token-secret"));
    assert!(!credential_debug.contains("old-refresh-token"));
}

#[test]
fn refresh_provider_oauth_token_rejects_reauthorize_only_provider_before_secret_lookup() {
    let mut transport = FakeTokenTransport::ok("{}");
    let mut secret_store =
        FakeSecretStore::new("secret-store:anilist-user-1").with_refresh_token("refresh-token");

    let error = refresh_provider_oauth_token(
        ProviderCredentialRefreshInput {
            provider: Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            client_id: "anilist-client".to_owned(),
            client_secret: Some("anilist-secret".to_owned()),
            credential_store_ref: "secret-store:anilist-user-1".to_owned(),
        },
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("AniList refresh must remain disabled");

    assert_eq!(
        error,
        ProviderAuthError::UnsupportedOAuthRequest {
            provider: Provider::AniList,
            request: "refresh_token",
        }
    );
    assert!(secret_store.lookups.is_empty());
    assert!(secret_store.stored.is_empty());
    assert!(transport.requests.is_empty());
}

#[test]
fn refresh_provider_oauth_token_rejects_invalid_client_parameters_before_secret_lookup() {
    for (label, client_id, client_secret, expected_field) in [
        ("empty client id", "", Some("bangumi-secret"), "client_id"),
        (
            "control client id",
            "bad\nclient",
            Some("bangumi-secret"),
            "client_id",
        ),
        (
            "empty client secret",
            "bangumi-client",
            Some(" "),
            "client_secret",
        ),
        (
            "control client secret",
            "bangumi-client",
            Some("bad\nsecret"),
            "client_secret",
        ),
    ] {
        let mut transport = FakeTokenTransport::ok("{}");
        let mut secret_store = FakeSecretStore::new("secret-store:bangumi-user-1:v2")
            .with_refresh_token("old-refresh-token");

        let error = refresh_provider_oauth_token(
            ProviderCredentialRefreshInput {
                provider: Provider::Bangumi,
                account_id: "bangumi-user-1".to_owned(),
                bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
                client_id: client_id.to_owned(),
                client_secret: client_secret.map(str::to_owned),
                credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            },
            1_700_000_000,
            &mut transport,
            &mut secret_store,
        )
        .expect_err("invalid client parameters should fail before secret lookup");

        assert_eq!(
            error,
            ProviderAuthError::InvalidOAuthParameter {
                provider: Provider::Bangumi,
                field: expected_field,
            },
            "{label}"
        );
        assert!(secret_store.lookups.is_empty(), "{label}");
        assert!(secret_store.stored.is_empty(), "{label}");
        assert!(transport.requests.is_empty(), "{label}");
    }
}

#[test]
fn refresh_provider_oauth_token_rejects_invalid_stored_refresh_token_before_transport() {
    for refresh_token in [" ", "bad\nrefresh-token"] {
        let mut transport = FakeTokenTransport::ok(
            r#"{
                "token_type": "Bearer",
                "expires_in": 3600,
                "access_token": "must-not-store"
            }"#,
        );
        let mut secret_store =
            FakeSecretStore::new("secret-store:mal-user-1:v2").with_refresh_token(refresh_token);

        let error = refresh_provider_oauth_token(
            ProviderCredentialRefreshInput {
                provider: Provider::MyAnimeList,
                account_id: "mal-user-1".to_owned(),
                bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
                client_id: "mal-client".to_owned(),
                client_secret: Some("mal-secret".to_owned()),
                credential_store_ref: "secret-store:mal-user-1:v1".to_owned(),
            },
            1_700_000_000,
            &mut transport,
            &mut secret_store,
        )
        .expect_err("invalid stored refresh token should fail before transport");

        assert_eq!(
            error,
            ProviderAuthError::InvalidOAuthParameter {
                provider: Provider::MyAnimeList,
                field: "refresh_token",
            }
        );
        assert_eq!(secret_store.lookups.len(), 1);
        assert!(transport.requests.is_empty());
        assert!(secret_store.stored.is_empty());
        assert!(!format!("{error:?}").contains("must-not-store"));
    }
}

#[test]
fn refresh_provider_credential_if_needed_refreshes_only_when_preflight_requires_refresh() {
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-access-token-secret",
            "refresh_token": "rotated-refresh-token-secret"
        }"#,
    );
    let mut secret_store = FakeSecretStore::new("secret-store:bangumi-user-1:v2")
        .with_refresh_token("old-refresh-token");

    let outcome = sync_core::provider::refresh_provider_credential_if_needed(
        ProviderCredentialRefreshPlanInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            client_id: "bangumi-client".to_owned(),
            client_secret: Some("bangumi-secret".to_owned()),
        },
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("near-expiry Bangumi credential should refresh");

    let ProviderCredentialRefreshOutcome::Refreshed(credential) = outcome else {
        panic!("expected refreshed outcome");
    };
    assert_eq!(credential.provider, Provider::Bangumi);
    assert_eq!(credential.account_id, "bangumi-user-1");
    assert_eq!(
        credential.credential_store_ref,
        "secret-store:bangumi-user-1:v2"
    );
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_003_600);
    assert_eq!(
        credential.refresh_token,
        ProviderRefreshTokenState::present_with_unknown_expiry()
    );
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(transport.requests.len(), 1);
    assert_eq!(secret_store.stored.len(), 1);
    assert_eq!(
        secret_store.stored[0].refresh_token.as_deref(),
        Some("rotated-refresh-token-secret")
    );
}

#[test]
fn refresh_provider_credential_if_needed_uses_stored_token_without_secret_lookup_when_valid() {
    let mut transport = FakeTokenTransport::ok("{}");
    let mut secret_store = FakeSecretStore::new("secret-store:bangumi-user-1:v2")
        .with_refresh_token("old-refresh-token");

    let outcome = sync_core::provider::refresh_provider_credential_if_needed(
        ProviderCredentialRefreshPlanInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            client_id: "bangumi-client".to_owned(),
            client_secret: Some("bangumi-secret".to_owned()),
        },
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("valid credential should not refresh");

    assert_eq!(
        outcome,
        ProviderCredentialRefreshOutcome::UseStoredAccessToken
    );
    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
    assert!(secret_store.stored.is_empty());
}

#[test]
fn refresh_provider_credential_if_needed_validates_metadata_before_noop_outcomes() {
    for (label, account_id, credential_store_ref, client_id, expected_field) in [
        (
            "empty account",
            "",
            "secret-store:bangumi-user-1:v1",
            "bangumi-client",
            "account_id",
        ),
        (
            "empty ref",
            "bangumi-user-1",
            "",
            "bangumi-client",
            "credential_store_ref",
        ),
        (
            "empty client",
            "bangumi-user-1",
            "secret-store:bangumi-user-1:v1",
            "",
            "client_id",
        ),
        (
            "control ref",
            "bangumi-user-1",
            "bad\nref",
            "bangumi-client",
            "credential_store_ref",
        ),
    ] {
        let mut transport = FakeTokenTransport::ok("{}");
        let mut secret_store = FakeSecretStore::new("secret-store:bangumi-user-1:v2")
            .with_refresh_token("old-refresh-token");

        let result = sync_core::provider::refresh_provider_credential_if_needed(
            ProviderCredentialRefreshPlanInput {
                provider: Provider::Bangumi,
                account_id: account_id.to_owned(),
                bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
                credential_store_ref: credential_store_ref.to_owned(),
                access_token_expires_at_epoch_secs: 1_700_259_200,
                refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
                client_id: client_id.to_owned(),
                client_secret: Some("bangumi-secret".to_owned()),
            },
            1_700_000_000,
            &mut transport,
            &mut secret_store,
        );
        let error = match result {
            Ok(outcome) => panic!("{label} should fail before noop, got {outcome:?}"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ProviderAuthError::InvalidOAuthParameter {
                provider: Provider::Bangumi,
                field: expected_field,
            },
            "{label}"
        );
        assert!(secret_store.lookups.is_empty(), "{label}");
        assert!(transport.requests.is_empty(), "{label}");
        assert!(secret_store.stored.is_empty(), "{label}");
    }
}

#[test]
fn refresh_provider_credential_if_needed_reauthorizes_without_secret_lookup_when_not_refreshable() {
    let mut transport = FakeTokenTransport::ok("{}");
    let mut secret_store =
        FakeSecretStore::new("secret-store:mal-user-1:v2").with_refresh_token("old-refresh-token");

    let outcome = sync_core::provider::refresh_provider_credential_if_needed(
        ProviderCredentialRefreshPlanInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:mal-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::absent(),
            client_id: "mal-client".to_owned(),
            client_secret: Some("mal-secret".to_owned()),
        },
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("absent refresh token should route to reauth");

    assert_eq!(
        outcome,
        ProviderCredentialRefreshOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker
        }
    );
    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
    assert!(secret_store.stored.is_empty());
}

#[test]
fn refresh_provider_credential_if_needed_reauthorizes_before_refresh_token_expiry_without_secret_lookup(
) {
    let now = 1_700_000_000;
    let mut transport = FakeTokenTransport::ok("{}");
    let mut secret_store =
        FakeSecretStore::new("secret-store:mal-user-1:v2").with_refresh_token("old-refresh-token");

    let outcome = sync_core::provider::refresh_provider_credential_if_needed(
        ProviderCredentialRefreshPlanInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:mal-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: now + 30 * 60,
            refresh_token: ProviderRefreshTokenState::present_expires_at(now + 60 * 60),
            client_id: "mal-client".to_owned(),
            client_secret: Some("mal-secret".to_owned()),
        },
        now,
        &mut transport,
        &mut secret_store,
    )
    .expect("near-expiry refresh token should route to reauth");

    assert_eq!(
        outcome,
        ProviderCredentialRefreshOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker
        }
    );
    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
    assert!(secret_store.stored.is_empty());
}

#[test]
fn refresh_provider_credential_if_needed_rejects_empty_new_credential_ref_after_secret_write() {
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-access-token-secret",
            "refresh_token": "rotated-refresh-token-secret"
        }"#,
    );
    let mut secret_store = FakeSecretStore::new("").with_refresh_token("old-refresh-token");

    let error = sync_core::provider::refresh_provider_credential_if_needed(
        ProviderCredentialRefreshPlanInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            client_id: "bangumi-client".to_owned(),
            client_secret: Some("bangumi-secret".to_owned()),
        },
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("empty new credential ref should not produce refreshed metadata");

    assert_eq!(
        error,
        ProviderAuthError::InvalidOAuthParameter {
            provider: Provider::Bangumi,
            field: "credential_store_ref",
        }
    );
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(transport.requests.len(), 1);
    assert_eq!(secret_store.stored.len(), 1);
}

#[test]
fn refresh_provider_credential_plan_input_redacts_secret_material_in_debug_output() {
    let input = ProviderCredentialRefreshPlanInput {
        provider: Provider::Bangumi,
        account_id: "bangumi-user-1".to_owned(),
        bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
        credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
        access_token_expires_at_epoch_secs: 1_700_000_300,
        refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
        client_id: "bangumi-client".to_owned(),
        client_secret: Some("bangumi-client-secret".to_owned()),
    };

    let debug = format!("{input:?}");

    assert!(debug.contains("Bangumi"));
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("secret-store:bangumi-user-1:v1"));
    assert!(!debug.contains("bangumi-client-secret"));
}

#[test]
fn provider_credential_capabilities_distinguish_refresh_from_reauth_only() {
    let bangumi = provider_credential_capability(Provider::Bangumi);
    assert_eq!(bangumi.provider, Provider::Bangumi);
    assert_eq!(
        bangumi.access_token_lifetime_seconds,
        Some(7 * 24 * 60 * 60)
    );
    assert_eq!(bangumi.refresh_policy, ProviderRefreshPolicy::RefreshToken);
    assert!(bangumi
        .bootstrap_modes
        .contains(&ProviderAuthBootstrapMode::AuthBroker));

    let myanimelist = provider_credential_capability(Provider::MyAnimeList);
    assert_eq!(myanimelist.provider, Provider::MyAnimeList);
    assert_eq!(myanimelist.access_token_lifetime_seconds, Some(60 * 60));
    assert_eq!(
        myanimelist.refresh_token_lifetime_seconds,
        Some(30 * 24 * 60 * 60)
    );
    assert_eq!(
        myanimelist.refresh_policy,
        ProviderRefreshPolicy::RefreshToken
    );

    let anilist = provider_credential_capability(Provider::AniList);
    assert_eq!(anilist.provider, Provider::AniList);
    assert_eq!(
        anilist.access_token_lifetime_seconds,
        Some(365 * 24 * 60 * 60)
    );
    assert_eq!(
        anilist.refresh_policy,
        ProviderRefreshPolicy::ReauthorizeOnly
    );
    assert!(anilist
        .bootstrap_modes
        .contains(&ProviderAuthBootstrapMode::ManualPin));
}

#[test]
fn provider_credential_preflight_refreshes_or_reauthorizes_before_expiry() {
    let now = 1_700_000_000;

    let bangumi_near_expiry = ProviderCredentialState::available(
        Provider::Bangumi,
        now + 30 * 60,
        ProviderRefreshTokenState::present_with_unknown_expiry(),
    );
    assert_eq!(
        plan_provider_credential_action(bangumi_near_expiry, now),
        ProviderCredentialAction::RefreshWithProvider
    );

    let anilist_near_expiry = ProviderCredentialState::available(
        Provider::AniList,
        now + 20 * 24 * 60 * 60,
        ProviderRefreshTokenState::absent(),
    );
    assert_eq!(
        plan_provider_credential_action(anilist_near_expiry, now),
        ProviderCredentialAction::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
        }
    );

    let missing = ProviderCredentialState::missing(Provider::MyAnimeList);
    assert_eq!(
        plan_provider_credential_action(missing, now),
        ProviderCredentialAction::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
        }
    );
}

#[test]
fn provider_credential_preflight_uses_stored_token_when_not_near_expiry() {
    let now = 1_700_000_000;

    let bangumi = ProviderCredentialState::available(
        Provider::Bangumi,
        now + 3 * 24 * 60 * 60,
        ProviderRefreshTokenState::present_expires_at(now + 14 * 24 * 60 * 60),
    );
    assert_eq!(
        plan_provider_credential_action(bangumi, now),
        ProviderCredentialAction::UseStoredAccessToken
    );

    let anilist = ProviderCredentialState::available(
        Provider::AniList,
        now + 180 * 24 * 60 * 60,
        ProviderRefreshTokenState::absent(),
    );
    assert_eq!(
        plan_provider_credential_action(anilist, now),
        ProviderCredentialAction::UseStoredAccessToken
    );
}

#[test]
fn provider_credential_preflight_reauthorizes_when_refresh_token_is_expired() {
    let now = 1_700_000_000;
    let myanimelist = ProviderCredentialState::available(
        Provider::MyAnimeList,
        now + 30,
        ProviderRefreshTokenState::present_expires_at(now - 1),
    );

    assert_eq!(
        plan_provider_credential_action(myanimelist, now),
        ProviderCredentialAction::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
        }
    );
}

#[test]
fn provider_credential_preflight_reauthorizes_before_refresh_token_expiry_window() {
    let now = 1_700_000_000;
    let myanimelist = ProviderCredentialState::available(
        Provider::MyAnimeList,
        now + 30 * 60,
        ProviderRefreshTokenState::present_expires_at(now + 60 * 60),
    );

    assert_eq!(
        plan_provider_credential_action(myanimelist, now),
        ProviderCredentialAction::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
        }
    );
}

#[test]
fn provider_credential_preflight_reauthorizes_when_refresh_token_is_absent() {
    let now = 1_700_000_000;
    let bangumi = ProviderCredentialState::available(
        Provider::Bangumi,
        now - 1,
        ProviderRefreshTokenState::absent(),
    );

    assert_eq!(
        plan_provider_credential_action(bangumi, now),
        ProviderCredentialAction::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
        }
    );
}

#[test]
fn parse_provider_rate_limit_reads_common_headers_case_insensitively() {
    let headers = vec![
        ProviderHttpHeader::public("x-ratelimit-limit", "90"),
        ProviderHttpHeader::public("X-RateLimit-Remaining", "0"),
        ProviderHttpHeader::public("Retry-After", "60"),
    ];

    let rate_limit = parse_provider_rate_limit(Provider::AniList, &headers);

    assert_eq!(rate_limit.provider, Provider::AniList);
    assert_eq!(rate_limit.limit, Some(90));
    assert_eq!(rate_limit.remaining, Some(0));
    assert_eq!(rate_limit.retry_after_seconds, Some(60));
    assert!(rate_limit.should_backoff());
}

#[test]
fn parse_provider_rate_limit_ignores_invalid_numbers() {
    let headers = vec![
        ProviderHttpHeader::public("X-RateLimit-Limit", "many"),
        ProviderHttpHeader::public("X-RateLimit-Remaining", "-1"),
        ProviderHttpHeader::public("Retry-After", "soon"),
    ];

    let rate_limit = parse_provider_rate_limit(Provider::Bangumi, &headers);

    assert_eq!(rate_limit.limit, None);
    assert_eq!(rate_limit.remaining, None);
    assert_eq!(rate_limit.retry_after_seconds, None);
    assert!(!rate_limit.should_backoff());
}
