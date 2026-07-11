use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use sync_core::model::{CollectionStatus, MediaKind, Provider, SyncField};
use sync_core::provider::{
    parse_anilist_collection_fixture, parse_bangumi_collection_fixture,
    parse_bangumi_episode_collection_fixture, plan_provider_credential_action,
    ProviderAuthBootstrapMode, ProviderAuthFlow, ProviderCredentialAction,
    ProviderCredentialExchangeResult, ProviderCredentialState, ProviderRefreshTokenState,
};
use sync_core::store::{
    ExternalIdEdgeInput, ExternalIdEdgeUpsertInput, FieldObservationChangeOrigin,
    ProviderCredentialInput, ProviderCredentialRefreshAttemptFinalizeStatus,
    ProviderCredentialRefreshAttemptInput, ProviderCredentialRefreshAttemptStatus,
    ProviderItemInput, SqliteStore, StoreError, WriteJournalFieldIntent, WriteJournalIntent,
    WriteJournalIntentOutcome,
};

fn table_columns(path: &Path, table: &str) -> Vec<String> {
    let connection = Connection::open(path).expect("sqlite file should open for schema check");
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("table info statement should prepare");

    statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table info query should run")
        .collect::<Result<Vec<_>, _>>()
        .expect("table column rows should parse")
}

#[test]
fn migrations_create_required_tables_and_fts_indexes() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let schema_objects = store
        .schema_objects()
        .expect("schema object inspection should work");

    for table in [
        "provider_item",
        "identity_work",
        "external_id_edge",
        "collection_entry",
        "field_provenance",
        "sync_state",
        "write_journal",
        "write_journal_field",
        "collection_snapshot_state",
        "conflict",
        "manual_mapping",
        "provider_credential",
        "provider_credential_refresh_attempt",
        "bangumi_episode_collection",
        "provider_item_fts",
    ] {
        assert!(
            schema_objects.iter().any(|name| name == table),
            "missing schema object {table}"
        );
    }

    for index in [
        "idx_external_id_edge_lookup",
        "idx_external_id_edge_work",
        "idx_provider_item_lookup",
        "idx_provider_item_media_kind",
        "idx_collection_entry_work_kind",
        "idx_collection_entry_account_work",
        "idx_field_provenance_lookup",
        "idx_write_journal_provider_account_started",
        "idx_write_journal_field_journal",
        "idx_write_journal_field_target",
        "idx_conflict_work_field_resolved",
        "idx_provider_credential_provider_account",
        "idx_provider_credential_refresh_attempt_provider_account",
        "idx_bangumi_episode_collection_subject_sort",
    ] {
        assert!(
            schema_objects.iter().any(|name| name == index),
            "missing schema index {index}"
        );
    }
}

#[test]
fn bangumi_episode_collection_snapshot_persists_subject_episode_id_order() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let snapshot = parse_bangumi_episode_collection_fixture(
        "253",
        r#"{
            "data": [
                {
                    "episode": { "id": 1000, "type": 0, "sort": 0, "ep": 0 },
                    "type": 1,
                    "updated_at": 1699999999
                },
                {
                    "episode": { "id": 1002, "type": 0, "sort": 2, "ep": 2 },
                    "type": 2,
                    "updated_at": 1700000000
                },
                {
                    "episode": { "id": 1001, "type": 0, "sort": 1, "ep": 1 },
                    "type": 2,
                    "updated_at": 1700000001
                }
            ]
        }"#,
    )
    .expect("episode snapshot should parse");

    let imported = store
        .upsert_bangumi_episode_collection_snapshot("fixture-account", &snapshot)
        .expect("episode snapshot should import");

    assert_eq!(imported, 3);
    assert_eq!(
        store
            .bangumi_episode_ids_for_done_prefix("fixture-account", "253", 2)
            .expect("episode ids should query"),
        vec![1001, 1002]
    );
    assert_eq!(
        store
            .bangumi_episode_ids_for_progress_prefix("fixture-account", "253", 3)
            .expect("progress episode ids should query"),
        vec![1000, 1001, 1002]
    );

    let replacement = parse_bangumi_episode_collection_fixture(
        "253",
        r#"{
            "data": [
                {
                    "episode": { "id": 1001, "type": 0, "sort": 1, "ep": 1 },
                    "type": 2,
                    "updated_at": 1700000100
                }
            ]
        }"#,
    )
    .expect("replacement episode snapshot should parse");
    store
        .upsert_bangumi_episode_collection_snapshot("fixture-account", &replacement)
        .expect("replacement should import");

    assert_eq!(
        store
            .bangumi_episode_ids_for_done_prefix("fixture-account", "253", 2)
            .expect("episode ids should query after replacement"),
        vec![1001]
    );
}

#[test]
fn provider_credential_metadata_round_trips_without_secret_values() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "keychain:bangumi-sync/bangumi-user-1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_604_800,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_700_000_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    let credential = store
        .provider_credential(Provider::Bangumi, "bangumi-user-1")
        .expect("credential lookup should work")
        .expect("credential should exist");

    assert_eq!(credential.provider, Provider::Bangumi);
    assert_eq!(credential.account_id, "bangumi-user-1");
    assert_eq!(credential.auth_flow, ProviderAuthFlow::AuthorizationCode);
    assert_eq!(
        credential.bootstrap_mode,
        ProviderAuthBootstrapMode::AuthBroker
    );
    assert_eq!(
        credential.credential_store_ref,
        "keychain:bangumi-sync/bangumi-user-1"
    );
    assert_eq!(
        credential.credential_state(),
        ProviderCredentialState::available(
            Provider::Bangumi,
            1_700_604_800,
            ProviderRefreshTokenState::present_with_unknown_expiry()
        )
    );
    assert_eq!(
        plan_provider_credential_action(credential.credential_state(), 1_700_600_000),
        ProviderCredentialAction::RefreshWithProvider
    );
    assert!(
        !format!("{credential:?}").contains("access-token"),
        "debug output should not imply token material is present"
    );
    assert!(
        !format!("{credential:?}").contains("keychain:"),
        "debug output should not expose credential store refs"
    );
}

#[test]
fn provider_credential_state_lookup_omits_secret_reference_details() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "keychain:bangumi-sync/bangumi-user-1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_604_800,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_700_000_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    let credential_state = store
        .provider_credential_state(Provider::Bangumi, "bangumi-user-1")
        .expect("credential state lookup should work")
        .expect("credential state should exist");

    assert_eq!(
        credential_state,
        ProviderCredentialState::available(
            Provider::Bangumi,
            1_700_604_800,
            ProviderRefreshTokenState::present_with_unknown_expiry()
        )
    );
    assert!(
        !format!("{credential_state:?}").contains("keychain:"),
        "state-only lookup should not materialize credential store refs"
    );
}

#[test]
fn provider_credential_metadata_validates_provider_capabilities() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    let mal_error = store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "keychain:mal-user-1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_003_600,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect_err("MAL credentials must use PKCE auth flow");
    assert_eq!(
        mal_error,
        StoreError::InvalidProviderCredential {
            provider: Provider::MyAnimeList,
            reason: "auth flow is not supported by provider".to_owned(),
        }
    );

    let anilist_error = store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "keychain:anilist-user-1".to_owned(),
            access_token_expires_at_epoch_secs: 1_731_536_000,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect_err("AniList credentials must not persist refresh token metadata");
    assert_eq!(
        anilist_error,
        StoreError::InvalidProviderCredential {
            provider: Provider::AniList,
            reason: "refresh token state is not supported by provider".to_owned(),
        }
    );

    let bangumi_error = store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::ManualPin,
            credential_store_ref: "keychain:bangumi-user-1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_604_800,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect_err("Bangumi credentials must use a supported bootstrap mode");
    assert_eq!(
        bangumi_error,
        StoreError::InvalidProviderCredential {
            provider: Provider::Bangumi,
            reason: "bootstrap mode is not supported by provider".to_owned(),
        }
    );
}

#[test]
fn existing_sqlite_store_is_upgraded_with_provider_credential_table() {
    let path = std::env::temp_dir().join(format!(
        "bangumi-sync-old-store-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);

    {
        let connection = Connection::open(&path).expect("old sqlite file should create");
        connection
            .execute_batch(include_str!("../migrations/0001_initial.sql"))
            .expect("old initial migration should run");
        connection
            .execute_batch(include_str!("../migrations/0002_fts_indexes.sql"))
            .expect("old fts migration should run");
        connection
            .execute_batch(
                "INSERT INTO identity_work(id, media_kind, display_title)
                 VALUES (1, 'anime', 'legacy populated work');
                 INSERT INTO collection_entry(
                    id,
                    account_id,
                    work_id,
                    provider,
                    provider_entry_id,
                    media_kind,
                    status,
                    score_hundred,
                    provider_payload_hash
                 )
                 VALUES (
                    1,
                    'legacy-account',
                    1,
                    'anilist',
                    '1',
                    'anime',
                    'completed',
                    100,
                    'legacy-payload'
                 );
                 INSERT INTO field_provenance(
                    collection_entry_id,
                    field_name,
                    provider,
                    observed_value_hash,
                    source_reliability
                 )
                 VALUES
                    (1, 'status', 'anilist', 'legacy-status-hash', 'provider_snapshot'),
                    (1, 'score_hundred', 'anilist', 'legacy-score-hash', 'provider_snapshot');",
            )
            .expect("populated legacy rows should insert");
    }

    let store = SqliteStore::open(&path).expect("existing store should open and self-upgrade");
    let schema_objects = store
        .schema_objects()
        .expect("schema object inspection should work");
    assert!(schema_objects
        .iter()
        .any(|name| name == "provider_credential"));
    assert!(schema_objects
        .iter()
        .any(|name| name == "provider_credential_refresh_attempt"));
    assert!(schema_objects
        .iter()
        .any(|name| name == "write_journal_field"));
    let provider_item_columns = table_columns(&path, "provider_item");
    assert!(
        provider_item_columns
            .iter()
            .any(|column| column == "release_year"),
        "existing stores should be upgraded with provider_item.release_year"
    );
    let field_provenance_columns = table_columns(&path, "field_provenance");
    for column in [
        "previous_observed_value_hash",
        "observation_version",
        "last_changed_observation_version",
        "change_origin",
        "attributed_write_journal_field_id",
        "externally_cleared",
        "pending_external_change",
    ] {
        assert!(
            field_provenance_columns
                .iter()
                .any(|existing| existing == column),
            "existing stores should be upgraded with field_provenance.{column}"
        );
    }
    let write_journal_field_columns = table_columns(&path, "write_journal_field");
    for column in [
        "source_value_hash",
        "basis_snapshot_generation",
        "consumed_at",
    ] {
        assert!(
            write_journal_field_columns
                .iter()
                .any(|existing| existing == column),
            "existing stores should be upgraded with write_journal_field.{column}"
        );
    }
    let provenance_fields = store
        .field_provenance_fields(1)
        .expect("upgraded legacy provenance should load");
    assert_eq!(
        provenance_fields,
        vec![
            "progress_chapters".to_owned(),
            "progress_episodes".to_owned(),
            "progress_volumes".to_owned(),
            "score".to_owned(),
            "status".to_owned(),
        ]
    );
    let observations = store
        .field_observations(1)
        .expect("upgraded legacy observations should load");
    assert!(observations.values().all(|observation| {
        observation.observation_version == 1 && observation.last_changed_observation_version == 1
    }));

    let _ = std::fs::remove_file(path);
}

#[test]
fn existing_sqlite_store_rebuilds_provider_item_fts_search_text() {
    let path = std::env::temp_dir().join(format!(
        "bangumi-sync-old-fts-store-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);

    {
        let connection = Connection::open(&path).expect("old sqlite file should create");
        connection
            .execute_batch(include_str!("../migrations/0001_initial.sql"))
            .expect("old initial migration should run");
        connection
            .execute_batch(include_str!("../migrations/0002_fts_indexes.sql"))
            .expect("old fts migration should run");
        connection
            .execute(
                "INSERT INTO provider_item(
                    id,
                    provider,
                    media_kind,
                    external_id,
                    canonical_title,
                    format,
                    source_payload_hash
                 )
                 VALUES (1, 'bangumi', 'anime', 'fullwidth-spy-family', 'ＳＰＹ×ＦＡＭＩＬＹ', 'TV', 'sha256:old-fullwidth')",
                [],
            )
            .expect("old provider item should insert");
        connection
            .execute(
                "INSERT INTO provider_item_alias(provider_item_id, alias)
                 VALUES (1, 'スパイファミリー')",
                [],
            )
            .expect("old provider item alias should insert");
        connection
            .execute(
                "INSERT INTO provider_item_fts(
                    provider_item_id,
                    media_kind,
                    canonical_title,
                    aliases
                 )
                 VALUES (1, 'anime', 'ＳＰＹ×ＦＡＭＩＬＹ', 'スパイファミリー')",
                [],
            )
            .expect("old raw fts row should insert");
    }

    let store = SqliteStore::open(&path).expect("existing store should open and rebuild fts");
    let candidates = store
        .search_provider_items(MediaKind::Anime, "spy", 10)
        .expect("rebuilt title search should work");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "fullwidth-spy-family");
    assert_eq!(candidates[0].canonical_title, "ＳＰＹ×ＦＡＭＩＬＹ");

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn existing_sqlite_store_rebuilds_stale_provider_item_fts_search_text_version() {
    let path = std::env::temp_dir().join(format!(
        "bangumi-sync-stale-fts-store-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);

    {
        let store = SqliteStore::open(&path).expect("sqlite store should create");
        store
            .upsert_provider_item(ProviderItemInput {
                provider: Provider::Bangumi,
                media_kind: MediaKind::Anime,
                external_id: "mob-psycho-100-2".to_owned(),
                canonical_title: "Mob Psycho 100 Ⅱ".to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year: Some(2019),
                source_payload_hash: "sha256:mob-psycho-100-2".to_owned(),
            })
            .expect("provider item should insert");
    }
    {
        let connection = Connection::open(&path).expect("sqlite file should reopen");
        connection
            .execute("DELETE FROM provider_item_fts", [])
            .expect("fts rows should clear");
        connection
            .execute(
                "INSERT INTO provider_item_fts(
                    provider_item_id,
                    media_kind,
                    canonical_title,
                    aliases
                 )
                 VALUES (1, 'anime', 'Mob Psycho 100 Ⅱ', '')",
                [],
            )
            .expect("stale raw-only fts row should insert");
        connection
            .execute(
                "UPDATE store_metadata
                 SET value = '3'
                 WHERE key = 'provider_item_fts_search_text_version'",
                [],
            )
            .expect("search text marker should downgrade");
    }

    let store = SqliteStore::open(&path).expect("existing store should open and rebuild fts");
    let candidates = store
        .search_provider_items(MediaKind::Anime, "ii", 10)
        .expect("rebuilt title search should work");
    assert_eq!(candidates.len(), 1);

    drop(store);
    let connection = Connection::open(&path).expect("sqlite file should reopen");
    let search_text_version: String = connection
        .query_row(
            "SELECT value
             FROM store_metadata
             WHERE key = 'provider_item_fts_search_text_version'",
            [],
            |row| row.get(0),
        )
        .expect("search text marker should exist");
    assert_eq!(search_text_version, "4");

    let _ = std::fs::remove_file(path);
}

#[test]
fn read_only_sqlite_store_does_not_migrate_incomplete_schema() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-read-only-store-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let path = temp_dir.join("sync-v2-old-schema.sqlite3");
    let _ = std::fs::remove_file(&path);

    let connection = Connection::open(&path).expect("fixture db should create");
    connection
        .execute(
            "CREATE TABLE provider_item(id INTEGER PRIMARY KEY, provider TEXT NOT NULL)",
            [],
        )
        .expect("fixture schema should create");
    drop(connection);

    let store = SqliteStore::open_read_only(&path).expect("read-only store should open");
    let credential_error = store
        .provider_credential_state(Provider::Bangumi, "bangumi-user-1")
        .expect_err("missing provider_credential table should be reported as read error");
    assert!(
        matches!(credential_error, StoreError::Sqlite { ref message } if message.contains("no such table")),
        "unexpected error: {credential_error:?}"
    );
    drop(store);

    let connection = Connection::open(&path).expect("fixture db should reopen");
    let migrated: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM sqlite_schema
                WHERE name = 'provider_credential'
            )",
            [],
            |row| row.get(0),
        )
        .expect("schema check should work");
    assert!(
        !migrated,
        "read-only open must not create credential schema"
    );
    let columns = table_columns(&path, "provider_item");
    assert!(
        !columns.iter().any(|column| column == "release_year"),
        "read-only open must not add provider_item.release_year"
    );
}

#[test]
fn existing_read_write_sqlite_store_does_not_migrate_incomplete_schema() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-existing-read-write-store-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let path = temp_dir.join("sync-v2-old-schema-read-write.sqlite3");
    let _ = std::fs::remove_file(&path);

    let connection = Connection::open(&path).expect("fixture db should create");
    connection
        .execute(
            "CREATE TABLE provider_item(id INTEGER PRIMARY KEY, provider TEXT NOT NULL)",
            [],
        )
        .expect("fixture schema should create");
    drop(connection);

    let store = SqliteStore::open_existing_read_write(&path)
        .expect("existing read-write store should open");
    let credential_error = store
        .provider_credential_state(Provider::Bangumi, "bangumi-user-1")
        .expect_err("missing provider_credential table should be reported as read error");
    assert!(
        matches!(credential_error, StoreError::Sqlite { ref message } if message.contains("no such table")),
        "unexpected error: {credential_error:?}"
    );
    drop(store);

    let connection = Connection::open(&path).expect("fixture db should reopen");
    let migrated: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM sqlite_schema
                WHERE name = 'provider_credential'
            )",
            [],
            |row| row.get(0),
        )
        .expect("schema check should work");
    assert!(
        !migrated,
        "existing read-write open must not create credential schema"
    );
    let columns = table_columns(&path, "provider_item");
    assert!(
        !columns.iter().any(|column| column == "release_year"),
        "existing read-write open must not add provider_item.release_year"
    );
}

#[test]
fn provider_credential_metadata_updates_refresh_state_in_place() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCodePkcePlain,
            bootstrap_mode: ProviderAuthBootstrapMode::LocalCallback,
            credential_store_ref: "sqlite-secret-ref:mal-user-1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_003_600,
            refresh_token: ProviderRefreshTokenState::present_expires_at(1_702_592_000),
            last_refresh_at_epoch_secs: Some(1_700_000_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCodePkcePlain,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "sqlite-secret-ref:mal-user-1-rotated".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_007_200,
            refresh_token: ProviderRefreshTokenState::absent(),
            last_refresh_at_epoch_secs: Some(1_700_003_600),
            last_reauth_request_at_epoch_secs: Some(1_700_003_500),
        })
        .expect("credential metadata should update");

    let credential = store
        .provider_credential(Provider::MyAnimeList, "mal-user-1")
        .expect("credential lookup should work")
        .expect("credential should exist");

    assert_eq!(
        credential.bootstrap_mode,
        ProviderAuthBootstrapMode::AuthBroker
    );
    assert_eq!(
        credential.credential_store_ref,
        "sqlite-secret-ref:mal-user-1-rotated"
    );
    assert_eq!(
        credential.credential_state(),
        ProviderCredentialState::available(
            Provider::MyAnimeList,
            1_700_007_200,
            ProviderRefreshTokenState::absent()
        )
    );
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_003_600));
    assert_eq!(
        credential.last_reauth_request_at_epoch_secs,
        Some(1_700_003_500)
    );
}

#[test]
fn external_id_lookup_is_scoped_by_provider_and_media_kind() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let work_id = store
        .create_identity_work(MediaKind::Anime, "Cowboy Bebop")
        .expect("identity work should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "direct".to_owned(),
            dataset_version: Some("test-v1".to_owned()),
        })
        .expect("edge should insert");

    assert_eq!(
        store
            .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
            .expect("lookup should work"),
        Some(work_id)
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::AniList, MediaKind::Manga, "1")
            .expect("lookup should work"),
        None
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "1")
            .expect("lookup should work"),
        None
    );
}

#[test]
fn external_id_edge_upsert_preserves_stronger_manual_edge_metadata() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let work_id = store
        .create_identity_work(MediaKind::Anime, "Cowboy Bebop")
        .expect("identity work should insert");
    let auto_match_work_id = store
        .create_identity_work(MediaKind::Anime, "Wrong Cowboy Bebop candidate")
        .expect("identity work should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "legacy-manual".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("manual-v1".to_owned()),
        })
        .expect("manual edge should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: auto_match_work_id,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "auto-match".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
            dataset_version: Some("auto-v1".to_owned()),
        })
        .expect("weaker edge metadata should be ignored");

    let edge = store
        .external_id_edge_details(Provider::AniList, MediaKind::Anime, "1")
        .expect("edge lookup should work")
        .expect("edge should exist");
    assert_eq!(edge.work_id, work_id);
    assert_eq!(edge.confidence, 1000);
    assert_eq!(edge.source, "legacy-manual");
    assert_eq!(edge.match_method, "manual");
    assert_eq!(edge.dataset_version.as_deref(), Some("manual-v1"));
}

#[test]
fn external_id_edge_upsert_preserves_stronger_crosswalk_edge_metadata() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let crosswalk_work_id = store
        .create_identity_work(MediaKind::Anime, "Cowboy Bebop")
        .expect("identity work should insert");
    let auto_match_work_id = store
        .create_identity_work(MediaKind::Anime, "Wrong Cowboy Bebop candidate")
        .expect("identity work should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: crosswalk_work_id,
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Anime,
            external_id: "5".to_owned(),
            source: "anilist idMal".to_owned(),
            confidence: 1000,
            match_method: "anilist-id-mal".to_owned(),
            dataset_version: Some("crosswalk-v1".to_owned()),
        })
        .expect("crosswalk edge should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: auto_match_work_id,
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Anime,
            external_id: "5".to_owned(),
            source: "auto-match".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
            dataset_version: Some("auto-v1".to_owned()),
        })
        .expect("weaker edge metadata should be ignored");

    let edge = store
        .external_id_edge_details(Provider::MyAnimeList, MediaKind::Anime, "5")
        .expect("edge lookup should work")
        .expect("edge should exist");
    assert_eq!(edge.work_id, crosswalk_work_id);
    assert_eq!(edge.confidence, 1000);
    assert_eq!(edge.source, "anilist idMal");
    assert_eq!(edge.match_method, "anilist-id-mal");
    assert_eq!(edge.dataset_version.as_deref(), Some("crosswalk-v1"));
}

#[test]
fn external_id_edge_upsert_preserves_higher_confidence_direct_edge_metadata() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let direct_work_id = store
        .create_identity_work(MediaKind::Anime, "Cowboy Bebop")
        .expect("identity work should insert");
    let auto_match_work_id = store
        .create_identity_work(MediaKind::Anime, "Wrong Cowboy Bebop candidate")
        .expect("identity work should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: direct_work_id,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "direct".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("direct edge should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: auto_match_work_id,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "auto-match".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
            dataset_version: Some("auto-v1".to_owned()),
        })
        .expect("weaker edge metadata should be ignored");

    let edge = store
        .external_id_edge_details(Provider::AniList, MediaKind::Anime, "1")
        .expect("edge lookup should work")
        .expect("edge should exist");
    assert_eq!(edge.work_id, direct_work_id);
    assert_eq!(edge.confidence, 1000);
    assert_eq!(edge.source, "fixture");
    assert_eq!(edge.match_method, "direct");
    assert_eq!(edge.dataset_version.as_deref(), Some("fixture-v1"));
}

#[test]
fn atomic_external_id_edge_upsert_rolls_back_created_work_on_edge_error() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    let error = store
        .upsert_external_id_edges_atomically(
            None,
            MediaKind::Anime,
            "Cowboy Bebop",
            &[
                ExternalIdEdgeUpsertInput {
                    provider: Provider::Bangumi,
                    media_kind: MediaKind::Anime,
                    external_id: "253".to_owned(),
                    source: "auto-match".to_owned(),
                    confidence: 900,
                    match_method: "fts-title-exact".to_owned(),
                    dataset_version: None,
                },
                ExternalIdEdgeUpsertInput {
                    provider: Provider::AniList,
                    media_kind: MediaKind::Anime,
                    external_id: "1".to_owned(),
                    source: "auto-match".to_owned(),
                    confidence: 1001,
                    match_method: "invalid".to_owned(),
                    dataset_version: None,
                },
            ],
        )
        .expect_err("invalid edge should roll back the batch");

    assert!(matches!(
        error,
        StoreError::InvalidConfidence { value: 1001 }
    ));
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        0,
        "failed atomic edge batch should roll back the created work"
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "253")
            .expect("lookup should work"),
        None
    );
}

#[test]
fn atomic_external_id_edge_upsert_rolls_back_created_work_when_edges_are_rejected() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let existing_work_id = store
        .create_identity_work(MediaKind::Anime, "Cowboy Bebop")
        .expect("identity work should insert");
    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: existing_work_id,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "direct".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("direct edge should insert");

    let error = store
        .upsert_external_id_edges_atomically(
            None,
            MediaKind::Anime,
            "Wrong Cowboy Bebop candidate",
            &[ExternalIdEdgeUpsertInput {
                provider: Provider::AniList,
                media_kind: MediaKind::Anime,
                external_id: "1".to_owned(),
                source: "auto-match".to_owned(),
                confidence: 900,
                match_method: "fts-title-exact".to_owned(),
                dataset_version: Some("auto-v1".to_owned()),
            }],
        )
        .expect_err("rejected edge batch should roll back the created work");

    assert!(matches!(
        error,
        StoreError::ExternalIdEdgeUpsertRejected {
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            ref external_id
        } if external_id == "1"
    ));
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        1,
        "failed atomic edge batch should roll back the created work"
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
            .expect("lookup should work"),
        Some(existing_work_id)
    );
}

#[test]
fn provider_credential_metadata_persists_exchange_result_without_token_values() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    store
        .upsert_provider_credential_exchange(ProviderCredentialExchangeResult {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCodePkcePlain,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:mal-user-1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_003_600,
            refresh_token: ProviderRefreshTokenState::present_expires_at(1_702_592_000),
            last_refresh_at_epoch_secs: None,
        })
        .expect("exchange metadata should persist");

    let credential = store
        .provider_credential(Provider::MyAnimeList, "mal-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");

    assert_eq!(credential.provider, Provider::MyAnimeList);
    assert_eq!(
        credential.auth_flow,
        ProviderAuthFlow::AuthorizationCodePkcePlain
    );
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
    assert_eq!(credential.last_refresh_at_epoch_secs, None);
    let debug = format!("{credential:?}");
    assert!(!debug.contains("access-token"));
    assert!(!debug.contains("refresh-token"));
}

#[test]
fn provider_credential_metadata_persists_refresh_timestamp_from_exchange_result() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    store
        .upsert_provider_credential_exchange(ProviderCredentialExchangeResult {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v2".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_604_800,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_700_000_000),
        })
        .expect("refresh metadata should persist");

    let credential = store
        .provider_credential(Provider::Bangumi, "bangumi-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");

    assert_eq!(
        credential.credential_store_ref,
        "secret-store:bangumi-user-1:v2"
    );
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_604_800);
    assert_eq!(
        credential.refresh_token,
        ProviderRefreshTokenState::present_with_unknown_expiry()
    );
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));
    assert_eq!(credential.last_reauth_request_at_epoch_secs, None);
}

#[test]
fn provider_credential_refresh_attempt_finalizes_with_compare_and_swap() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    store
        .upsert_provider_credential(ProviderCredentialInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("initial credential metadata should insert");

    let attempt_id = store
        .begin_provider_credential_refresh_attempt(ProviderCredentialRefreshAttemptInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            previous_credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            started_at_epoch_secs: 1_700_000_000,
        })
        .expect("refresh attempt should start");
    let status = store
        .finalize_provider_credential_refresh_attempt(
            attempt_id,
            ProviderCredentialExchangeResult {
                provider: Provider::Bangumi,
                account_id: "bangumi-user-1".to_owned(),
                auth_flow: ProviderAuthFlow::AuthorizationCode,
                bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
                credential_store_ref: "secret-store:bangumi-user-1:v2".to_owned(),
                access_token_expires_at_epoch_secs: 1_700_003_600,
                refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
                last_refresh_at_epoch_secs: Some(1_700_000_000),
            },
            1_700_000_001,
        )
        .expect("matching secret ref should commit");
    assert_eq!(
        status,
        ProviderCredentialRefreshAttemptFinalizeStatus::Committed
    );

    let committed_attempt = store
        .provider_credential_refresh_attempt(attempt_id)
        .expect("attempt lookup should work")
        .expect("attempt should exist");
    assert_eq!(
        committed_attempt.status,
        ProviderCredentialRefreshAttemptStatus::Committed
    );
    assert_eq!(
        committed_attempt.attempted_credential_store_ref.as_deref(),
        Some("secret-store:bangumi-user-1:v2")
    );

    let stale_attempt_id = store
        .begin_provider_credential_refresh_attempt(ProviderCredentialRefreshAttemptInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            previous_credential_store_ref: "secret-store:bangumi-user-1:v1".to_owned(),
            started_at_epoch_secs: 1_700_000_100,
        })
        .expect("stale refresh attempt should start");
    let stale_status = store
        .finalize_provider_credential_refresh_attempt(
            stale_attempt_id,
            ProviderCredentialExchangeResult {
                provider: Provider::Bangumi,
                account_id: "bangumi-user-1".to_owned(),
                auth_flow: ProviderAuthFlow::AuthorizationCode,
                bootstrap_mode: ProviderAuthBootstrapMode::AuthBroker,
                credential_store_ref: "secret-store:bangumi-user-1:v3".to_owned(),
                access_token_expires_at_epoch_secs: 1_700_007_200,
                refresh_token: ProviderRefreshTokenState::present_with_unknown_expiry(),
                last_refresh_at_epoch_secs: Some(1_700_000_100),
            },
            1_700_000_101,
        )
        .expect("stale secret ref should be recorded as a conflict");
    assert_eq!(
        stale_status,
        ProviderCredentialRefreshAttemptFinalizeStatus::Conflict
    );

    let credential = store
        .provider_credential(Provider::Bangumi, "bangumi-user-1")
        .expect("credential lookup should work")
        .expect("credential should exist");
    assert_eq!(
        credential.credential_store_ref,
        "secret-store:bangumi-user-1:v2"
    );
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_003_600);
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));

    let stale_attempt = store
        .provider_credential_refresh_attempt(stale_attempt_id)
        .expect("attempt lookup should work")
        .expect("attempt should exist");
    assert_eq!(
        stale_attempt.status,
        ProviderCredentialRefreshAttemptStatus::Failed
    );
    assert_eq!(
        stale_attempt.failure_kind.as_deref(),
        Some("credential_store_ref_conflict")
    );
    assert_eq!(
        stale_attempt.attempted_credential_store_ref.as_deref(),
        Some("secret-store:bangumi-user-1:v3")
    );
    let debug = format!("{stale_attempt:?}");
    assert!(!debug.contains("secret-store:bangumi-user-1:v1"));
    assert!(!debug.contains("secret-store:bangumi-user-1:v3"));
    assert!(!debug.contains("access-token"));
    assert!(!debug.contains("refresh-token"));
}

#[test]
fn provider_item_import_persists_payload_hash_and_searches_titles() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "975".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            aliases: vec!["カウボーイビバップ".to_owned(), "Kauboi Bibappu".to_owned()],
            format: Some("TV".to_owned()),
            release_year: Some(1998),
            source_payload_hash: "sha256:bangumi-975-v1".to_owned(),
        })
        .expect("provider item should insert");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Manga,
            external_id: "book-1".to_owned(),
            canonical_title: "Cowboy Bebop Shooting Star".to_owned(),
            aliases: Vec::new(),
            format: Some("MANGA".to_owned()),
            release_year: Some(1998),
            source_payload_hash: "sha256:book-1-v1".to_owned(),
        })
        .expect("provider item should insert");

    let anime_hash = store
        .provider_item_payload_hash(Provider::Bangumi, MediaKind::Anime, "975")
        .expect("hash lookup should work");
    assert_eq!(anime_hash.as_deref(), Some("sha256:bangumi-975-v1"));

    let candidates = store
        .search_provider_items(MediaKind::Anime, "bebop", 10)
        .expect("title search should work");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].provider, Provider::Bangumi);
    assert_eq!(candidates[0].external_id, "975");
    assert_eq!(candidates[0].canonical_title, "Cowboy Bebop");
    assert_eq!(candidates[0].release_year, Some(1998));

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "975".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            aliases: vec!["カウボーイビバップ".to_owned()],
            format: Some("TV".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:bangumi-975-v2".to_owned(),
        })
        .expect("provider item should update and clear release year");
    let updated_candidates = store
        .search_provider_items(MediaKind::Anime, "bebop", 10)
        .expect("title search should work after update");
    assert_eq!(updated_candidates.len(), 1);
    assert_eq!(updated_candidates[0].release_year, None);
}

#[test]
fn provider_item_search_indexes_full_width_ascii_folded_title_text() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "fullwidth-spy-family".to_owned(),
            canonical_title: "ＳＰＹ×ＦＡＭＩＬＹ".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: Some(2022),
            source_payload_hash: "sha256:fullwidth-spy-family".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = store
        .search_provider_items(MediaKind::Anime, "spy", 10)
        .expect("title search should work");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].provider, Provider::Bangumi);
    assert_eq!(candidates[0].external_id, "fullwidth-spy-family");
    assert_eq!(candidates[0].canonical_title, "ＳＰＹ×ＦＡＭＩＬＹ");
}

#[test]
fn provider_item_search_returns_raw_aliases_after_indexing_folded_text() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "fullwidth-spy-family-alias".to_owned(),
            canonical_title: "スパイファミリー".to_owned(),
            aliases: vec!["ＳＰＹ×ＦＡＭＩＬＹ".to_owned()],
            format: Some("TV".to_owned()),
            release_year: Some(2022),
            source_payload_hash: "sha256:fullwidth-spy-family-alias".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = store
        .search_provider_items(MediaKind::Anime, "spy", 10)
        .expect("alias search should work");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "fullwidth-spy-family-alias");
    assert_eq!(candidates[0].canonical_title, "スパイファミリー");
    assert_eq!(
        candidates[0].aliases,
        vec!["ＳＰＹ×ＦＡＭＩＬＹ".to_owned()]
    );
}

#[test]
fn collection_snapshot_import_persists_entries_and_is_idempotent() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let work_id = store
        .create_identity_work(MediaKind::Anime, "Cowboy Bebop")
        .expect("identity work should insert");
    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id,
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "253".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: None,
        })
        .expect("identity edge should insert");

    let fixture = r#"{
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
    }"#;
    let snapshot = parse_bangumi_collection_fixture(fixture, MediaKind::Anime)
        .expect("fixture should normalize");

    let imported = store
        .upsert_collection_snapshot("fixture-account", &snapshot)
        .expect("snapshot should import");
    assert_eq!(imported, 1);
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("count should work"),
        1
    );

    let entry = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("lookup should work")
        .expect("entry should exist");
    assert_eq!(entry.work_id, Some(work_id));
    assert_eq!(entry.status, CollectionStatus::Completed);
    assert_eq!(entry.score_hundred, Some(100));
    assert_eq!(entry.progress_episodes, Some(26));
    assert_eq!(entry.progress_chapters, None);
    assert_eq!(entry.progress_volumes, None);
    assert_eq!(entry.provider_payload_hash, snapshot.raw_payload_hash());
    assert_eq!(entry.provider_updated_at_epoch_secs, None);

    let provenance_fields = store
        .field_provenance_fields(entry.id)
        .expect("provenance lookup should work");
    assert_eq!(
        provenance_fields,
        vec![
            "progress_chapters".to_owned(),
            "progress_episodes".to_owned(),
            "progress_volumes".to_owned(),
            "score".to_owned(),
            "status".to_owned()
        ]
    );
    let initial_observations = store
        .field_observations(entry.id)
        .expect("field observation lookup should work");
    assert_eq!(initial_observations.len(), 5);
    for field in [
        SyncField::Status,
        SyncField::Score,
        SyncField::ProgressEpisodes,
        SyncField::ProgressChapters,
        SyncField::ProgressVolumes,
    ] {
        let observation = initial_observations
            .get(&field)
            .expect("default-writable field should have initial provenance");
        assert_eq!(observation.observation_version, 1);
        assert_eq!(observation.previous_observed_value_hash, None);
        assert_eq!(
            observation.change_origin,
            FieldObservationChangeOrigin::Initial
        );
    }

    let updated_fixture = r#"{
        "data": [
            {
                "subject_id": 253,
                "subject_type": "anime",
                "collection_type": "do",
                "rate": 9,
                "ep_status": 27,
                "vol_status": 0
            }
        ]
    }"#;
    let updated_snapshot = parse_bangumi_collection_fixture(updated_fixture, MediaKind::Anime)
        .expect("fixture should normalize");

    let imported = store
        .upsert_collection_snapshot("fixture-account", &updated_snapshot)
        .expect("snapshot should update");
    assert_eq!(imported, 1);
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("count should work"),
        1,
        "same account/provider/kind/id should update in place"
    );

    let updated = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("lookup should work")
        .expect("entry should exist");
    assert_eq!(updated.id, entry.id);
    assert_eq!(updated.work_id, Some(work_id));
    assert_eq!(updated.status, CollectionStatus::InProgress);
    assert_eq!(updated.score_hundred, Some(90));
    assert_eq!(updated.progress_episodes, Some(27));
    assert_eq!(
        updated.provider_payload_hash,
        updated_snapshot.raw_payload_hash()
    );
    let updated_observations = store
        .field_observations(updated.id)
        .expect("updated field observations should load");
    for field in [
        SyncField::Status,
        SyncField::Score,
        SyncField::ProgressEpisodes,
    ] {
        let observation = updated_observations
            .get(&field)
            .expect("changed field should have provenance");
        assert_eq!(observation.observation_version, 2);
        assert!(observation.previous_observed_value_hash.is_some());
        assert_eq!(
            observation.change_origin,
            FieldObservationChangeOrigin::External
        );
    }
    for field in [SyncField::ProgressChapters, SyncField::ProgressVolumes] {
        let observation = updated_observations
            .get(&field)
            .expect("unchanged field should have provenance");
        assert_eq!(observation.observation_version, 2);
        assert_eq!(
            observation.previous_observed_value_hash.as_deref(),
            Some(observation.observed_value_hash.as_str())
        );
        assert_eq!(
            observation.change_origin,
            FieldObservationChangeOrigin::Unchanged
        );
    }
}

#[test]
fn collection_snapshot_provenance_tracks_optional_field_clear() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let initial = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("initial fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &initial)
        .expect("initial snapshot should import");
    let entry = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("entry lookup should work")
        .expect("entry should exist");
    let initial_score = store
        .field_observations(entry.id)
        .expect("initial observations should load")
        .remove(&SyncField::Score)
        .expect("initial score observation should exist");

    let cleared = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":0,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("cleared fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &cleared)
        .expect("cleared snapshot should import");
    let cleared_score = store
        .field_observations(entry.id)
        .expect("cleared observations should load")
        .remove(&SyncField::Score)
        .expect("cleared score observation should exist");
    assert_eq!(cleared_score.observation_version, 2);
    assert_eq!(
        cleared_score.previous_observed_value_hash,
        Some(initial_score.observed_value_hash)
    );
    assert_ne!(
        cleared_score.previous_observed_value_hash.as_deref(),
        Some(cleared_score.observed_value_hash.as_str())
    );
    assert_eq!(
        cleared_score.change_origin,
        FieldObservationChangeOrigin::External
    );
    assert!(cleared_score.externally_cleared);

    store
        .upsert_collection_snapshot("fixture-account", &cleared)
        .expect("unchanged cleared snapshot should import");
    let unchanged_score = store
        .field_observations(entry.id)
        .expect("unchanged observations should load")
        .remove(&SyncField::Score)
        .expect("unchanged score observation should exist");
    assert_eq!(unchanged_score.observation_version, 3);
    assert_eq!(
        unchanged_score.previous_observed_value_hash.as_deref(),
        Some(unchanged_score.observed_value_hash.as_str())
    );
    assert_eq!(
        unchanged_score.change_origin,
        FieldObservationChangeOrigin::Unchanged
    );
    assert!(unchanged_score.externally_cleared);
}

#[test]
fn attempted_write_journal_requires_reconciliation_before_retry() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let work_id = store
        .create_identity_work(MediaKind::Anime, "journal fixture")
        .expect("work should create");
    for (provider, external_id) in [(Provider::Bangumi, "253"), (Provider::AniList, "1")] {
        store
            .upsert_external_id_edge(ExternalIdEdgeInput {
                work_id,
                provider,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                source: "fixture".to_owned(),
                confidence: 1000,
                match_method: "manual-test".to_owned(),
                dataset_version: None,
            })
            .expect("identity edge should insert");
    }
    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":70,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    let source_entry = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("source lookup should work")
        .expect("source should exist");
    let target_entry = store
        .collection_entry_details("fixture-account", Provider::AniList, MediaKind::Anime, "1")
        .expect("target lookup should work")
        .expect("target should exist");
    let source_hash = store
        .field_observations(source_entry.id)
        .expect("source observations should load")
        .remove(&SyncField::Score)
        .expect("source score should exist")
        .observed_value_hash;
    let target_hash = store
        .field_observations(target_entry.id)
        .expect("target observations should load")
        .remove(&SyncField::Score)
        .expect("target score should exist")
        .observed_value_hash;
    let intent = WriteJournalIntent {
        provider: Provider::AniList,
        account_id: "fixture-account".to_owned(),
        operation_id: "ambiguous-attempt-fixture".to_owned(),
        request_body: r#"{"score":100}"#.to_owned(),
        work_id,
        source_provider: Provider::Bangumi,
        media_kind: MediaKind::Anime,
        target_provider_entry_id: "1".to_owned(),
        fields: vec![WriteJournalFieldIntent {
            field: SyncField::Score,
            before_value_hash: target_hash,
            source_value_hash: source_hash.clone(),
            expected_value_hash: source_hash,
        }],
    };

    assert_eq!(
        store
            .record_write_journal_intent(intent.clone())
            .expect("first intent should record"),
        WriteJournalIntentOutcome::Recorded
    );
    assert_eq!(
        store
            .record_write_journal_intent(intent)
            .expect_err("ambiguous attempted write must not be resent"),
        StoreError::AmbiguousWriteJournalIntent {
            provider: Provider::AniList,
            target_provider_entry_id: "1".to_owned(),
        }
    );
}

#[test]
fn collection_entry_work_ids_can_be_backfilled_after_identity_edges_are_created() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let snapshot = parse_bangumi_collection_fixture(
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
        MediaKind::Anime,
    )
    .expect("fixture should normalize");

    store
        .upsert_collection_snapshot("fixture-account", &snapshot)
        .expect("snapshot should import without a pre-existing identity edge");
    let entry = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("lookup should work")
        .expect("entry should exist");
    assert_eq!(entry.work_id, None);

    let work_id = store
        .create_identity_work(MediaKind::Anime, "Cowboy Bebop")
        .expect("identity work should insert");
    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id,
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "253".to_owned(),
            source: "auto-match".to_owned(),
            confidence: 1000,
            match_method: "title-exact".to_owned(),
            dataset_version: None,
        })
        .expect("identity edge should insert");

    let updated = store
        .backfill_collection_entry_work_ids(
            "fixture-account",
            MediaKind::Anime,
            &[Provider::Bangumi],
        )
        .expect("collection entry work ids should backfill");
    assert_eq!(updated, 1);

    let backfilled = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("lookup should work")
        .expect("entry should exist");
    assert_eq!(backfilled.work_id, Some(work_id));
}

#[test]
fn collection_snapshot_import_removes_entries_missing_from_latest_provider_snapshot() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let initial_snapshot = parse_bangumi_collection_fixture(
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
                    "subject_id": 999,
                    "subject_type": "anime",
                    "collection_type": "wish",
                    "rate": 0,
                    "ep_status": 0,
                    "vol_status": 0
                }
            ]
        }"#,
        MediaKind::Anime,
    )
    .expect("fixture should normalize");
    store
        .upsert_collection_snapshot("fixture-account", &initial_snapshot)
        .expect("initial snapshot should import");
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("count should work"),
        2
    );

    let latest_snapshot = parse_bangumi_collection_fixture(
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
        MediaKind::Anime,
    )
    .expect("fixture should normalize");
    store
        .upsert_collection_snapshot("fixture-account", &latest_snapshot)
        .expect("latest snapshot should replace stale provider rows");

    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("count should work"),
        1
    );
    assert!(store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "999",
        )
        .expect("lookup should work")
        .is_none());
    assert!(store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("lookup should work")
        .is_some());
}

#[test]
fn collection_snapshot_import_prunes_only_matching_account_provider_and_kind() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let bangumi_anime = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":999,"subject_type":"anime","collection_type":"wish","rate":0,"ep_status":0,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi anime fixture should normalize");
    let bangumi_manga = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"do","rate":8,"ep_status":12,"vol_status":3}]}"#,
        MediaKind::Manga,
    )
    .expect("bangumi manga fixture should normalize");
    let anilist_anime = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist anime fixture should normalize");

    store
        .upsert_collection_snapshot("fixture-account", &bangumi_anime)
        .expect("fixture account bangumi anime should import");
    store
        .upsert_collection_snapshot("other-account", &bangumi_anime)
        .expect("other account bangumi anime should import");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi_manga)
        .expect("fixture account bangumi manga should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist_anime)
        .expect("fixture account anilist anime should import");

    let latest_bangumi_anime = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("latest bangumi anime fixture should normalize");
    store
        .upsert_collection_snapshot("fixture-account", &latest_bangumi_anime)
        .expect("latest fixture account bangumi anime should import");

    assert!(store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "999",
        )
        .expect("lookup should work")
        .is_none());
    assert!(store
        .collection_entry_details("other-account", Provider::Bangumi, MediaKind::Anime, "999",)
        .expect("lookup should work")
        .is_some());
    assert!(store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Manga,
            "9001",
        )
        .expect("lookup should work")
        .is_some());
    assert!(store
        .collection_entry_details("fixture-account", Provider::AniList, MediaKind::Anime, "1",)
        .expect("lookup should work")
        .is_some());
}

#[test]
fn collection_snapshot_import_clears_stale_provider_timestamp() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");
    let fixture_with_timestamp = r#"{
        "data": [
            {
                "subject_id": 253,
                "subject_type": "anime",
                "collection_type": "collect",
                "rate": 10,
                "ep_status": 26,
                "vol_status": 0,
                "updated_at": "2026-06-01T00:00:00Z"
            }
        ]
    }"#;
    let snapshot_with_timestamp =
        parse_bangumi_collection_fixture(fixture_with_timestamp, MediaKind::Anime)
            .expect("fixture should normalize");

    store
        .upsert_collection_snapshot("fixture-account", &snapshot_with_timestamp)
        .expect("snapshot should import");

    let entry = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("lookup should work")
        .expect("entry should exist");
    assert_eq!(entry.provider_updated_at_epoch_secs, Some(1_780_272_000));

    let fixture_without_timestamp = r#"{
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
    }"#;
    let snapshot_without_timestamp =
        parse_bangumi_collection_fixture(fixture_without_timestamp, MediaKind::Anime)
            .expect("fixture should normalize");

    store
        .upsert_collection_snapshot("fixture-account", &snapshot_without_timestamp)
        .expect("snapshot should update");

    let updated = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("lookup should work")
        .expect("entry should exist");
    assert_eq!(updated.id, entry.id);
    assert_eq!(updated.provider_updated_at_epoch_secs, None);
}

#[test]
fn invalid_edge_confidence_is_rejected_before_sqlite_write() {
    let store = SqliteStore::open_in_memory().expect("in-memory store should open");

    let error = store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: 1,
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Manga,
            external_id: "42".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1001,
            match_method: "direct".to_owned(),
            dataset_version: None,
        })
        .expect_err("invalid confidence should be rejected");

    assert_eq!(error, StoreError::InvalidConfidence { value: 1001 });
}
