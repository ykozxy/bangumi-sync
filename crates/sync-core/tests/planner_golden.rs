use sync_core::identity::import_legacy_manual_relations;
use sync_core::model::{CollectionStatus, MediaKind, Provider, SyncField};
use sync_core::provider::{
    parse_anilist_collection_fixture, parse_bangumi_collection_fixture,
    parse_myanimelist_collection_fixture,
};
use sync_core::store::SqliteStore;
use sync_core::sync::{plan_dry_run, PlannedActionKind, PlannedFieldChange};

#[test]
fn planner_adds_missing_provider_entry_when_sources_agree() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let bangumi = parse_bangumi_collection_fixture(
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
    .expect("bangumi fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert!(plan.is_dry_run());
    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.actions().len(), 1);

    let action = &plan.actions()[0];
    assert_eq!(action.kind, PlannedActionKind::AddEntry);
    assert_eq!(action.source_provider, Provider::Bangumi);
    assert_eq!(action.target_provider, Provider::AniList);
    assert_eq!(action.target_provider_entry_id, "1");
    assert_eq!(action.media_kind, MediaKind::Anime);
    assert_eq!(action.status, CollectionStatus::Completed);
    assert_eq!(action.score_hundred, Some(100));
    assert_eq!(action.progress_episodes, Some(26));
    assert_eq!(
        action.field_updates,
        vec![
            SyncField::Status,
            SyncField::Score,
            SyncField::ProgressEpisodes,
        ]
    );
    assert!(action.reason.contains("missing target provider entry"));
}

#[test]
fn planner_adds_missing_manga_entry_with_chapter_and_volume_progress() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Manga, "[[9001, 2]]")
        .expect("manual relation import should work");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{
            "data": [
                {
                    "subject_id": 9001,
                    "subject_type": "book",
                    "collection_type": "do",
                    "rate": 8,
                    "ep_status": 12,
                    "vol_status": 3
                }
            ]
        }"#,
        MediaKind::Manga,
    )
    .expect("bangumi fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Manga,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.actions().len(), 1);

    let action = &plan.actions()[0];
    assert_eq!(action.kind, PlannedActionKind::AddEntry);
    assert_eq!(action.target_provider, Provider::AniList);
    assert_eq!(action.target_provider_entry_id, "2");
    assert_eq!(action.media_kind, MediaKind::Manga);
    assert_eq!(action.status, CollectionStatus::InProgress);
    assert_eq!(action.score_hundred, Some(80));
    assert_eq!(action.progress_episodes, None);
    assert_eq!(action.progress_chapters, Some(12));
    assert_eq!(action.progress_volumes, Some(3));
    assert_eq!(
        action.field_updates,
        vec![
            SyncField::Status,
            SyncField::Score,
            SyncField::ProgressChapters,
            SyncField::ProgressVolumes,
        ]
    );
}

#[test]
fn planner_updates_existing_provider_entry_only_for_missing_optional_fields() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let bangumi = parse_bangumi_collection_fixture(
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
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME" },
                                    "status": "COMPLETED",
                                    "score": 0,
                                    "progress": 26,
                                    "progressVolumes": null
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.actions().len(), 1);

    let action = &plan.actions()[0];
    assert_eq!(action.kind, PlannedActionKind::UpdateEntry);
    assert_eq!(action.source_provider, Provider::Bangumi);
    assert_eq!(action.target_provider, Provider::AniList);
    assert_eq!(action.target_provider_entry_id, "1");
    assert_eq!(action.media_kind, MediaKind::Anime);
    assert_eq!(action.score_hundred, Some(100));
    assert_eq!(action.progress_episodes, Some(26));
    assert_eq!(action.field_updates, vec![SyncField::Score]);
    assert_eq!(
        action.field_changes,
        vec![PlannedFieldChange {
            field: SyncField::Score,
            old_value: None,
            new_value: "100".to_owned(),
        }]
    );
    assert!(action.reason.contains("missing target field"));
}

#[test]
fn planner_emits_conflicts_and_blocks_actions_when_sources_disagree() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: store
                .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "253")
                .expect("lookup should work")
                .expect("work should exist"),
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Anime,
            external_id: "5114".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");

    let bangumi = parse_bangumi_collection_fixture(
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
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME" },
                                    "status": "CURRENT",
                                    "score": 70,
                                    "progress": 12,
                                    "progressVolumes": null
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");

    assert!(
        plan.actions().is_empty(),
        "conflicted work should not emit writes"
    );
    let conflict_fields = plan
        .conflicts()
        .iter()
        .map(|conflict| conflict.field)
        .collect::<Vec<_>>();
    assert!(conflict_fields.contains(&SyncField::Status));
    assert!(conflict_fields.contains(&SyncField::Score));
    assert!(conflict_fields.contains(&SyncField::ProgressEpisodes));
    assert!(plan
        .conflicts()
        .iter()
        .all(|conflict| conflict.reason.contains("multiple source values")));
}

#[test]
fn planner_uses_newer_reliable_provider_timestamp_to_resolve_disagreement() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{
            "data": [
                {
                    "subject_id": 253,
                    "subject_type": "anime",
                    "collection_type": "do",
                    "rate": 7,
                    "ep_status": 12,
                    "vol_status": 0,
                    "updated_at": "2026-06-01T00:00:00Z"
                }
            ]
        }"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME" },
                                    "status": "COMPLETED",
                                    "score": 90,
                                    "progress": 26,
                                    "progressVolumes": null,
                                    "updatedAt": 1780444800
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.actions().len(), 1);

    let action = &plan.actions()[0];
    assert_eq!(action.kind, PlannedActionKind::UpdateEntry);
    assert_eq!(action.source_provider, Provider::AniList);
    assert_eq!(action.target_provider, Provider::Bangumi);
    assert_eq!(action.status, CollectionStatus::Completed);
    assert_eq!(action.score_hundred, Some(90));
    assert_eq!(action.progress_episodes, Some(26));
    assert_eq!(
        action.field_updates,
        vec![SyncField::Status, SyncField::Score]
    );
    assert_eq!(
        action.field_changes,
        vec![
            PlannedFieldChange {
                field: SyncField::Status,
                old_value: Some("in_progress".to_owned()),
                new_value: "completed".to_owned(),
            },
            PlannedFieldChange {
                field: SyncField::Score,
                old_value: Some("70".to_owned()),
                new_value: "90".to_owned(),
            },
        ]
    );
    assert!(action.reason.contains("newer reliable provider timestamp"));
}

#[test]
fn planner_ignores_unselected_provider_values_when_planning_subset() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: store
                .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "253")
                .expect("lookup should work")
                .expect("work should exist"),
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Anime,
            external_id: "5114".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{
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
        }"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME" },
                                    "status": "COMPLETED",
                                    "score": 100,
                                    "progress": 26,
                                    "progressVolumes": null,
                                    "updatedAt": 1780358400
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    let mal = parse_myanimelist_collection_fixture(
        r#"{
            "data": [
                {
                    "node": { "id": 5114, "media_type": "anime" },
                    "list_status": {
                        "status": "watching",
                        "score": 7,
                        "num_episodes_watched": 12,
                        "updated_at": "2026-06-03T00:00:00Z"
                    }
                }
            ]
        }"#,
        MediaKind::Anime,
    )
    .expect("mal fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("mal snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert!(
        plan.actions().is_empty(),
        "unselected provider values must not drive subset sync actions"
    );
    assert!(
        plan.conflicts().is_empty(),
        "unselected provider values must not create subset conflicts"
    );
}

#[test]
fn planner_keeps_conflict_when_only_newer_timestamp_is_low_reliability() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{
            "data": [
                {
                    "subject_id": 253,
                    "subject_type": "anime",
                    "collection_type": "collect",
                    "rate": 10,
                    "ep_status": 26,
                    "vol_status": 0,
                    "updated_at": "2026-06-03T00:00:00Z"
                }
            ]
        }"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME" },
                                    "status": "CURRENT",
                                    "score": 70,
                                    "progress": 12,
                                    "progressVolumes": null,
                                    "updatedAt": 1780358400
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

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
}

#[test]
fn planner_keeps_conflict_when_reliable_timestamps_tie() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: store
                .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
                .expect("lookup should work")
                .expect("work should exist"),
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Anime,
            external_id: "5114".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");

    let anilist = parse_anilist_collection_fixture(
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME" },
                                    "status": "COMPLETED",
                                    "score": 90,
                                    "progress": 26,
                                    "progressVolumes": null,
                                    "updatedAt": 1780444800
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    let mal = parse_myanimelist_collection_fixture(
        r#"{
            "data": [
                {
                    "node": { "id": 5114, "media_type": "anime" },
                    "list_status": {
                        "status": "watching",
                        "score": 7,
                        "num_episodes_watched": 12,
                        "updated_at": "2026-06-03T00:00:00Z"
                    }
                }
            ]
        }"#,
        MediaKind::Anime,
    )
    .expect("mal fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("mal snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");

    assert!(
        plan.actions().is_empty(),
        "tied reliable timestamps should not emit writes"
    );
    let conflict_fields = plan
        .conflicts()
        .iter()
        .map(|conflict| conflict.field)
        .collect::<Vec<_>>();
    assert!(conflict_fields.contains(&SyncField::Status));
    assert!(conflict_fields.contains(&SyncField::Score));
    assert!(conflict_fields.contains(&SyncField::ProgressEpisodes));
}

#[test]
fn planner_uses_field_level_changed_sources_when_reliable_providers_change_different_manga_fields()
{
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Manga, "[[9001, 2]]")
        .expect("manual relation import should work");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: store
                .find_work_by_external_id(Provider::Bangumi, MediaKind::Manga, "9001")
                .expect("lookup should work")
                .expect("work should exist"),
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Manga,
            external_id: "30013".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{
            "data": [
                {
                    "subject_id": 9001,
                    "subject_type": "book",
                    "collection_type": "do",
                    "rate": 7,
                    "ep_status": 12,
                    "vol_status": 3,
                    "updated_at": "2026-06-01T00:00:00Z"
                }
            ]
        }"#,
        MediaKind::Manga,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 2,
                                    "media": { "type": "MANGA" },
                                    "status": "CURRENT",
                                    "score": 90,
                                    "progress": 12,
                                    "progressVolumes": 3,
                                    "updatedAt": 1780444800
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        MediaKind::Manga,
    )
    .expect("anilist fixture should parse");
    let mal = parse_myanimelist_collection_fixture(
        r#"{
            "data": [
                {
                    "node": { "id": 30013, "media_type": "manga" },
                    "list_status": {
                        "status": "reading",
                        "score": 7,
                        "num_chapters_read": 14,
                        "num_volumes_read": 3,
                        "updated_at": "2026-06-03T00:00:00Z"
                    }
                }
            ]
        }"#,
        MediaKind::Manga,
    )
    .expect("mal fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("mal snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Manga,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");

    assert!(plan.conflicts().is_empty());
    assert!(plan.actions().iter().any(|action| {
        action.source_provider == Provider::AniList
            && action.target_provider == Provider::Bangumi
            && action.field_updates == vec![SyncField::Score]
            && action.field_changes
                == vec![PlannedFieldChange {
                    field: SyncField::Score,
                    old_value: Some("70".to_owned()),
                    new_value: "90".to_owned(),
                }]
    }));
    assert!(plan.actions().iter().any(|action| {
        action.source_provider == Provider::AniList
            && action.target_provider == Provider::MyAnimeList
            && action.field_updates == vec![SyncField::Score]
            && action.field_changes
                == vec![PlannedFieldChange {
                    field: SyncField::Score,
                    old_value: Some("70".to_owned()),
                    new_value: "90".to_owned(),
                }]
    }));
    assert!(plan.actions().iter().any(|action| {
        action.source_provider == Provider::MyAnimeList
            && action.target_provider == Provider::Bangumi
            && action.field_updates == vec![SyncField::ProgressChapters]
            && action.field_changes
                == vec![PlannedFieldChange {
                    field: SyncField::ProgressChapters,
                    old_value: Some("12".to_owned()),
                    new_value: "14".to_owned(),
                }]
    }));
    assert!(plan.actions().iter().any(|action| {
        action.source_provider == Provider::MyAnimeList
            && action.target_provider == Provider::AniList
            && action.field_updates == vec![SyncField::ProgressChapters]
            && action.field_changes
                == vec![PlannedFieldChange {
                    field: SyncField::ProgressChapters,
                    old_value: Some("12".to_owned()),
                    new_value: "14".to_owned(),
                }]
    }));
    assert!(
        plan.actions().iter().all(|action| {
            !(action.target_provider == Provider::AniList
                && action.field_updates.contains(&SyncField::Score))
        }),
        "a newer MAL chapter update must not regress AniList score"
    );
}

#[test]
fn planner_uses_local_history_for_two_providers_without_timestamps() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let baseline_bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":7,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("baseline bangumi fixture should parse");
    let baseline_anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":70,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("baseline anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &baseline_bangumi)
        .expect("baseline bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &baseline_anilist)
        .expect("baseline anilist snapshot should import");

    let changed_bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("changed bangumi fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &changed_bangumi)
        .expect("changed bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &changed_bangumi)
        .expect("unchanged changed-source snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &baseline_anilist)
        .expect("unchanged anilist snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.actions().len(), 1);
    let action = &plan.actions()[0];
    assert_eq!(action.source_provider, Provider::Bangumi);
    assert_eq!(action.target_provider, Provider::AniList);
    assert_eq!(action.field_updates, vec![SyncField::Score]);
    assert_eq!(
        action.field_changes,
        vec![PlannedFieldChange {
            field: SyncField::Score,
            old_value: Some("70".to_owned()),
            new_value: "100".to_owned(),
        }]
    );
    assert!(action.reason.contains("local-history"));
}

#[test]
fn planner_fails_closed_when_two_providers_both_changed_the_same_field() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let baseline_bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":7,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("baseline bangumi fixture should parse");
    let baseline_anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":70,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("baseline anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &baseline_bangumi)
        .expect("baseline bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &baseline_anilist)
        .expect("baseline anilist snapshot should import");

    let changed_bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("changed bangumi fixture should parse");
    let changed_anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":90,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("changed anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &changed_bangumi)
        .expect("changed bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &changed_anilist)
        .expect("changed anilist snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert!(plan.actions().is_empty());
    assert_eq!(plan.conflicts().len(), 1);
    assert_eq!(plan.conflicts()[0].field, SyncField::Score);
}

#[test]
fn planner_does_not_treat_a_stale_equal_peer_as_change_acknowledgement() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let initial_bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":7,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("initial bangumi fixture should parse");
    let stale_equal_anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("stale equal anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &initial_bangumi)
        .expect("initial bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &stale_equal_anilist)
        .expect("stale equal anilist snapshot should import");

    let changed_bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("changed bangumi fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &changed_bangumi)
        .expect("changed bangumi snapshot should import");

    let changed_anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":70,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("changed anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &changed_anilist)
        .expect("changed anilist snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert!(plan.actions().is_empty());
    assert_eq!(plan.conflicts().len(), 1);
    assert_eq!(plan.conflicts()[0].field, SyncField::Score);
}

#[test]
fn planner_blocks_unrepresentable_optional_clear_instead_of_restoring_old_value() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let baseline_bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("baseline bangumi fixture should parse");
    let baseline_anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("baseline anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &baseline_bangumi)
        .expect("baseline bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &baseline_anilist)
        .expect("baseline anilist snapshot should import");

    let cleared_bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":0,"ep_status":26,"vol_status":0}]}"#,
        MediaKind::Anime,
    )
    .expect("cleared bangumi fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &cleared_bangumi)
        .expect("cleared bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &cleared_bangumi)
        .expect("unchanged cleared bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &baseline_anilist)
        .expect("unchanged anilist snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::AniList, Provider::Bangumi],
    )
    .expect("plan should build");

    assert!(plan.actions().is_empty());
    assert_eq!(plan.conflicts().len(), 1);
    assert_eq!(plan.conflicts()[0].field, SyncField::Score);
    assert!(plan.conflicts()[0]
        .values
        .iter()
        .any(|value| value.value == "<unset>"));
}

#[test]
fn planner_keeps_conflict_for_stale_reliable_field_outlier() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Manga, "[[9001, 2]]")
        .expect("manual relation import should work");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: store
                .find_work_by_external_id(Provider::Bangumi, MediaKind::Manga, "9001")
                .expect("lookup should work")
                .expect("work should exist"),
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Manga,
            external_id: "30013".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"do","rate":7,"ep_status":12,"vol_status":3,"updated_at":"2026-06-02T00:00:00Z"}]}"#,
        MediaKind::Manga,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"MANGA"},"status":"CURRENT","score":90,"progress":12,"progressVolumes":3,"updatedAt":1780272000}]}]}}}"#,
        MediaKind::Manga,
    )
    .expect("anilist fixture should parse");
    let mal = parse_myanimelist_collection_fixture(
        r#"{"data":[{"node":{"id":30013,"media_type":"manga"},"list_status":{"status":"reading","score":7,"num_chapters_read":12,"num_volumes_read":3,"updated_at":"2026-06-03T00:00:00Z"}}]}"#,
        MediaKind::Manga,
    )
    .expect("mal fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("mal snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Manga,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");

    assert!(plan.actions().is_empty());
    assert!(plan
        .conflicts()
        .iter()
        .any(|conflict| conflict.field == SyncField::Score));
}

#[test]
fn planner_keeps_conflict_when_reliable_field_outlier_is_older_than_any_peer() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Manga, "[[9001, 2]]")
        .expect("manual relation import should work");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: store
                .find_work_by_external_id(Provider::Bangumi, MediaKind::Manga, "9001")
                .expect("lookup should work")
                .expect("work should exist"),
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Manga,
            external_id: "30013".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"do","rate":7,"ep_status":12,"vol_status":3,"updated_at":"2026-06-01T00:00:00Z"}]}"#,
        MediaKind::Manga,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"MANGA"},"status":"CURRENT","score":90,"progress":12,"progressVolumes":3,"updatedAt":1780358400}]}]}}}"#,
        MediaKind::Manga,
    )
    .expect("anilist fixture should parse");
    let mal = parse_myanimelist_collection_fixture(
        r#"{"data":[{"node":{"id":30013,"media_type":"manga"},"list_status":{"status":"reading","score":7,"num_chapters_read":12,"num_volumes_read":3,"updated_at":"2026-06-03T00:00:00Z"}}]}"#,
        MediaKind::Manga,
    )
    .expect("mal fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("mal snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Manga,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");

    assert!(plan.actions().is_empty());
    assert!(plan
        .conflicts()
        .iter()
        .any(|conflict| conflict.field == SyncField::Score));
}

#[test]
fn planner_keeps_resolved_field_actions_when_another_field_remains_ambiguous() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Manga, "[[9001, 2]]")
        .expect("manual relation import should work");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: store
                .find_work_by_external_id(Provider::Bangumi, MediaKind::Manga, "9001")
                .expect("lookup should work")
                .expect("work should exist"),
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Manga,
            external_id: "30013".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");

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
    .expect("mal fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("mal snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Manga,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");

    assert!(plan
        .conflicts()
        .iter()
        .any(|conflict| conflict.field == SyncField::Status));
    assert!(plan.actions().iter().any(|action| {
        action.source_provider == Provider::AniList
            && action.target_provider == Provider::Bangumi
            && action.field_updates == vec![SyncField::Score]
    }));
    assert!(plan.actions().iter().any(|action| {
        action.source_provider == Provider::AniList
            && action.target_provider == Provider::MyAnimeList
            && action.field_updates == vec![SyncField::Score]
    }));
}

#[test]
fn planner_excludes_bangumi_anime_episode_progress_from_applyable_actions() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    import_legacy_manual_relations(&store, MediaKind::Anime, "[[253, 1]]")
        .expect("manual relation import should work");

    let bangumi = parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"do","rate":7,"ep_status":12,"vol_status":0,"updated_at":"2026-06-01T00:00:00Z"}]}"#,
        MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":90,"progress":26,"progressVolumes":null,"updatedAt":1780444800}]}]}}}"#,
        MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList],
    )
    .expect("plan should build");

    assert_eq!(plan.actions().len(), 1);
    assert_eq!(plan.actions()[0].target_provider, Provider::Bangumi);
    assert_eq!(
        plan.actions()[0].field_updates,
        vec![SyncField::Status, SyncField::Score]
    );
    assert!(!plan.actions()[0]
        .field_updates
        .contains(&SyncField::ProgressEpisodes));

    let diagnostic = plan
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.source_provider == Provider::AniList
                && diagnostic.target_provider == Provider::Bangumi
                && diagnostic.media_kind == MediaKind::Anime
                && diagnostic.field == SyncField::ProgressEpisodes
        })
        .expect("skipped Bangumi anime episode progress should be diagnosed");
    assert_eq!(diagnostic.work_id, plan.actions()[0].work_id);
    assert_eq!(diagnostic.target_provider_entry_id, "253");
    assert_eq!(
        diagnostic.reason,
        "Bangumi anime episode progress writes require resolved episode IDs; aggregate progress is unsupported"
    );
}

#[test]
fn planner_ignores_unmatched_collection_entries() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let mal = parse_myanimelist_collection_fixture(
        r#"{
            "data": [
                {
                    "node": { "id": 5114, "media_type": "anime" },
                    "list_status": {
                        "status": "completed",
                        "score": 10,
                        "num_episodes_watched": 64
                    }
                }
            ]
        }"#,
        MediaKind::Anime,
    )
    .expect("mal fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");

    assert!(plan.actions().is_empty());
    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.skipped_unmatched_count(), 1);
}
