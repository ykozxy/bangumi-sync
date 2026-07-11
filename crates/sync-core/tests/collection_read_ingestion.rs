use sync_core::model::{CollectionStatus, MediaKind, Provider};
use sync_core::provider::{
    AuthorizedProviderReadRequest, AuthorizedProviderReadTransport, ProviderAccessToken,
    ProviderAuthBootstrapMode, ProviderAuthError, ProviderCredentialAccessTokenStore,
    ProviderCredentialSecretLookup, ProviderRefreshTokenState,
};
use sync_core::store::{ProviderCredentialInput, SqliteStore};
use sync_core::sync::{
    import_collection_snapshot_pages_with_stored_access_token,
    StoredProviderCollectionImportOutcome,
};

struct FakeAccessTokenStore {
    lookups: Vec<ProviderCredentialSecretLookup>,
    access_token: String,
}

impl FakeAccessTokenStore {
    fn new(access_token: &str) -> Self {
        Self {
            lookups: Vec::new(),
            access_token: access_token.to_owned(),
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
            self.access_token.clone(),
        )
    }
}

struct RecordingAuthorizedReadTransport {
    requests: Vec<AuthorizedProviderReadRequest>,
    responses: Vec<Result<String, String>>,
}

impl RecordingAuthorizedReadTransport {
    fn ok(response_body: &str) -> Self {
        Self::sequence(vec![response_body])
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
        if self.responses.is_empty() {
            return Err("missing fake response".to_owned());
        }
        self.responses.remove(0)
    }
}

#[test]
fn stored_collection_import_fetches_authorized_snapshot_and_upserts_sqlite() {
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
    let mut secret_store = FakeAccessTokenStore::new("valid-access-token-secret");
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
                }
            ]
        }"#,
        r#"{
            "data": [
                {
                    "episode": { "id": 1001, "type": 0, "sort": 1, "ep": 1 },
                    "type": 2,
                    "updated_at": 1700000000
                }
            ]
        }"#,
    ]);

    let outcome = import_collection_snapshot_pages_with_stored_access_token(
        &store,
        Provider::Bangumi,
        MediaKind::Anime,
        "bangumi-user-1",
        50,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("stored credential should fetch and import snapshot");

    assert_eq!(
        outcome,
        StoredProviderCollectionImportOutcome::Imported { imported: 1 }
    );
    assert_eq!(
        store
            .collection_entry_count("bangumi-user-1")
            .expect("collection count should load"),
        1
    );
    let details = store
        .collection_entry_details("bangumi-user-1", Provider::Bangumi, MediaKind::Anime, "253")
        .expect("collection entry should query")
        .expect("collection entry should exist");
    assert_eq!(details.status, CollectionStatus::Completed);
    assert_eq!(details.progress_episodes, Some(26));
    assert!(store
        .write_journal_entries("bangumi-user-1", Provider::Bangumi)
        .expect("write journal should query")
        .is_empty());

    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(read_transport.requests.len(), 2);
    assert!(read_transport.requests[0].headers.iter().any(|header| {
        header.name == "Authorization"
            && header.value == "Bearer valid-access-token-secret"
            && header.sensitive
    }));
    assert!(!format!("{:?}", read_transport.requests[0]).contains("valid-access-token-secret"));
    assert!(read_transport.requests[1]
        .request
        .url
        .contains("/v0/users/-/collections/253/episodes?episode_type=0"));
    assert_eq!(
        store
            .bangumi_episode_ids_for_done_prefix("bangumi-user-1", "253", 1)
            .expect("bangumi episode ids should query"),
        vec![1001]
    );
}

#[test]
fn stored_bangumi_anime_import_rolls_back_collection_when_episode_read_fails() {
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
    let mut secret_store = FakeAccessTokenStore::new("valid-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport {
        requests: Vec::new(),
        responses: vec![
            Ok(r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#.to_owned()),
            Err("episode read failed".to_owned()),
        ],
    };

    let error = import_collection_snapshot_pages_with_stored_access_token(
        &store,
        Provider::Bangumi,
        MediaKind::Anime,
        "bangumi-user-1",
        50,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect_err("episode read failure should fail the import");

    assert!(
        format!("{error:?}").contains("episode read failed"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        store
            .collection_entry_count("bangumi-user-1")
            .expect("collection count should load"),
        0
    );
    assert!(store
        .bangumi_episode_ids_for_done_prefix("bangumi-user-1", "253", 1)
        .expect("episode ids should query")
        .is_empty());
}

#[test]
fn stored_collection_import_persists_anilist_id_mal_identity_edges() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:anilist-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_735_000_000,
            refresh_token: ProviderRefreshTokenState::absent(),
            last_refresh_at_epoch_secs: Some(1_699_999_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    let mut secret_store = FakeAccessTokenStore::new("valid-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::ok(
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
    );

    let outcome = import_collection_snapshot_pages_with_stored_access_token(
        &store,
        Provider::AniList,
        MediaKind::Anime,
        "anilist-user-1",
        500,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("stored credential should fetch and import snapshot");

    assert_eq!(
        outcome,
        StoredProviderCollectionImportOutcome::Imported { imported: 1 }
    );
    let anilist_work_id = store
        .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    let mal_work_id = store
        .find_work_by_external_id(Provider::MyAnimeList, MediaKind::Anime, "5114")
        .expect("mal edge lookup should work")
        .expect("mal edge should exist");
    assert_eq!(anilist_work_id, mal_work_id);

    let anilist_details = store
        .collection_entry_details("anilist-user-1", Provider::AniList, MediaKind::Anime, "1")
        .expect("collection entry should query")
        .expect("collection entry should exist");
    assert_eq!(anilist_details.work_id, Some(anilist_work_id));
    let mal_edge = store
        .external_id_edge_details(Provider::MyAnimeList, MediaKind::Anime, "5114")
        .expect("edge details should query")
        .expect("edge details should exist");
    assert_eq!(mal_edge.confidence, 1000);
    assert_eq!(mal_edge.match_method, "anilist-id-mal");
    assert_eq!(mal_edge.source, "anilist idMal");
    assert!(store
        .write_journal_entries("anilist-user-1", Provider::AniList)
        .expect("write journal should query")
        .is_empty());
}

#[test]
fn stored_collection_import_refresh_required_does_not_read_or_import() {
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
    let mut secret_store = FakeAccessTokenStore::new("valid-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::ok("{}");

    let outcome = import_collection_snapshot_pages_with_stored_access_token(
        &store,
        Provider::Bangumi,
        MediaKind::Anime,
        "bangumi-user-1",
        50,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("refresh-required credential should be classified without reading");

    assert_eq!(
        outcome,
        StoredProviderCollectionImportOutcome::RefreshRequired
    );
    assert!(secret_store.lookups.is_empty());
    assert!(read_transport.requests.is_empty());
    assert_eq!(
        store
            .collection_entry_count("bangumi-user-1")
            .expect("collection count should load"),
        0
    );
}

#[test]
fn stored_collection_import_reauthorize_required_does_not_read_or_import() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let mut secret_store = FakeAccessTokenStore::new("valid-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::ok("{}");

    let outcome = import_collection_snapshot_pages_with_stored_access_token(
        &store,
        Provider::AniList,
        MediaKind::Anime,
        "anilist-user-1",
        50,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
    )
    .expect("missing credential should be classified without reading");

    assert_eq!(
        outcome,
        StoredProviderCollectionImportOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker
        }
    );
    assert!(secret_store.lookups.is_empty());
    assert!(read_transport.requests.is_empty());
    assert_eq!(
        store
            .collection_entry_count("anilist-user-1")
            .expect("collection count should load"),
        0
    );
    assert!(store
        .write_journal_entries("anilist-user-1", Provider::AniList)
        .expect("write journal should query")
        .is_empty());
}
