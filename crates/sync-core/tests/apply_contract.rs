use std::collections::VecDeque;
use sync_core::identity::import_legacy_manual_relations;

use sync_core::model::{CollectionStatus, MediaKind, Provider, SyncField};
use sync_core::provider::{
    parse_anilist_collection_fixture, parse_bangumi_collection_fixture,
    parse_myanimelist_collection_fixture, provider_credential_capability,
    AuthorizedProviderReadRequest, AuthorizedProviderReadTransport, AuthorizedProviderWriteRequest,
    AuthorizedProviderWriteTransport, ProviderAccessToken, ProviderAuthBootstrapMode,
    ProviderAuthError, ProviderCredentialAccessTokenStore, ProviderCredentialSecretLookup,
    ProviderRefreshTokenState, ProviderWriteRequest, ProviderWriteRequestMethod,
};
use sync_core::store::{
    ExternalIdEdgeInput, ProviderCredentialInput, SqliteStore, WriteJournalStatus,
};
use sync_core::sync::{
    apply_plan_with_writer, apply_plan_with_writer_and_deferred_verifier,
    apply_plan_with_writer_and_verifier, plan_dry_run, ApplyError, PlannedAction,
    ProviderPostWriteEntry, ProviderPostWriteState, ProviderPostWriteVerifier,
    ProviderRequestWriter, ProviderWriteResult, ProviderWriter,
    StoredAccessTokenProviderPostWriteVerifier, StoredAccessTokenProviderWriter,
};

#[test]
fn mock_apply_records_successful_write_journal_entry() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    assert_eq!(plan.actions().len(), 1);

    let mut writer = RecordingWriter::default();
    let summary = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect("mock apply should succeed");

    assert_eq!(summary.attempted, 1);
    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(
        writer.calls,
        vec![(
            "fixture-account".to_owned(),
            Provider::AniList,
            "1".to_owned()
        )]
    );

    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].provider, Provider::AniList);
    assert_eq!(journals[0].account_id, "fixture-account");
    assert!(journals[0].operation_id.contains("work-"));
    assert!(journals[0].request_hash.starts_with("fnv1a64:"));
    assert!(journals[0]
        .response_hash
        .as_deref()
        .expect("response hash should be stored")
        .starts_with("fnv1a64:"));
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
    assert!(journals[0].completed_at.is_some());
}

#[test]
fn mock_apply_records_failed_write_journal_entry_before_returning_error() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut writer = FailingWriter;
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("mock apply should fail");

    assert_eq!(
        error,
        ApplyError::Provider {
            provider: Provider::AniList,
            message: "mock provider failure".to_owned(),
        }
    );
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Failed);
    assert!(journals[0]
        .response_hash
        .as_deref()
        .expect("error response hash should be stored")
        .starts_with("fnv1a64:"));
    assert!(journals[0].completed_at.is_some());
}

#[test]
fn apply_with_post_write_verifier_records_success_after_matching_state() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    assert_eq!(plan.actions().len(), 1);

    let mut writer = RecordingWriter::default();
    let mut verifier = SequencePostWriteVerifier::new(vec![Ok(ProviderPostWriteState::Found(
        post_write_entry_from_action(&plan.actions()[0]),
    ))]);
    let summary = apply_plan_with_writer_and_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect("matching post-write verification should succeed");

    assert_eq!(summary.attempted, 1);
    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.post_write_verified, 1);
    assert_eq!(summary.post_write_skipped, 0);
    assert_eq!(
        verifier.calls,
        vec![(
            "fixture-account".to_owned(),
            Provider::AniList,
            "1".to_owned(),
            r#"{"ok":true}"#.to_owned()
        )]
    );
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
}

#[test]
fn apply_with_post_write_verifier_records_failed_journal_when_state_mismatches_after_write_ack() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    let action = &plan.actions()[0];
    assert_eq!(action.field_updates, vec![SyncField::Score]);

    let mut mismatched_entry = post_write_entry_from_action(action);
    mismatched_entry.score_hundred = Some(80);
    let mut writer = RecordingWriter::default();
    let mut verifier =
        SequencePostWriteVerifier::new(vec![Ok(ProviderPostWriteState::Found(mismatched_entry))]);
    let error = apply_plan_with_writer_and_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect_err("post-write mismatch should fail apply");

    match error {
        ApplyError::PostWriteVerification {
            provider,
            target_provider_entry_id,
            field: Some(SyncField::Score),
            expected,
            actual: Some(actual),
        } => {
            assert_eq!(provider, Provider::AniList);
            assert_eq!(target_provider_entry_id, "1");
            assert_eq!(expected, action.field_changes[0].new_value);
            assert_eq!(actual, "80");
        }
        other => panic!("expected post-write verification error, got {other:?}"),
    }
    assert_eq!(writer.calls.len(), 1);
    assert_eq!(verifier.calls.len(), 1);
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Failed);
    assert!(journals[0].response_hash.is_some());
}

#[test]
fn apply_with_post_write_verifier_does_not_verify_when_writer_fails() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut writer = FailingWriter;
    let mut verifier = SequencePostWriteVerifier::new(Vec::new());
    let error = apply_plan_with_writer_and_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect_err("writer failure should fail before post-write verification");

    assert_eq!(
        error,
        ApplyError::Provider {
            provider: Provider::AniList,
            message: "mock provider failure".to_owned(),
        }
    );
    assert!(verifier.calls.is_empty());
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Failed);
}

#[test]
fn apply_with_post_write_verifier_compares_only_planned_default_writable_fields() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    let action = &plan.actions()[0];
    assert_eq!(action.field_updates, vec![SyncField::Score]);

    let mut post_write_entry = post_write_entry_from_action(action);
    post_write_entry.status = CollectionStatus::InProgress;
    let mut writer = RecordingWriter::default();
    let mut verifier =
        SequencePostWriteVerifier::new(vec![Ok(ProviderPostWriteState::Found(post_write_entry))]);
    let summary = apply_plan_with_writer_and_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect("unplanned field differences should not fail post-write verification");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.post_write_verified, 1);
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
}

#[test]
fn apply_with_post_write_verifier_accepts_bangumi_ten_point_score_round_trip() {
    let store = seeded_bangumi_missing_rounding_score_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::AniList, Provider::Bangumi],
    )
    .expect("plan should build");
    let action = &plan.actions()[0];
    assert_eq!(action.target_provider, Provider::Bangumi);
    assert_eq!(action.score_hundred, Some(85));
    assert_eq!(action.field_updates, vec![SyncField::Score]);

    let mut post_write_entry = post_write_entry_from_action(action);
    post_write_entry.score_hundred = Some(90);
    let mut writer = RecordingWriter::default();
    let mut verifier =
        SequencePostWriteVerifier::new(vec![Ok(ProviderPostWriteState::Found(post_write_entry))]);
    let summary = apply_plan_with_writer_and_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect("Bangumi read-back score should match the provider 10-point round trip");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.post_write_verified, 1);
    let journals = store
        .write_journal_entries("fixture-account", Provider::Bangumi)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
}

#[test]
fn apply_with_post_write_verifier_accepts_myanimelist_ten_point_score_round_trip() {
    let store = seeded_myanimelist_missing_rounding_score_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");
    let action = &plan.actions()[0];
    assert_eq!(action.target_provider, Provider::MyAnimeList);
    assert_eq!(action.score_hundred, Some(85));
    assert_eq!(action.field_updates, vec![SyncField::Score]);

    let mut post_write_entry = post_write_entry_from_action(action);
    post_write_entry.score_hundred = Some(90);
    let mut writer = RecordingWriter::default();
    let mut verifier =
        SequencePostWriteVerifier::new(vec![Ok(ProviderPostWriteState::Found(post_write_entry))]);
    let summary = apply_plan_with_writer_and_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect("MyAnimeList read-back score should match the provider 10-point round trip");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.post_write_verified, 1);
    let journals = store
        .write_journal_entries("fixture-account", Provider::MyAnimeList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
}

#[test]
fn stored_post_write_verifier_fetches_snapshot_and_records_success_after_matching_entry() {
    let store = seeded_update_store();
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    let action = &plan.actions()[0];
    assert_eq!(action.target_provider, Provider::AniList);
    assert_eq!(action.field_updates, vec![SyncField::Score]);

    let mut writer = RecordingWriter::default();
    let mut secret_store = FakeAccessTokenStore::new("read-back-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::ok(vec![
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null}]}]}}}"#,
    ]);
    let mut verifier = StoredAccessTokenProviderPostWriteVerifier::new(
        &store,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
        50,
    );

    let summary = apply_plan_with_writer_and_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect("matching read-back state should succeed");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.post_write_verified, 1);
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(secret_store.lookups[0].provider, Provider::AniList);
    assert_eq!(read_transport.requests.len(), 1);
    assert_eq!(
        read_transport.requests[0].request.provider,
        Provider::AniList
    );
    assert!(read_transport.requests[0].headers.iter().any(|header| {
        header.name == "Authorization"
            && header.value == "Bearer read-back-access-token-secret"
            && header.sensitive
    }));
    assert!(!format!("{:?}", read_transport.requests[0]).contains("read-back-access-token-secret"));

    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
}

#[test]
fn deferred_stored_post_write_verifier_reuses_snapshot_for_multiple_actions_on_same_target_collection(
) {
    let store = seeded_two_anilist_score_update_store();
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    assert_eq!(plan.actions().len(), 2);
    assert!(plan
        .actions()
        .iter()
        .all(|action| action.target_provider == Provider::AniList));

    let read_back = r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null},{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":90,"progress":12,"progressVolumes":null}]}]}}}"#;
    let mut writer = RecordingWriter::default();
    let mut secret_store = FakeAccessTokenStore::new("read-back-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::ok(vec![read_back, read_back]);
    let mut verifier = StoredAccessTokenProviderPostWriteVerifier::new_with_snapshot_cache(
        &store,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
        50,
    );

    let summary = apply_plan_with_writer_and_deferred_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect("matching cached read-back state should succeed");

    assert_eq!(summary.attempted, 2);
    assert_eq!(summary.succeeded, 2);
    assert_eq!(summary.post_write_verified, 2);
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(read_transport.requests.len(), 1);
}

#[test]
fn deferred_stored_post_write_verifier_completes_remaining_journals_after_cached_miss() {
    let store = seeded_two_anilist_score_update_store();
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    assert_eq!(plan.actions().len(), 2);

    let read_back_missing_first = r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":90,"progress":12,"progressVolumes":null}]}]}}}"#;
    let mut writer = RecordingWriter::default();
    let mut secret_store = FakeAccessTokenStore::new("read-back-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::ok(vec![read_back_missing_first]);
    let mut verifier = StoredAccessTokenProviderPostWriteVerifier::new_with_snapshot_cache(
        &store,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
        50,
    );

    let error = apply_plan_with_writer_and_deferred_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect_err("missing cached read-back entry should fail deferred apply");

    assert_eq!(
        error,
        ApplyError::PostWriteVerification {
            provider: Provider::AniList,
            target_provider_entry_id: "1".to_owned(),
            field: None,
            expected: "entry present".to_owned(),
            actual: None,
        }
    );
    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(read_transport.requests.len(), 1);
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 2);
    assert!(journals
        .iter()
        .all(|journal| journal.result_status == WriteJournalStatus::Failed));
    assert!(journals
        .iter()
        .all(|journal| journal.completed_at.is_some()));
}

#[test]
fn stored_post_write_verifier_records_failed_journal_when_read_back_entry_is_missing() {
    let store = seeded_update_store();
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut writer = RecordingWriter::default();
    let mut secret_store = FakeAccessTokenStore::new("read-back-access-token-secret");
    let mut read_transport = RecordingAuthorizedReadTransport::ok(vec![
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":999,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null}]}]}}}"#,
    ]);
    let mut verifier = StoredAccessTokenProviderPostWriteVerifier::new(
        &store,
        1_700_000_000,
        &mut secret_store,
        &mut read_transport,
        50,
    );

    let error = apply_plan_with_writer_and_verifier(
        &store,
        "fixture-account",
        &plan,
        &mut writer,
        &mut verifier,
    )
    .expect_err("missing read-back entry should fail apply");

    assert_eq!(
        error,
        ApplyError::PostWriteVerification {
            provider: Provider::AniList,
            target_provider_entry_id: "1".to_owned(),
            field: None,
            expected: "entry present".to_owned(),
            actual: None,
        }
    );
    assert_eq!(read_transport.requests.len(), 1);
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Failed);
}

#[test]
fn mock_apply_rejects_conflicted_plan_before_journal_or_writer_call() {
    let store = seeded_conflicted_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    assert!(plan.actions().is_empty());
    assert!(plan
        .conflicts()
        .iter()
        .any(|conflict| conflict.field == SyncField::Status));

    let mut writer = RecordingWriter::default();
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("conflicted plan should not apply");

    assert_eq!(
        error,
        ApplyError::ConflictsPresent {
            count: plan.conflicts().len()
        }
    );
    assert!(writer.calls.is_empty());
    assert!(store
        .write_journal_entries("fixture-account", Provider::Bangumi)
        .expect("journal lookup should work")
        .is_empty());
    assert!(store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work")
        .is_empty());
}

#[test]
fn mock_apply_rejects_mixed_action_and_conflict_plan_before_journal_or_writer_call() {
    let store = seeded_manga_mixed_resolved_and_conflicted_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Manga,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");
    assert!(
        !plan.actions().is_empty(),
        "fixture should include field-level resolved actions"
    );
    assert!(
        !plan.conflicts().is_empty(),
        "fixture should retain unrelated conflicts"
    );

    let mut writer = RecordingWriter::default();
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("mixed conflicted plan should not apply");

    assert_eq!(
        error,
        ApplyError::ConflictsPresent {
            count: plan.conflicts().len()
        }
    );
    assert!(writer.calls.is_empty());
    for provider in [Provider::Bangumi, Provider::AniList, Provider::MyAnimeList] {
        assert!(store
            .write_journal_entries("fixture-account", provider)
            .expect("journal lookup should work")
            .is_empty());
    }
}

#[test]
fn mock_apply_reuses_same_journal_row_for_duplicate_operation() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut writer = RecordingWriter::default();
    apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect("first mock apply should succeed");
    apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect("second mock apply should succeed");

    assert_eq!(writer.calls.len(), 2);
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(
        journals.len(),
        1,
        "same deterministic operation id should upsert one journal row"
    );
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
}

#[test]
fn provider_request_writer_sends_built_request_through_injected_transport() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut transport = RecordingTransport::ok(r#"{"data":{"SaveMediaListEntry":{"id":1}}}"#);
    let mut writer = ProviderRequestWriter::new(&mut transport);
    let summary = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect("request writer should use fake transport");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(transport.requests.len(), 1);
    let request = &transport.requests[0];
    assert_eq!(request.method, ProviderWriteRequestMethod::Post);
    assert_eq!(request.url, "https://graphql.anilist.co");
    assert_eq!(request.content_type, "application/json");
    assert!(request.body.contains("SaveMediaListEntry"));
    assert!(request.body.contains("\"mediaId\":1"));

    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
    assert_eq!(
        journals[0].request_hash,
        stable_hash_for_test(&canonical_provider_write_request_body_for_test(request))
    );
    assert!(journals[0].response_hash.is_some());
}

#[test]
fn provider_request_writer_records_failed_transport_response() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut transport = RecordingTransport::fail("mock http 500");
    let mut writer = ProviderRequestWriter::new(&mut transport);
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("transport failure should fail apply");

    assert_eq!(
        error,
        ApplyError::Provider {
            provider: Provider::AniList,
            message: "mock http 500".to_owned(),
        }
    );
    assert_eq!(transport.requests.len(), 1);
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Failed);
    assert!(journals[0].response_hash.is_some());
}

#[test]
fn provider_request_writer_records_build_error_without_transport_call() {
    let store = seeded_invalid_anilist_target_id_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");
    assert_eq!(plan.actions().len(), 1);
    assert_eq!(plan.actions()[0].target_provider, Provider::AniList);
    assert_eq!(plan.actions()[0].target_provider_entry_id, "not-a-number");

    let mut transport = RecordingTransport::ok(r#"{"ok":true}"#);
    let mut writer = ProviderRequestWriter::new(&mut transport);
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("unsupported provider request should fail apply");

    match error {
        ApplyError::Provider { provider, message } => {
            assert_eq!(provider, Provider::AniList);
            assert!(message.contains("failed to build provider request"));
            assert!(message.contains("InvalidProviderEntryId"));
            assert!(message.contains("not-a-number"));
        }
        other => panic!("expected provider request build error, got {other:?}"),
    }
    assert!(transport.requests.is_empty());

    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Failed);
    assert!(journals[0].request_hash.starts_with("fnv1a64:"));
    assert!(journals[0].response_hash.is_some());
}

#[test]
fn stored_credential_writer_authorizes_and_sends_write_with_stored_token() {
    let store = seeded_update_store();
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut secret_store = FakeAccessTokenStore::new("write-access-token-secret");
    let mut transport =
        RecordingAuthorizedWriteTransport::ok(r#"{"data":{"SaveMediaListEntry":{"id":1}}}"#);
    {
        let mut writer = StoredAccessTokenProviderWriter::new(
            &store,
            1_700_000_000,
            &mut secret_store,
            &mut transport,
        );
        let summary = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
            .expect("stored credential writer should apply");

        assert_eq!(summary.attempted, 1);
        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 0);
    }

    assert_eq!(secret_store.lookups.len(), 1);
    assert_eq!(secret_store.lookups[0].provider, Provider::AniList);
    assert_eq!(secret_store.lookups[0].account_id, "fixture-account");
    assert_eq!(
        secret_store.lookups[0].credential_store_ref,
        "secret-store:anilist:fixture-account:v1"
    );
    assert_eq!(transport.requests.len(), 1);
    let request = &transport.requests[0];
    assert_eq!(request.request.provider, Provider::AniList);
    assert_eq!(request.request.method, ProviderWriteRequestMethod::Post);
    assert_eq!(request.request.url, "https://graphql.anilist.co");
    assert!(request.request.body.contains("SaveMediaListEntry"));
    assert!(request.headers.iter().any(|header| header.name == "Accept"
        && header.value == "application/json"
        && !header.sensitive));
    assert!(request
        .headers
        .iter()
        .any(|header| header.name == "Content-Type"
            && header.value == "application/json"
            && !header.sensitive));
    assert!(request
        .headers
        .iter()
        .any(|header| header.name == "Authorization"
            && header.value == "Bearer write-access-token-secret"
            && header.sensitive));
    let debug = format!("{request:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("write-access-token-secret"));

    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Succeeded);
    assert_eq!(
        journals[0].request_hash,
        stable_hash_for_test(&canonical_provider_write_request_body_for_test(
            &request.request
        ))
    );
}

#[test]
fn stored_credential_writer_reports_refresh_required_without_write_call() {
    let store = seeded_bangumi_missing_score_store();
    seed_refresh_required_credential(&store, Provider::Bangumi, "fixture-account");
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::AniList, Provider::Bangumi],
    )
    .expect("plan should build");
    assert_eq!(plan.actions().len(), 1);
    assert_eq!(plan.actions()[0].target_provider, Provider::Bangumi);
    assert_eq!(plan.actions()[0].field_updates, vec![SyncField::Score]);

    let mut secret_store = FakeAccessTokenStore::new("unused-access-token-secret");
    let mut transport = RecordingAuthorizedWriteTransport::ok(r#"{"ok":true}"#);
    let mut writer = StoredAccessTokenProviderWriter::new(
        &store,
        1_700_000_000,
        &mut secret_store,
        &mut transport,
    );
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("refresh-required credential should not apply");

    assert_eq!(
        error,
        ApplyError::ProviderCredentialRefreshRequired {
            provider: Provider::Bangumi,
        }
    );
    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
    let journals = store
        .write_journal_entries("fixture-account", Provider::Bangumi)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Failed);
    assert!(journals[0].response_hash.is_some());
}

#[test]
fn stored_credential_writer_reports_reauthorize_without_write_call() {
    let store = seeded_update_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut secret_store = FakeAccessTokenStore::new("unused-access-token-secret");
    let mut transport = RecordingAuthorizedWriteTransport::ok(r#"{"ok":true}"#);
    let mut writer = StoredAccessTokenProviderWriter::new(
        &store,
        1_700_000_000,
        &mut secret_store,
        &mut transport,
    );
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("missing credential should require reauthorization");

    assert_eq!(
        error,
        ApplyError::ProviderCredentialReauthorizeRequired {
            provider: Provider::AniList,
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
        }
    );
    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
    let journals = store
        .write_journal_entries("fixture-account", Provider::AniList)
        .expect("journal lookup should work");
    assert_eq!(journals.len(), 1);
    assert_eq!(journals[0].result_status, WriteJournalStatus::Failed);
    assert!(journals[0].response_hash.is_some());
}

#[test]
fn stored_credential_writer_build_error_does_not_read_credentials() {
    let store = seeded_invalid_anilist_target_id_store();
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut secret_store = FakeAccessTokenStore::new("unused-access-token-secret");
    let mut transport = RecordingAuthorizedWriteTransport::ok(r#"{"ok":true}"#);
    let mut writer = StoredAccessTokenProviderWriter::new(
        &store,
        1_700_000_000,
        &mut secret_store,
        &mut transport,
    );
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("provider request build error should fail apply");

    match error {
        ApplyError::Provider { provider, message } => {
            assert_eq!(provider, Provider::AniList);
            assert!(message.contains("failed to build provider request"));
            assert!(message.contains("InvalidProviderEntryId"));
            assert!(message.contains("not-a-number"));
        }
        other => panic!("expected provider build error, got {other:?}"),
    }
    assert!(secret_store.lookups.is_empty());
    assert!(transport.requests.is_empty());
}

#[test]
fn stored_credential_writer_rejects_provider_mismatched_token_before_transport() {
    let store = seeded_update_store();
    seed_valid_credential(&store, Provider::AniList, "fixture-account");
    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    let mut secret_store =
        FakeAccessTokenStore::new("wrong-provider-token").with_provider(Provider::Bangumi);
    let mut transport = RecordingAuthorizedWriteTransport::ok(r#"{"ok":true}"#);
    let mut writer = StoredAccessTokenProviderWriter::new(
        &store,
        1_700_000_000,
        &mut secret_store,
        &mut transport,
    );
    let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
        .expect_err("provider-mismatched token should not apply");

    match error {
        ApplyError::Provider { provider, message } => {
            assert_eq!(provider, Provider::AniList);
            assert!(message.contains("failed to authorize provider write request"));
            assert!(message.contains("ProviderMismatch"));
            assert!(!message.contains("wrong-provider-token"));
        }
        other => panic!("expected authorization error, got {other:?}"),
    }
    assert_eq!(secret_store.lookups.len(), 1);
    assert!(transport.requests.is_empty());
}

#[derive(Default)]
struct RecordingWriter {
    calls: Vec<(String, Provider, String)>,
}

impl ProviderWriter for RecordingWriter {
    fn apply_collection_action(
        &mut self,
        account_id: &str,
        action: &PlannedAction,
    ) -> Result<ProviderWriteResult, ApplyError> {
        self.calls.push((
            account_id.to_owned(),
            action.target_provider,
            action.target_provider_entry_id.clone(),
        ));
        Ok(ProviderWriteResult {
            response_body: r#"{"ok":true}"#.to_owned(),
        })
    }
}

struct FailingWriter;

impl ProviderWriter for FailingWriter {
    fn apply_collection_action(
        &mut self,
        _account_id: &str,
        action: &PlannedAction,
    ) -> Result<ProviderWriteResult, ApplyError> {
        Err(ApplyError::Provider {
            provider: action.target_provider,
            message: "mock provider failure".to_owned(),
        })
    }
}

struct SequencePostWriteVerifier {
    responses: VecDeque<Result<ProviderPostWriteState, ApplyError>>,
    calls: Vec<(String, Provider, String, String)>,
}

impl SequencePostWriteVerifier {
    fn new(responses: Vec<Result<ProviderPostWriteState, ApplyError>>) -> Self {
        Self {
            responses: responses.into(),
            calls: Vec::new(),
        }
    }
}

impl ProviderPostWriteVerifier for SequencePostWriteVerifier {
    fn read_collection_entry_after_write(
        &mut self,
        account_id: &str,
        action: &PlannedAction,
        result: &ProviderWriteResult,
    ) -> Result<ProviderPostWriteState, ApplyError> {
        self.calls.push((
            account_id.to_owned(),
            action.target_provider,
            action.target_provider_entry_id.clone(),
            result.response_body.clone(),
        ));
        self.responses
            .pop_front()
            .expect("post-write verifier response should be seeded")
    }
}

fn post_write_entry_from_action(action: &PlannedAction) -> ProviderPostWriteEntry {
    ProviderPostWriteEntry {
        status: action.status,
        score_hundred: action.score_hundred,
        progress_episodes: action.progress_episodes,
        progress_chapters: action.progress_chapters,
        progress_volumes: action.progress_volumes,
    }
}

struct RecordingTransport {
    requests: Vec<ProviderWriteRequest>,
    response: Result<String, String>,
}

impl RecordingTransport {
    fn ok(response_body: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Ok(response_body.to_owned()),
        }
    }

    fn fail(message: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Err(message.to_owned()),
        }
    }
}

impl sync_core::sync::ProviderRequestTransport for RecordingTransport {
    fn send_provider_write_request(
        &mut self,
        request: ProviderWriteRequest,
    ) -> Result<String, String> {
        self.requests.push(request);
        self.response.clone()
    }
}

struct FakeAccessTokenStore {
    lookups: Vec<ProviderCredentialSecretLookup>,
    access_token: String,
    token_provider: Option<Provider>,
}

impl FakeAccessTokenStore {
    fn new(access_token: &str) -> Self {
        Self {
            lookups: Vec::new(),
            access_token: access_token.to_owned(),
            token_provider: None,
        }
    }

    fn with_provider(mut self, provider: Provider) -> Self {
        self.token_provider = Some(provider);
        self
    }
}

impl ProviderCredentialAccessTokenStore for FakeAccessTokenStore {
    fn get_provider_access_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<ProviderAccessToken, ProviderAuthError> {
        let provider = lookup.provider;
        self.lookups.push(lookup);
        ProviderAccessToken::new(
            self.token_provider.unwrap_or(provider),
            self.access_token.clone(),
        )
    }
}

struct RecordingAuthorizedWriteTransport {
    requests: Vec<AuthorizedProviderWriteRequest>,
    response: Result<String, String>,
}

impl RecordingAuthorizedWriteTransport {
    fn ok(response_body: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Ok(response_body.to_owned()),
        }
    }
}

impl AuthorizedProviderWriteTransport for RecordingAuthorizedWriteTransport {
    fn send_authorized_provider_write_request(
        &mut self,
        request: AuthorizedProviderWriteRequest,
    ) -> Result<String, String> {
        self.requests.push(request);
        self.response.clone()
    }
}

struct RecordingAuthorizedReadTransport {
    requests: Vec<AuthorizedProviderReadRequest>,
    responses: VecDeque<Result<String, String>>,
}

impl RecordingAuthorizedReadTransport {
    fn ok(response_bodies: Vec<&str>) -> Self {
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
        self.responses
            .pop_front()
            .expect("authorized read response should be seeded")
    }
}

fn seeded_update_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");

    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    store
}

fn seeded_two_anilist_score_update_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1], [254, 2]]")
        .expect("manual relation import should work");
    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":254,"subject_type":"anime","collection_type":"collect","rate":9,"ep_status":12,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null},{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":12,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");

    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    store
}

fn seeded_bangumi_missing_score_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"do","rate":0,"ep_status":12,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"CURRENT","score":70,"progress":12,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");

    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    store
}

fn seeded_bangumi_missing_rounding_score_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":0,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":85,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");

    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    store
}

fn seeded_myanimelist_missing_rounding_score_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let work_id = store
        .create_identity_work(MediaKind::Anime, "rounding score work")
        .expect("identity work should create");
    for (provider, external_id) in [(Provider::AniList, "1"), (Provider::MyAnimeList, "5")] {
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
            .expect("external id edge should insert");
    }
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":85,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    let mal = parse_myanimelist_collection_fixture(
        r#"{"data":[{"node":{"id":5,"media_type":"anime"},"list_status":{"status":"completed","score":0,"num_episodes_watched":26,"updated_at":"2026-06-01T00:00:00Z"}}]}"#,
        MediaKind::Anime,
    )
    .expect("myanimelist fixture should parse");

    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("myanimelist snapshot should import");

    store
}

fn seeded_conflicted_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"CURRENT","score":70,"progress":12,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");

    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    store
}

fn seeded_manga_mixed_resolved_and_conflicted_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Manga, "[[9001, 2]]")
        .expect("manual relation import should work");
    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: store
                .find_work_by_external_id(Provider::Bangumi, MediaKind::Manga, "9001")
                .expect("lookup should work")
                .expect("work should exist"),
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Manga,
            external_id: "30013".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual-test".to_owned(),
            dataset_version: None,
        })
        .expect("external id edge should insert");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"collect","rate":7,"ep_status":12,"vol_status":3,"updated_at":"2026-06-01T00:00:00Z"}]}"#,
        MediaKind::Manga,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"MANGA"},"status":"CURRENT","score":90,"progress":12,"progressVolumes":3,"updatedAt":1780444800}]}]}}}"#,
        MediaKind::Manga,
    )
    .expect("anilist fixture should parse");
    let mal = parse_myanimelist_collection_fixture(
        r#"{"data":[{"node":{"id":30013,"media_type":"manga"},"list_status":{"status":"dropped","score":7,"num_chapters_read":12,"num_volumes_read":3,"updated_at":"2026-06-03T00:00:00Z"}}]}"#,
        MediaKind::Manga,
    )
    .expect("myanimelist fixture should parse");

    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("myanimelist snapshot should import");

    store
}

fn seeded_invalid_anilist_target_id_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let work_id = store
        .create_identity_work(MediaKind::Anime, "invalid anilist target id work")
        .expect("identity work should create");
    for (provider, external_id) in [
        (Provider::Bangumi, "253"),
        (Provider::AniList, "not-a-number"),
    ] {
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
            .expect("external id edge should insert");
    }

    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");

    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");

    store
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
            auth_flow: provider_credential_capability(provider).auth_flow,
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

fn canonical_provider_write_request_body_for_test(request: &ProviderWriteRequest) -> String {
    let method = match request.method {
        ProviderWriteRequestMethod::Post => "POST",
        ProviderWriteRequestMethod::Patch => "PATCH",
        ProviderWriteRequestMethod::Put => "PUT",
    };

    serde_json::json!({
        "method": method,
        "url": request.url,
        "content_type": request.content_type,
        "body": request.body,
    })
    .to_string()
}

fn stable_hash_for_test(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;

    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    format!("fnv1a64:{hash:016x}")
}
