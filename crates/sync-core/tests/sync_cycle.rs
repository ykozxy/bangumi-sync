use sync_core::identity::import_legacy_manual_relations;
use sync_core::model::{MediaKind, Provider, SyncField};
use sync_core::provider::{
    AuthorizedProviderReadRequest, AuthorizedProviderReadTransport, ProviderAccessToken,
    ProviderAuthBootstrapMode, ProviderAuthError, ProviderCredentialAccessTokenStore,
    ProviderCredentialSecretLookup, ProviderCredentialSecretStore,
    ProviderCredentialSecretStoreInput, ProviderOAuthTokenHttpResponse, ProviderOAuthTokenRequest,
    ProviderOAuthTokenTransport, ProviderRefreshTokenState,
};
use sync_core::store::{ProviderCredentialInput, SqliteStore};
use sync_core::sync::{
    refresh_snapshots_and_plan_dry_run_with_auto_refresh,
    refresh_snapshots_and_plan_dry_run_with_stored_access_tokens, PlannedActionKind,
    ProviderSyncCredentialRefreshConfig, StoredProviderCollectionImportOutcome,
    StoredProviderSyncCycleOutcome,
};

struct FakeAccessTokenStore {
    lookups: Vec<ProviderCredentialSecretLookup>,
}

impl FakeAccessTokenStore {
    fn new() -> Self {
        Self {
            lookups: Vec::new(),
        }
    }
}

impl ProviderCredentialAccessTokenStore for FakeAccessTokenStore {
    fn get_provider_access_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<ProviderAccessToken, ProviderAuthError> {
        self.lookups.push(lookup);
        ProviderAccessToken::new(
            self.lookups
                .last()
                .expect("lookup should have just been recorded")
                .provider,
            "valid-access-token-secret",
        )
    }
}

struct FakeCredentialStore {
    access_lookups: Vec<ProviderCredentialSecretLookup>,
    refresh_lookups: Vec<ProviderCredentialSecretLookup>,
    stored: Vec<ProviderCredentialSecretStoreInput>,
    access_token: String,
    refresh_token: String,
    next_ref: String,
}

impl FakeCredentialStore {
    fn new(access_token: &str, refresh_token: &str, next_ref: &str) -> Self {
        Self {
            access_lookups: Vec::new(),
            refresh_lookups: Vec::new(),
            stored: Vec::new(),
            access_token: access_token.to_owned(),
            refresh_token: refresh_token.to_owned(),
            next_ref: next_ref.to_owned(),
        }
    }
}

impl ProviderCredentialAccessTokenStore for FakeCredentialStore {
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

impl ProviderCredentialSecretStore for FakeCredentialStore {
    fn get_provider_refresh_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<String, ProviderAuthError> {
        self.refresh_lookups.push(lookup);
        Ok(self.refresh_token.clone())
    }

    fn put_provider_tokens(
        &mut self,
        input: ProviderCredentialSecretStoreInput,
    ) -> Result<String, ProviderAuthError> {
        self.access_token = input.access_token.clone();
        self.stored.push(input);
        Ok(self.next_ref.clone())
    }
}

struct FakeTokenTransport {
    requests: Vec<ProviderOAuthTokenRequest>,
    response_body: String,
}

impl FakeTokenTransport {
    fn ok(response_body: &str) -> Self {
        Self {
            requests: Vec::new(),
            response_body: response_body.to_owned(),
        }
    }
}

impl ProviderOAuthTokenTransport for FakeTokenTransport {
    fn send_token_request(
        &mut self,
        request: &ProviderOAuthTokenRequest,
    ) -> Result<ProviderOAuthTokenHttpResponse, ProviderAuthError> {
        self.requests.push(request.clone());
        Ok(ProviderOAuthTokenHttpResponse {
            status: 200,
            body: self.response_body.clone(),
        })
    }
}

struct SequencedAuthorizedReadTransport {
    requests: Vec<AuthorizedProviderReadRequest>,
    responses: Vec<Result<String, String>>,
}

impl SequencedAuthorizedReadTransport {
    fn new(response_bodies: Vec<&str>) -> Self {
        Self {
            requests: Vec::new(),
            responses: response_bodies
                .into_iter()
                .map(|body| Ok(body.to_owned()))
                .collect(),
        }
    }
}

impl AuthorizedProviderReadTransport for SequencedAuthorizedReadTransport {
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
fn sync_cycle_refreshes_selected_snapshots_before_building_plan() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    seed_valid_credential(&store, Provider::Bangumi, "fixture-account");
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    let mut secret_store = FakeAccessTokenStore::new();
    let mut read_transport = SequencedAuthorizedReadTransport::new(vec![
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        r#"{"data":[{"episode":{"id":1001,"type":0,"sort":1,"ep":1},"type":2,"updated_at":1700000000},{"episode":{"id":1002,"type":0,"sort":2,"ep":2},"type":2,"updated_at":1700000001}]}"#,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    ]);

    let outcome = refresh_snapshots_and_plan_dry_run_with_stored_access_tokens(
        &store,
        "fixture-account",
        &[MediaKind::Anime],
        &[Provider::Bangumi, Provider::AniList],
        false,
        50,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("sync cycle should refresh snapshots and plan");

    let StoredProviderSyncCycleOutcome::Planned { imports, plan } = outcome else {
        panic!("expected completed dry-run plan");
    };
    assert_eq!(imports.len(), 2);
    assert!(imports.iter().all(|summary| matches!(
        summary.outcome,
        StoredProviderCollectionImportOutcome::Imported { imported: 1 }
    )));
    assert_eq!(secret_store.lookups.len(), 2);
    assert_eq!(read_transport.requests.len(), 3);
    assert!(read_transport.requests[1]
        .request
        .url
        .contains("/v0/users/-/collections/253/episodes?episode_type=0"));
    assert_eq!(
        store
            .bangumi_episode_ids_for_done_prefix("fixture-account", "253", 2)
            .expect("Bangumi episode ids should query"),
        vec![1001, 1002]
    );
    assert_eq!(plan.actions().len(), 1);
    let action = &plan.actions()[0];
    assert_eq!(action.kind, PlannedActionKind::UpdateEntry);
    assert_eq!(action.source_provider, Provider::Bangumi);
    assert_eq!(action.target_provider, Provider::AniList);
    assert_eq!(action.target_provider_entry_id, "1");
    assert_eq!(action.field_updates, vec![SyncField::Score]);
}

#[test]
fn sync_cycle_uses_refreshed_anilist_id_mal_crosswalk_without_manual_relation() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    seed_valid_credential(&store, Provider::MyAnimeList, "fixture-account");
    let mut secret_store = FakeAccessTokenStore::new();
    let mut read_transport = SequencedAuthorizedReadTransport::new(vec![
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME", "idMal": 5114 },
                                    "status": "COMPLETED",
                                    "score": 90,
                                    "progress": 64,
                                    "progressVolumes": null
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        r#"{
            "data": [
                {
                    "node": { "id": 5114, "media_type": "anime" },
                    "list_status": {
                        "status": "completed",
                        "score": 0,
                        "num_episodes_watched": 64
                    }
                }
            ]
        }"#,
    ]);

    let outcome = refresh_snapshots_and_plan_dry_run_with_stored_access_tokens(
        &store,
        "fixture-account",
        &[MediaKind::Anime],
        &[Provider::AniList, Provider::MyAnimeList],
        false,
        500,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("sync cycle should refresh snapshots and plan");

    let StoredProviderSyncCycleOutcome::Planned { imports, plan } = outcome else {
        panic!("expected completed dry-run plan");
    };
    assert_eq!(imports.len(), 2);
    assert!(imports.iter().all(|summary| matches!(
        summary.outcome,
        StoredProviderCollectionImportOutcome::Imported { imported: 1 }
    )));

    let anilist_work_id = store
        .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    let mal_work_id = store
        .find_work_by_external_id(Provider::MyAnimeList, MediaKind::Anime, "5114")
        .expect("mal edge lookup should work")
        .expect("mal edge should exist");
    assert_eq!(anilist_work_id, mal_work_id);

    assert_eq!(plan.actions().len(), 1);
    let action = &plan.actions()[0];
    assert_eq!(action.kind, PlannedActionKind::UpdateEntry);
    assert_eq!(action.source_provider, Provider::AniList);
    assert_eq!(action.target_provider, Provider::MyAnimeList);
    assert_eq!(action.target_provider_entry_id, "5114");
    assert_eq!(action.field_updates, vec![SyncField::Score]);
}

#[test]
fn sync_cycle_does_not_plan_when_any_selected_snapshot_needs_credentials() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    seed_refresh_required_credential(&store, Provider::Bangumi, "fixture-account");
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    let mut secret_store = FakeAccessTokenStore::new();
    let mut read_transport = SequencedAuthorizedReadTransport::new(vec![
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    ]);

    let outcome = refresh_snapshots_and_plan_dry_run_with_stored_access_tokens(
        &store,
        "fixture-account",
        &[MediaKind::Anime],
        &[Provider::Bangumi, Provider::AniList],
        false,
        50,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("sync cycle should report credential requirement");

    let StoredProviderSyncCycleOutcome::CredentialsRequired { imports } = outcome else {
        panic!("expected credential-required outcome without plan");
    };
    assert_eq!(imports.len(), 2);
    assert_eq!(
        imports[0].outcome,
        StoredProviderCollectionImportOutcome::RefreshRequired
    );
    assert_eq!(
        imports[1].outcome,
        StoredProviderCollectionImportOutcome::Imported { imported: 1 }
    );
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(read_transport.requests.len(), 1);
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        1
    );
}

#[test]
fn sync_cycle_auto_refreshes_expired_provider_before_reading_snapshot() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    seed_refresh_required_credential(&store, Provider::Bangumi, "fixture-account");
    let refreshed_ref = "secret-store:bangumi:fixture-account:v2";
    let mut secret_store = FakeCredentialStore::new(
        "stale-access-token-secret",
        "old-refresh-token-secret",
        refreshed_ref,
    );
    let mut token_transport = FakeTokenTransport::ok(
        r#"{
            "token_type": "Bearer",
            "expires_in": 200000,
            "access_token": "refreshed-access-token-secret",
            "refresh_token": "rotated-refresh-token-secret"
        }"#,
    );
    let mut read_transport = SequencedAuthorizedReadTransport::new(vec![
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        r#"{"data":[]}"#,
    ]);

    let outcome = refresh_snapshots_and_plan_dry_run_with_auto_refresh(
        &store,
        "fixture-account",
        &[MediaKind::Anime],
        &[Provider::Bangumi],
        false,
        &[ProviderSyncCredentialRefreshConfig {
            provider: Provider::Bangumi,
            client_id: "bangumi-client".to_owned(),
            client_secret: Some("bangumi-secret".to_owned()),
        }],
        50,
        1_700_000_000,
        &mut token_transport,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("sync cycle should refresh credential before reading snapshot");

    let StoredProviderSyncCycleOutcome::Planned { imports, plan } = outcome else {
        panic!("expected completed dry-run plan after automatic refresh");
    };
    assert_eq!(imports.len(), 1);
    assert_eq!(
        imports[0].outcome,
        StoredProviderCollectionImportOutcome::Imported { imported: 1 }
    );
    assert!(plan.actions().is_empty());
    assert_eq!(token_transport.requests.len(), 1);
    assert_eq!(secret_store.refresh_lookups.len(), 1);
    assert_eq!(
        secret_store.refresh_lookups[0].credential_store_ref,
        "secret-store:bangumi:fixture-account:v1"
    );
    assert_eq!(secret_store.stored.len(), 1);
    assert_eq!(
        secret_store.stored[0].refresh_token.as_deref(),
        Some("rotated-refresh-token-secret")
    );
    assert_eq!(secret_store.access_lookups.len(), 1);
    assert_eq!(
        secret_store.access_lookups[0].credential_store_ref,
        refreshed_ref
    );
    assert_eq!(read_transport.requests.len(), 2);
    assert!(read_transport.requests[0].headers.iter().any(|header| {
        header.name == "Authorization"
            && header.value == "Bearer refreshed-access-token-secret"
            && header.sensitive
    }));
    assert!(read_transport.requests[1]
        .request
        .url
        .contains("/v0/users/-/collections/253/episodes?episode_type=0"));

    let persisted = store
        .provider_credential(Provider::Bangumi, "fixture-account")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_eq!(persisted.credential_store_ref, refreshed_ref);
    assert_eq!(persisted.access_token_expires_at_epoch_secs, 1_700_200_000);
    assert_eq!(persisted.last_refresh_at_epoch_secs, Some(1_700_000_000));
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        1
    );
}

#[test]
fn sync_cycle_plan_uses_latest_snapshot_membership_after_refresh() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1], [999, 2]]")
        .expect("manual relation import should work");
    seed_valid_credential(&store, Provider::Bangumi, "fixture-account");
    seed_valid_credential(&store, Provider::AniList, "fixture-account");

    let stale_bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":999,"subject_type":"anime","collection_type":"wish","rate":0,"ep_status":0,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("stale bangumi fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &stale_bangumi)
        .expect("stale snapshot should import");
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        2
    );

    let mut secret_store = FakeAccessTokenStore::new();
    let mut read_transport = SequencedAuthorizedReadTransport::new(vec![
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        r#"{"data":[]}"#,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    ]);

    let outcome = refresh_snapshots_and_plan_dry_run_with_stored_access_tokens(
        &store,
        "fixture-account",
        &[MediaKind::Anime],
        &[Provider::Bangumi, Provider::AniList],
        false,
        50,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("sync cycle should refresh snapshots and plan");

    let StoredProviderSyncCycleOutcome::Planned { plan, .. } = outcome else {
        panic!("expected completed dry-run plan");
    };
    assert_eq!(plan.actions().len(), 1);
    assert!(plan
        .actions()
        .iter()
        .all(|action| action.target_provider_entry_id != "2"));
    assert!(store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "999",
        )
        .expect("lookup should work")
        .is_none());
}

fn seed_valid_credential(store: &SqliteStore, provider: Provider, account_id: &str) {
    let expires_at = match provider {
        Provider::AniList => 1_735_000_000,
        Provider::Bangumi | Provider::MyAnimeList => 1_700_259_200,
    };
    seed_credential(store, provider, account_id, expires_at);
}

fn seed_refresh_required_credential(store: &SqliteStore, provider: Provider, account_id: &str) {
    seed_credential(store, provider, account_id, 1_700_000_300);
}

fn seed_credential(
    store: &SqliteStore,
    provider: Provider,
    account_id: &str,
    access_token_expires_at_epoch_secs: i64,
) {
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider,
            account_id: account_id.to_owned(),
            auth_flow: sync_core::provider::provider_credential_capability(provider).auth_flow,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: format!("secret-store:{}:{account_id}:v1", provider.as_str()),
            access_token_expires_at_epoch_secs,
            refresh_token: refresh_token_for_provider(provider),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
}

fn refresh_token_for_provider(provider: Provider) -> ProviderRefreshTokenState {
    match provider {
        Provider::AniList => ProviderRefreshTokenState::absent(),
        Provider::Bangumi | Provider::MyAnimeList => {
            ProviderRefreshTokenState::present_with_unknown_expiry()
        }
    }
}
