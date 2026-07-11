use sync_core::identity::{
    import_legacy_ignore_entries, import_legacy_manual_relations, LegacyImportError,
};
use sync_core::model::{MediaKind, Provider};
use sync_core::store::{ManualMappingDecision, SqliteStore};

#[test]
fn manual_relations_import_as_high_confidence_identity_edges() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let imported =
        import_legacy_manual_relations(&store, MediaKind::Anime, "[[348335,138714],[840,1033]]")
            .expect("manual relations should import");

    assert_eq!(imported, 2);

    let first_bangumi_work = store
        .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "348335")
        .expect("bangumi lookup should work")
        .expect("bangumi edge should exist");
    let first_anilist_work = store
        .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "138714")
        .expect("anilist lookup should work")
        .expect("anilist edge should exist");

    assert_eq!(first_bangumi_work, first_anilist_work);
    assert_eq!(
        store
            .manual_mapping_decision(Provider::Bangumi, MediaKind::Anime, "348335")
            .expect("decision lookup should work"),
        Some(ManualMappingDecision::Link)
    );
    assert_eq!(
        store
            .manual_mapping_decision(Provider::AniList, MediaKind::Anime, "138714")
            .expect("decision lookup should work"),
        Some(ManualMappingDecision::Link)
    );
}

#[test]
fn manual_relation_import_is_idempotent_for_existing_pair() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    import_legacy_manual_relations(&store, MediaKind::Anime, "[[348335,138714]]")
        .expect("first import should work");
    let first_work = store
        .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "348335")
        .expect("lookup should work")
        .expect("edge should exist");

    import_legacy_manual_relations(&store, MediaKind::Anime, "[[348335,138714]]")
        .expect("second import should work");
    let second_work = store
        .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "348335")
        .expect("lookup should work")
        .expect("edge should still exist");

    assert_eq!(first_work, second_work);
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        1,
        "re-importing the same manual pair should not create duplicate works"
    );
}

#[test]
fn manual_relation_import_rejects_conflicting_existing_works() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let bangumi_work = store
        .create_identity_work(MediaKind::Anime, "bangumi-only")
        .expect("bangumi work should create");
    let anilist_work = store
        .create_identity_work(MediaKind::Anime, "anilist-only")
        .expect("anilist work should create");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: bangumi_work,
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "348335".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("bangumi edge should insert");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: anilist_work,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "138714".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("anilist edge should insert");

    let error = import_legacy_manual_relations(&store, MediaKind::Anime, "[[348335,138714]]")
        .expect_err("conflicting manual relation should be rejected");

    assert_eq!(
        error,
        LegacyImportError::ConflictingManualRelation {
            index: 0,
            bangumi_work_id: bangumi_work,
            anilist_work_id: anilist_work,
        }
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "138714")
            .expect("lookup should work"),
        Some(anilist_work),
        "failed manual import must not move existing AniList edge"
    );
}

#[test]
fn ignore_entries_import_as_negative_mappings_without_identity_edges() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let imported = import_legacy_ignore_entries(
        &store,
        MediaKind::Anime,
        r#"{
            "bangumi": [274613],
            "anilist": [12345],
            "mal": [67890]
        }"#,
    )
    .expect("ignore entries should import");

    assert_eq!(imported, 3);

    assert_eq!(
        store
            .manual_mapping_decision(Provider::Bangumi, MediaKind::Anime, "274613")
            .expect("decision lookup should work"),
        Some(ManualMappingDecision::Ignore)
    );
    assert_eq!(
        store
            .manual_mapping_decision(Provider::AniList, MediaKind::Anime, "12345")
            .expect("decision lookup should work"),
        Some(ManualMappingDecision::Ignore)
    );
    assert_eq!(
        store
            .manual_mapping_decision(Provider::MyAnimeList, MediaKind::Anime, "67890")
            .expect("decision lookup should work"),
        Some(ManualMappingDecision::Ignore)
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "274613")
            .expect("external id lookup should work"),
        None
    );
}

#[test]
fn manual_relation_entries_must_be_bangumi_anilist_pairs() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let error = import_legacy_manual_relations(&store, MediaKind::Anime, "[[348335,138714,1]]")
        .expect_err("triple relation should be rejected");

    assert_eq!(
        error,
        LegacyImportError::InvalidManualRelationLength { index: 0, len: 3 }
    );
}
