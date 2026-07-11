use sync_core::model::{CollectionStatus, MediaKind, Provider};
use sync_core::provider::{
    AuthorizedProviderReadRequest, AuthorizedProviderReadTransport, ProviderAccessToken,
    ProviderAuthBootstrapMode, ProviderAuthError, ProviderCredentialAccessTokenStore,
    ProviderCredentialRefreshOutcome, ProviderCredentialSecretLookup,
    ProviderCredentialSecretStore, ProviderCredentialSecretStoreInput,
    ProviderOAuthTokenHttpResponse, ProviderOAuthTokenRequest, ProviderOAuthTokenTransport,
    ProviderReadRequestMethod, ProviderRefreshTokenState,
};
use sync_core::store::{
    ProviderCredentialInput, ProviderCredentialRefreshAttemptStatus, SqliteStore,
};
use sync_core::sync::{
    acquire_stored_provider_access_token, fetch_collection_snapshot_pages_with_stored_access_token,
    fetch_collection_snapshot_with_stored_access_token,
    preflight_stored_provider_credential_secret, refresh_stored_provider_credential_if_needed,
    StoredCredentialRefreshError, StoredProviderAccessTokenOutcome,
    StoredProviderCollectionReadOutcome, StoredProviderCredentialSecretPreflightOutcome,
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
    lookups: Vec<ProviderCredentialSecretLookup>,
    access_lookups: Vec<ProviderCredentialSecretLookup>,
    stored: Vec<ProviderCredentialSecretStoreInput>,
    access_token: String,
    refresh_token: String,
    next_ref: String,
    on_put: Option<Box<dyn FnMut()>>,
}

impl FakeSecretStore {
    fn new(refresh_token: &str, next_ref: &str) -> Self {
        Self {
            lookups: Vec::new(),
            access_lookups: Vec::new(),
            stored: Vec::new(),
            access_token: "stored-access-token".to_owned(),
            refresh_token: refresh_token.to_owned(),
            next_ref: next_ref.to_owned(),
            on_put: None,
        }
    }

    fn with_access_token(mut self, access_token: &str) -> Self {
        self.access_token = access_token.to_owned();
        self
    }

    fn with_on_put(mut self, on_put: impl FnMut() + 'static) -> Self {
        self.on_put = Some(Box::new(on_put));
        self
    }
}

impl ProviderCredentialAccessTokenStore for FakeSecretStore {
    fn get_provider_access_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<ProviderAccessToken, ProviderAuthError> {
        self.access_lookups.push(lookup);
        ProviderAccessToken::new(
            self.access_lookups
                .last()
                .expect("lookup should have just been recorded")
                .provider,
            self.access_token.clone(),
        )
    }
}

impl ProviderCredentialSecretStore for FakeSecretStore {
    fn get_provider_refresh_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<String, ProviderAuthError> {
        self.lookups.push(lookup);
        Ok(self.refresh_token.clone())
    }

    fn put_provider_tokens(
        &mut self,
        input: ProviderCredentialSecretStoreInput,
    ) -> Result<String, ProviderAuthError> {
        self.stored.push(input);
        if let Some(on_put) = &mut self.on_put {
            on_put();
        }
        Ok(self.next_ref.clone())
    }
}

struct RecordingAuthorizedReadTransport {
    requests: Vec<AuthorizedProviderReadRequest>,
    responses: Vec<Result<String, String>>,
}

impl RecordingAuthorizedReadTransport {
    fn ok(response_body: &str) -> Self {
        Self {
            requests: Vec::new(),
            responses: vec![Ok(response_body.to_owned())],
        }
    }

    fn sequence(response_bodies: Vec<&str>) -> Self {
        Self {
            requests: Vec::new(),
            responses: response_bodies
                .into_iter()
                .map(|body| Ok(body.to_owned()))
                .collect(),
        }
    }
}

impl AuthorizedProviderReadTransport for RecordingAuthorizedReadTransport {
    fn send_authorized_provider_read_request(
        &mut self,
        request: AuthorizedProviderReadRequest,
    ) -> Result<String, String> {
        self.requests.push(request);
        if self.responses.len() > 1 {
            self.responses.remove(0)
        } else {
            self.responses
                .last()
                .cloned()
                .unwrap_or_else(|| Err("missing fake response".to_owned()))
        }
    }
}

#[test]
fn stored_access_token_acquisition_rejects_invalid_inputs_before_secret_lookup() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "bad\ncredential-ref".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2");

    let account_error = acquire_stored_provider_access_token(
        &store,
        Provider::Bangumi,
        "bad\naccount",
        1_700_000_000,
        &mut secret_store,
    )
    .expect_err("invalid account id should fail before secret lookup");
    assert!(matches!(
        account_error,
        StoredCredentialRefreshError::Auth(ProviderAuthError::InvalidOAuthParameter {
            provider: Provider::Bangumi,
            field: "account_id",
        })
    ));

    let ref_error = acquire_stored_provider_access_token(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        1_700_000_000,
        &mut secret_store,
    )
    .expect_err("invalid stored credential ref should fail before secret lookup");
    assert!(matches!(
        ref_error,
        StoredCredentialRefreshError::Auth(ProviderAuthError::InvalidOAuthParameter {
            provider: Provider::Bangumi,
            field: "credential_store_ref",
        })
    ));

    assert!(secret_store.access_lookups.is_empty());
    assert!(secret_store.lookups.is_empty());
    assert!(secret_store.stored.is_empty());
}

#[test]
fn stored_access_token_acquisition_uses_secret_store_only_when_metadata_is_valid() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2")
            .with_access_token("valid-access-token-secret");

    let outcome = acquire_stored_provider_access_token(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        1_700_000_000,
        &mut secret_store,
    )
    .expect("valid stored credential should produce an access token");

    let StoredProviderAccessTokenOutcome::Authorized(token) = outcome else {
        panic!("expected authorized token");
    };
    assert_eq!(token.provider(), Provider::Bangumi);
    assert_eq!(secret_store.access_lookups.len(), 1);
    assert_eq!(
        secret_store.access_lookups[0].credential_store_ref,
        "secret-store:bangumi-user-1:v1"
    );
    assert!(secret_store.lookups.is_empty());
    assert!(secret_store.stored.is_empty());
    let debug = format!("{token:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("valid-access-token-secret"));
}

#[test]
fn stored_access_token_acquisition_does_not_read_secret_when_refresh_or_reauth_is_required() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("bangumi credential metadata should insert");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:anilist-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_701_728_000,
            refresh_token: ProviderRefreshTokenState::absent(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: Some(1_699_999_000),
        })
        .expect("anilist credential metadata should insert");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2");

    let bangumi_outcome = acquire_stored_provider_access_token(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        1_700_000_000,
        &mut secret_store,
    )
    .expect("near-expiry refreshable credential should be classified");
    assert_eq!(
        bangumi_outcome,
        StoredProviderAccessTokenOutcome::RefreshRequired
    );

    let anilist_outcome = acquire_stored_provider_access_token(
        &store,
        Provider::AniList,
        "anilist-user-1",
        1_700_000_000,
        &mut secret_store,
    )
    .expect("near-expiry reauth-only credential should be classified");
    assert_eq!(
        anilist_outcome,
        StoredProviderAccessTokenOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker
        }
    );

    let missing_outcome = acquire_stored_provider_access_token(
        &store,
        Provider::MyAnimeList,
        "mal-user-1",
        1_700_000_000,
        &mut secret_store,
    )
    .expect("missing credential should be classified");
    assert_eq!(
        missing_outcome,
        StoredProviderAccessTokenOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker
        }
    );

    assert!(secret_store.access_lookups.is_empty());
    assert!(secret_store.lookups.is_empty());
    assert!(secret_store.stored.is_empty());
}

#[test]
fn stored_access_token_acquisition_rejects_invalid_secret_store_token() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2")
            .with_access_token("bad\naccess-token-secret");

    let error = acquire_stored_provider_access_token(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        1_700_000_000,
        &mut secret_store,
    )
    .expect_err("invalid stored access token should be rejected");

    assert!(matches!(
        error,
        StoredCredentialRefreshError::Auth(ProviderAuthError::InvalidBearerToken {
            provider: Provider::Bangumi
        })
    ));
    assert_eq!(secret_store.access_lookups.len(), 1);
    assert!(secret_store.lookups.is_empty());
    assert!(secret_store.stored.is_empty());
    assert!(!format!("{error:?}").contains("bad\naccess-token-secret"));
}

#[test]
fn stored_credential_secret_preflight_reads_only_the_secret_needed_for_the_planned_action() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-valid-user".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-valid-user:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("valid bangumi credential metadata should insert");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-refresh-user".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-refresh-user:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("refreshable bangumi credential metadata should insert");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::AniList,
            account_id: "anilist-reauth-user".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "bad\nsecret-store:anilist-reauth-user:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::absent(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("reauth-only anilist credential metadata should insert");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2")
            .with_access_token("valid-access-token-secret");

    let valid_outcome = preflight_stored_provider_credential_secret(
        &store,
        Provider::Bangumi,
        "bangumi-valid-user",
        1_700_000_000,
        &mut secret_store,
    )
    .expect("valid credential secret should preflight");
    assert_eq!(
        valid_outcome,
        StoredProviderCredentialSecretPreflightOutcome::AccessTokenReadable
    );
    assert_eq!(secret_store.access_lookups.len(), 1);
    assert_eq!(
        secret_store.access_lookups[0].credential_store_ref,
        "secret-store:bangumi-valid-user:v1"
    );
    assert!(secret_store.lookups.is_empty());
    assert!(secret_store.stored.is_empty());

    let refresh_outcome = preflight_stored_provider_credential_secret(
        &store,
        Provider::Bangumi,
        "bangumi-refresh-user",
        1_700_000_000,
        &mut secret_store,
    )
    .expect("refresh token should preflight");
    assert_eq!(
        refresh_outcome,
        StoredProviderCredentialSecretPreflightOutcome::RefreshTokenReadable
    );
    assert_eq!(secret_store.access_lookups.len(), 1);
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(
        secret_store.lookups[0].credential_store_ref,
        "secret-store:bangumi-refresh-user:v1"
    );
    assert!(secret_store.stored.is_empty());

    let reauth_outcome = preflight_stored_provider_credential_secret(
        &store,
        Provider::AniList,
        "anilist-reauth-user",
        1_700_000_000,
        &mut secret_store,
    )
    .expect("reauthorize action should not read secrets");
    assert_eq!(
        reauth_outcome,
        StoredProviderCredentialSecretPreflightOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker
        }
    );
    assert_eq!(secret_store.access_lookups.len(), 1);
    assert_eq!(secret_store.lookups.len(), 1);
    assert!(secret_store.stored.is_empty());
}

#[test]
fn stored_collection_read_authorizes_mock_transport_with_stored_access_token() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2")
            .with_access_token("valid-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::ok(
        r#"{
            "data": [
                {
                    "subject_id": 253,
                    "subject_type": "anime",
                    "collection_type": "collect",
                    "rate": 10,
                    "ep_status": 26,
                    "vol_status": 0
                }
            ]
        }"#,
    );

    let outcome = fetch_collection_snapshot_with_stored_access_token(
        &store,
        Provider::Bangumi,
        MediaKind::Anime,
        "bangumi-user-1",
        50,
        0,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("stored credential should authorize and read collection");

    let StoredProviderCollectionReadOutcome::Snapshot(snapshot) = outcome else {
        panic!("expected parsed collection snapshot");
    };
    assert_eq!(snapshot.provider(), Provider::Bangumi);
    assert_eq!(snapshot.media_kind(), MediaKind::Anime);
    assert_eq!(snapshot.entries().len(), 1);
    assert_eq!(snapshot.entries()[0].provider_entry_id(), "253");
    assert_eq!(snapshot.entries()[0].status(), CollectionStatus::Completed);
    assert_eq!(snapshot.entries()[0].progress().episodes(), Some(26));

    assert_eq!(secret_store.access_lookups.len(), 1);
    assert_eq!(
        secret_store.access_lookups[0].credential_store_ref,
        "secret-store:bangumi-user-1:v1"
    );
    assert!(secret_store.lookups.is_empty());
    assert!(secret_store.stored.is_empty());

    assert_eq!(read_transport.requests.len(), 1);
    let authorized = &read_transport.requests[0];
    assert_eq!(authorized.request.provider, Provider::Bangumi);
    assert_eq!(authorized.request.method, ProviderReadRequestMethod::Get);
    assert!(authorized.request.url.contains("subject_type=2"));
    assert!(!authorized.request.url.contains("valid-access-token-secret"));
    assert!(!authorized
        .request
        .body
        .as_deref()
        .unwrap_or_default()
        .contains("valid-access-token-secret"));
    assert!(authorized.headers.iter().any(|header| {
        header.name == "Authorization"
            && header.value == "Bearer valid-access-token-secret"
            && header.sensitive
    }));

    let debug = format!("{authorized:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("valid-access-token-secret"));
}

#[test]
fn stored_collection_read_pages_authorizes_each_mock_transport_request() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2")
            .with_access_token("valid-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::sequence(vec![
        r#"{
            "data": [
                {
                    "subject_id": 253,
                    "subject_type": "anime",
                    "collection_type": "collect",
                    "rate": 10,
                    "ep_status": 26,
                    "vol_status": 0
                },
                {
                    "subject_id": 998,
                    "subject_type": "anime",
                    "collection_type": "do",
                    "rate": 8,
                    "ep_status": 3,
                    "vol_status": 0
                }
            ]
        }"#,
        r#"{
            "data": [
                {
                    "subject_id": 999,
                    "subject_type": "anime",
                    "collection_type": "wish",
                    "rate": 0,
                    "ep_status": 0,
                    "vol_status": 0
                }
            ]
        }"#,
    ]);

    let outcome = fetch_collection_snapshot_pages_with_stored_access_token(
        &store,
        Provider::Bangumi,
        MediaKind::Anime,
        "bangumi-user-1",
        2,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("stored credential should authorize and read collection pages");

    let StoredProviderCollectionReadOutcome::Snapshot(snapshot) = outcome else {
        panic!("expected parsed collection snapshot");
    };
    assert_eq!(snapshot.entries().len(), 3);
    assert_eq!(snapshot.entries()[2].provider_entry_id(), "999");
    assert_eq!(secret_store.access_lookups.len(), 1);
    assert!(secret_store.lookups.is_empty());
    assert!(secret_store.stored.is_empty());

    assert_eq!(read_transport.requests.len(), 2);
    assert!(read_transport.requests[0].request.url.contains("offset=0"));
    assert!(read_transport.requests[1].request.url.contains("offset=2"));
    for authorized in &read_transport.requests {
        assert!(authorized.headers.iter().any(|header| {
            header.name == "Authorization"
                && header.value == "Bearer valid-access-token-secret"
                && header.sensitive
        }));
        assert!(!format!("{authorized:?}").contains("valid-access-token-secret"));
    }
}

#[test]
fn stored_credential_refresh_updates_sqlite_metadata_without_persisting_tokens() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-access-token-secret",
            "refresh_token": "rotated-refresh-token-secret"
        }"#,
    );
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2");

    let outcome = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        "bangumi-client".to_owned(),
        Some("bangumi-secret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("stored credential should refresh");

    let ProviderCredentialRefreshOutcome::Refreshed(result) = outcome else {
        panic!("expected refreshed outcome");
    };
    assert_eq!(
        result.credential_store_ref,
        "secret-store:bangumi-user-1:v2"
    );
    assert_eq!(result.last_refresh_at_epoch_secs, Some(1_700_000_000));
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(transport.requests.len(), 1);
    assert_eq!(secret_store.stored.len(), 1);
    assert_eq!(
        secret_store.stored[0].refresh_token.as_deref(),
        Some("rotated-refresh-token-secret")
    );

    let persisted = store
        .provider_credential(Provider::Bangumi, "bangumi-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_eq!(
        persisted.credential_store_ref,
        "secret-store:bangumi-user-1:v2"
    );
    assert_eq!(persisted.access_token_expires_at_epoch_secs, 1_700_003_600);
    assert_eq!(
        persisted.refresh_token,
        ProviderRefreshTokenState::present_with_unknown_expiry()
    );
    assert_eq!(persisted.last_refresh_at_epoch_secs, Some(1_700_000_000));
    let debug = format!("{persisted:?}");
    assert!(!debug.contains("new-access-token-secret"));
    assert!(!debug.contains("rotated-refresh-token-secret"));
}

#[test]
fn stored_refresh_secret_written_but_metadata_commit_conflict_is_fail_closed() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-refresh-conflict-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-refresh-conflict.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = SqliteStore::open(&db_path).expect("sqlite store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    let mut transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-access-token-secret",
            "refresh_token": "rotated-refresh-token-secret"
        }"#,
    );
    let db_path_for_concurrent_update = db_path.clone();
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2").with_on_put(
            move || {
                let store = SqliteStore::open(&db_path_for_concurrent_update)
                    .expect("concurrent store should open");
                store
                    .upsert_provider_credential(ProviderCredentialInput {
                        provider: Provider::Bangumi,
                        account_id: "bangumi-user-1".to_owned(),
                        auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
                        bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
                        credential_store_ref: "secret-store:bangumi-user-1:v-concurrent".to_owned(),
                        access_token_expires_at_epoch_secs: 1_700_007_200,
                        refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
                        last_refresh_at_epoch_secs: Some(1_700_000_010),
                        last_reauth_request_at_epoch_secs: None,
                    })
                    .expect("concurrent credential metadata should update");
            },
        );

    let error = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        "bangumi-client".to_owned(),
        Some("bangumi-secret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("stale metadata commit should fail closed");

    let attempt_id = match error {
        StoredCredentialRefreshError::RefreshCommitConflict {
            provider,
            account_id,
            attempt_id,
        } => {
            assert_eq!(provider, Provider::Bangumi);
            assert_eq!(account_id, "bangumi-user-1");
            attempt_id
        }
        other => panic!("unexpected error: {other:?}"),
    };
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(transport.requests.len(), 1);
    assert_eq!(secret_store.stored.len(), 1);

    let persisted = store
        .provider_credential(Provider::Bangumi, "bangumi-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_eq!(
        persisted.credential_store_ref,
        "secret-store:bangumi-user-1:v-concurrent"
    );
    assert_eq!(persisted.access_token_expires_at_epoch_secs, 1_700_007_200);
    assert_eq!(persisted.last_refresh_at_epoch_secs, Some(1_700_000_010));

    let attempt = store
        .provider_credential_refresh_attempt(attempt_id)
        .expect("attempt lookup should work")
        .expect("attempt should remain observable");
    assert_eq!(
        attempt.status,
        ProviderCredentialRefreshAttemptStatus::Failed
    );
    assert_eq!(
        attempt.failure_kind.as_deref(),
        Some("credential_store_ref_conflict")
    );
    assert_eq!(
        attempt.attempted_credential_store_ref.as_deref(),
        Some("secret-store:bangumi-user-1:v2")
    );
    let debug = format!("{attempt:?}");
    assert!(!debug.contains("new-access-token-secret"));
    assert!(!debug.contains("rotated-refresh-token-secret"));

    drop(store);
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_dir(temp_dir);
}

#[test]
fn stored_credential_refresh_missing_metadata_reauthorizes_without_secret_or_transport() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let mut transport = FakeTokenTransport::ok("{}");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2");

    let outcome = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        "bangumi-client".to_owned(),
        Some("bangumi-secret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("missing metadata should produce reauth outcome");

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
fn stored_credential_refresh_valid_metadata_uses_stored_token_without_updating_sqlite() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut transport = FakeTokenTransport::ok("{}");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2");

    let outcome = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        "bangumi-client".to_owned(),
        Some("bangumi-secret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("valid stored credential should not refresh");

    assert_eq!(
        outcome,
        ProviderCredentialRefreshOutcome::UseStoredAccessToken
    );
    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
    assert!(secret_store.stored.is_empty());
    let persisted = store
        .provider_credential(Provider::Bangumi, "bangumi-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_eq!(
        persisted.credential_store_ref,
        "secret-store:bangumi-user-1:v1"
    );
    assert_eq!(persisted.access_token_expires_at_epoch_secs, 1_700_259_200);
    assert_eq!(persisted.last_refresh_at_epoch_secs, Some(1_699_999_000));
}

#[test]
fn stored_credential_refresh_anilist_near_expiry_reauthorizes_without_secret_or_transport() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:anilist-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_701_728_000,
            refresh_token: ProviderRefreshTokenState::absent(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: Some(1_699_999_000),
        })
        .expect("credential metadata should insert");
    let mut transport = FakeTokenTransport::ok("{}");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:anilist-user-1:v2");

    let outcome = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::AniList,
        "anilist-user-1",
        "anilist-client".to_owned(),
        Some("anilist-secret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect("anilist near-expiry credential should request reauth");

    assert_eq!(
        outcome,
        ProviderCredentialRefreshOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker
        }
    );
    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
    assert!(secret_store.stored.is_empty());
    let persisted = store
        .provider_credential(Provider::AniList, "anilist-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_eq!(
        persisted.credential_store_ref,
        "secret-store:anilist-user-1:v1"
    );
    assert_eq!(persisted.access_token_expires_at_epoch_secs, 1_701_728_000);
    assert_eq!(
        persisted.last_reauth_request_at_epoch_secs,
        Some(1_699_999_000)
    );
}

#[test]
fn stored_credential_refresh_invalid_parameters_fail_before_secret_or_transport() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let mut transport = FakeTokenTransport::ok("{}");
    let mut secret_store =
        FakeSecretStore::new("old-refresh-token", "secret-store:bangumi-user-1:v2");

    let account_error = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::Bangumi,
        "bangumi-user-1\n",
        "bangumi-client".to_owned(),
        Some("bangumi-secret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("invalid account id should fail closed");
    assert!(
        matches!(
            account_error,
            StoredCredentialRefreshError::Auth(ProviderAuthError::InvalidOAuthParameter {
                provider: Provider::Bangumi,
                field: "account_id",
            })
        ),
        "unexpected error: {account_error:?}"
    );

    let client_error = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        " ".to_owned(),
        Some("bangumi-secret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("invalid client id should fail closed");
    assert!(
        matches!(
            client_error,
            StoredCredentialRefreshError::Auth(ProviderAuthError::InvalidOAuthParameter {
                provider: Provider::Bangumi,
                field: "client_id",
            })
        ),
        "unexpected error: {client_error:?}"
    );

    let client_secret_error = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::Bangumi,
        "bangumi-user-1",
        "bangumi-client".to_owned(),
        Some("bad\nsecret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("invalid client secret should fail closed");
    assert!(
        matches!(
            client_secret_error,
            StoredCredentialRefreshError::Auth(ProviderAuthError::InvalidOAuthParameter {
                provider: Provider::Bangumi,
                field: "client_secret",
            })
        ),
        "unexpected error: {client_secret_error:?}"
    );

    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
    assert!(secret_store.stored.is_empty());
}

#[test]
fn stored_credential_refresh_failure_keeps_existing_sqlite_metadata() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:mal-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::present_expires_at(1_702_592_000),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut transport = FakeTokenTransport::with_status(
        401,
        r#"{"error":"invalid_grant","access_token":"must-not-store"}"#,
    );
    let mut secret_store = FakeSecretStore::new("old-refresh-token", "secret-store:mal-user-1:v2");

    let error = refresh_stored_provider_credential_if_needed(
        &store,
        Provider::MyAnimeList,
        "mal-user-1",
        "mal-client".to_owned(),
        Some("mal-secret".to_owned()),
        1_700_000_000,
        &mut transport,
        &mut secret_store,
    )
    .expect_err("failed provider refresh should not update metadata");

    assert!(format!("{error:?}").contains("UnexpectedOAuthStatus"));
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(transport.requests.len(), 1);
    assert!(secret_store.stored.is_empty());
    let persisted = store
        .provider_credential(Provider::MyAnimeList, "mal-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_eq!(persisted.credential_store_ref, "secret-store:mal-user-1:v1");
    assert_eq!(persisted.access_token_expires_at_epoch_secs, 1_700_000_300);
    assert_eq!(persisted.last_refresh_at_epoch_secs, None);
}
