use sync_core::identity::{
    auto_link_provider_item_match, import_anilist_id_mal_crosswalk, import_anime_offline_database,
    import_bangumi_dataset, match_provider_items, match_provider_items_with_metadata,
    select_auto_match, AnilistMalCrosswalk, AutoMatchDecision, CrosswalkImportError,
};
use sync_core::model::{CollectionStatus, MediaKind, Provider};
use sync_core::provider::parse_bangumi_collection_fixture;
use sync_core::store::{
    ExternalIdEdgeInput, ManualMappingDecision, ManualMappingInput, ProviderItemInput, SqliteStore,
};
use sync_core::sync::{plan_dry_run, PlannedActionKind};

#[test]
fn anilist_id_mal_crosswalk_imports_strong_edges() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let imported = import_anilist_id_mal_crosswalk(
        &store,
        MediaKind::Anime,
        &[AnilistMalCrosswalk {
            anilist_id: "1".to_owned(),
            myanimelist_id: "5".to_owned(),
            title: Some("Cowboy Bebop".to_owned()),
        }],
    )
    .expect("crosswalk should import");

    assert_eq!(imported, 1);
    let anilist_work = store
        .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
        .expect("anilist lookup should work")
        .expect("anilist edge should exist");
    let myanimelist_work = store
        .find_work_by_external_id(Provider::MyAnimeList, MediaKind::Anime, "5")
        .expect("myanimelist lookup should work")
        .expect("myanimelist edge should exist");

    assert_eq!(anilist_work, myanimelist_work);
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        1,
        "one crosswalk row should create one identity work"
    );
    let anilist_edge = store
        .external_id_edge_details(Provider::AniList, MediaKind::Anime, "1")
        .expect("edge lookup should work")
        .expect("anilist edge should exist");
    let myanimelist_edge = store
        .external_id_edge_details(Provider::MyAnimeList, MediaKind::Anime, "5")
        .expect("edge lookup should work")
        .expect("myanimelist edge should exist");

    assert_eq!(anilist_edge.confidence, 1000);
    assert_eq!(myanimelist_edge.confidence, 1000);
    assert_eq!(anilist_edge.match_method, "anilist-id-mal");
    assert_eq!(myanimelist_edge.match_method, "anilist-id-mal");
}

#[test]
fn anilist_id_mal_crosswalk_is_idempotent() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let row = AnilistMalCrosswalk {
        anilist_id: "1".to_owned(),
        myanimelist_id: "5".to_owned(),
        title: Some("Cowboy Bebop".to_owned()),
    };

    import_anilist_id_mal_crosswalk(&store, MediaKind::Anime, &[row.clone()])
        .expect("first import should work");
    import_anilist_id_mal_crosswalk(&store, MediaKind::Anime, &[row])
        .expect("second import should work");

    assert_eq!(
        store.identity_work_count().expect("count should work"),
        1,
        "re-importing a crosswalk row should not create duplicate works"
    );
}

#[test]
fn anilist_id_mal_crosswalk_rejects_conflicting_existing_works() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let anilist_work = store
        .create_identity_work(MediaKind::Anime, "AniList work")
        .expect("identity work should insert");
    let myanimelist_work = store
        .create_identity_work(MediaKind::Anime, "MyAnimeList work")
        .expect("identity work should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: anilist_work,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: None,
        })
        .expect("anilist edge should insert");
    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: myanimelist_work,
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Anime,
            external_id: "5".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: None,
        })
        .expect("myanimelist edge should insert");

    let error = import_anilist_id_mal_crosswalk(
        &store,
        MediaKind::Anime,
        &[AnilistMalCrosswalk {
            anilist_id: "1".to_owned(),
            myanimelist_id: "5".to_owned(),
            title: None,
        }],
    )
    .expect_err("conflicting works should be rejected");

    assert_eq!(
        error,
        CrosswalkImportError::ConflictingExistingWorks {
            anilist_work_id: anilist_work,
            myanimelist_work_id: myanimelist_work
        }
    );
}

#[test]
fn anilist_id_mal_crosswalk_preflights_conflicts_before_writing_rows() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let anilist_work = store
        .create_identity_work(MediaKind::Anime, "AniList work")
        .expect("identity work should insert");
    let myanimelist_work = store
        .create_identity_work(MediaKind::Anime, "MyAnimeList work")
        .expect("identity work should insert");

    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: anilist_work,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: None,
        })
        .expect("anilist edge should insert");
    store
        .upsert_external_id_edge(ExternalIdEdgeInput {
            work_id: myanimelist_work,
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Anime,
            external_id: "5".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: None,
        })
        .expect("myanimelist edge should insert");

    let error = import_anilist_id_mal_crosswalk(
        &store,
        MediaKind::Anime,
        &[
            AnilistMalCrosswalk {
                anilist_id: "100".to_owned(),
                myanimelist_id: "500".to_owned(),
                title: Some("Should Not Be Written".to_owned()),
            },
            AnilistMalCrosswalk {
                anilist_id: "1".to_owned(),
                myanimelist_id: "5".to_owned(),
                title: None,
            },
        ],
    )
    .expect_err("batch with conflict should be rejected before writes");

    assert_eq!(
        error,
        CrosswalkImportError::ConflictingExistingWorks {
            anilist_work_id: anilist_work,
            myanimelist_work_id: myanimelist_work
        }
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "100")
            .expect("lookup should work"),
        None,
        "preflight failure must not leave partial crosswalk writes"
    );
}

#[test]
fn anilist_id_mal_crosswalk_rejects_batch_conflicting_anilist_id_before_writes() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let error = import_anilist_id_mal_crosswalk(
        &store,
        MediaKind::Anime,
        &[
            AnilistMalCrosswalk {
                anilist_id: "1".to_owned(),
                myanimelist_id: "5".to_owned(),
                title: Some("Cowboy Bebop".to_owned()),
            },
            AnilistMalCrosswalk {
                anilist_id: "1".to_owned(),
                myanimelist_id: "6".to_owned(),
                title: Some("Conflicting Cowboy Bebop".to_owned()),
            },
        ],
    )
    .expect_err("same batch with conflicting AniList rows should be rejected");

    assert_eq!(
        error,
        CrosswalkImportError::ConflictingBatchRows {
            provider: Provider::AniList,
            external_id: "1".to_owned()
        }
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
            .expect("lookup should work"),
        None,
        "batch conflict must not leave partial crosswalk writes"
    );
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        0,
        "batch conflict must not create orphan identity works"
    );
}

#[test]
fn anilist_id_mal_crosswalk_rejects_batch_conflicting_mal_id_before_writes() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let error = import_anilist_id_mal_crosswalk(
        &store,
        MediaKind::Anime,
        &[
            AnilistMalCrosswalk {
                anilist_id: "1".to_owned(),
                myanimelist_id: "5".to_owned(),
                title: Some("Cowboy Bebop".to_owned()),
            },
            AnilistMalCrosswalk {
                anilist_id: "2".to_owned(),
                myanimelist_id: "5".to_owned(),
                title: Some("Conflicting Cowboy Bebop".to_owned()),
            },
        ],
    )
    .expect_err("same batch with conflicting MAL rows should be rejected");

    assert_eq!(
        error,
        CrosswalkImportError::ConflictingBatchRows {
            provider: Provider::MyAnimeList,
            external_id: "5".to_owned()
        }
    );
    assert_eq!(
        store
            .find_work_by_external_id(Provider::MyAnimeList, MediaKind::Anime, "5")
            .expect("lookup should work"),
        None,
        "batch conflict must not leave partial crosswalk writes"
    );
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        0,
        "batch conflict must not create orphan identity works"
    );
}

#[test]
fn anilist_id_mal_crosswalk_deduplicates_identical_batch_rows() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let row = AnilistMalCrosswalk {
        anilist_id: "1".to_owned(),
        myanimelist_id: "5".to_owned(),
        title: Some("Cowboy Bebop".to_owned()),
    };

    let imported = import_anilist_id_mal_crosswalk(&store, MediaKind::Anime, &[row.clone(), row])
        .expect("duplicate identical batch rows should be idempotent");

    assert_eq!(imported, 1);
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        1,
        "duplicate identical rows should not create orphan identity works"
    );
}

#[test]
fn anime_offline_database_rejects_batch_crosswalk_conflicts_before_provider_item_writes() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let error = import_anime_offline_database(
        &store,
        r#"{
            "data": [
                {
                    "title": "Cowboy Bebop",
                    "synonyms": [],
                    "type": "TV",
                    "sources": [
                        "https://anilist.co/anime/1",
                        "https://myanimelist.net/anime/5/Cowboy_Bebop"
                    ]
                },
                {
                    "title": "Conflicting Cowboy Bebop",
                    "synonyms": [],
                    "type": "TV",
                    "sources": [
                        "https://anilist.co/anime/1",
                        "https://myanimelist.net/anime/6/Conflicting_Cowboy_Bebop"
                    ]
                }
            ]
        }"#,
    )
    .expect_err("batch crosswalk conflict should reject dataset import");

    assert!(matches!(
        error,
        sync_core::identity::DatasetImportError::Crosswalk(
            CrosswalkImportError::ConflictingBatchRows {
                provider: Provider::AniList,
                external_id
            }
        ) if external_id == "1"
    ));
    assert_eq!(
        store
            .provider_item_payload_hash(Provider::AniList, MediaKind::Anime, "1")
            .expect("payload hash lookup should work"),
        None,
        "dataset crosswalk conflict must not leave provider item writes"
    );
}

#[test]
fn local_match_candidates_have_confidence_and_skip_ignored_ids() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "975".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            aliases: vec!["カウボーイビバップ".to_owned()],
            format: Some("TV".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:975".to_owned(),
        })
        .expect("provider item should insert");
    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "976".to_owned(),
            canonical_title: "Cowboy Bebop Recap".to_owned(),
            aliases: Vec::new(),
            format: Some("SPECIAL".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:976".to_owned(),
        })
        .expect("provider item should insert");
    store
        .upsert_manual_mapping(ManualMappingInput {
            work_id: None,
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "976".to_owned(),
            decision: ManualMappingDecision::Ignore,
            note: Some("fixture ignore".to_owned()),
        })
        .expect("ignore should insert");

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop", 10)
        .expect("match should work");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].provider, Provider::Bangumi);
    assert_eq!(candidates[0].external_id, "975");
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
}

#[test]
fn local_match_accepts_unique_exact_alias_candidate() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "975".to_owned(),
            canonical_title: "カウボーイビバップ".to_owned(),
            aliases: vec!["Cowboy Bebop".to_owned()],
            format: Some("TV".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:975".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-alias-exact");
    assert_eq!(
        decision,
        AutoMatchDecision::Accepted {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "975".to_owned(),
            confidence: 900,
            match_method: "fts-alias-exact".to_owned(),
        }
    );
}

#[test]
fn local_match_keeps_duplicate_exact_aliases_in_review() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (external_id, canonical_title) in
        [("975", "カウボーイビバップ"), ("976", "Cowboy Bebop Recap")]
    {
        store
            .upsert_provider_item(ProviderItemInput {
                provider: Provider::Bangumi,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: canonical_title.to_owned(),
                aliases: vec!["Cowboy Bebop".to_owned()],
                format: Some("TV".to_owned()),
                release_year: None,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.confidence == 900));
    assert!(candidates
        .iter()
        .all(|candidate| candidate.match_method == "fts-alias-exact"));
    assert_eq!(
        decision,
        AutoMatchDecision::NeedsReview {
            reason: "ambiguous-top-confidence".to_owned()
        }
    );
}

#[test]
fn local_match_keeps_risky_format_alias_exact_candidate_in_review() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (external_id, format) in [
        ("976", "SPECIAL"),
        ("977", "OVA"),
        ("978", "ONA"),
        ("979", "MOVIE"),
    ] {
        store
            .upsert_provider_item(ProviderItemInput {
                provider: Provider::Bangumi,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: format!("Cowboy Bebop {format}"),
                aliases: vec!["Cowboy Bebop".to_owned()],
                format: Some(format.to_owned()),
                release_year: None,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 4);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.confidence == 800));
    assert!(candidates
        .iter()
        .all(|candidate| candidate.match_method == "fts-alias-exact-risky-format"));
    assert_eq!(
        decision,
        AutoMatchDecision::NeedsReview {
            reason: "low-confidence".to_owned()
        }
    );
}

#[test]
fn local_match_keeps_unknown_format_alias_exact_candidate_in_review() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (external_id, format) in [
        ("unknown-format-none", None),
        ("unknown-format-empty", Some("")),
        ("unknown-format-blank", Some("   ")),
        ("unknown-format-music", Some("MUSIC")),
        ("unknown-format-pv", Some("PV")),
    ] {
        store
            .upsert_provider_item(ProviderItemInput {
                provider: Provider::Bangumi,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: format!("Cowboy Bebop Side Story {external_id}"),
                aliases: vec!["Cowboy Bebop".to_owned()],
                format: format.map(str::to_owned),
                release_year: None,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 5);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.confidence == 800));
    assert!(candidates
        .iter()
        .all(|candidate| candidate.match_method == "fts-alias-exact-unknown-format"));
    assert_eq!(
        decision,
        AutoMatchDecision::NeedsReview {
            reason: "low-confidence".to_owned()
        }
    );
}

#[test]
fn local_match_prefers_canonical_exact_series_over_risky_alias_exact_extra() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (external_id, canonical_title, aliases, format) in [
        ("series", "Cowboy Bebop", Vec::new(), Some("TV")),
        (
            "recap",
            "Cowboy Bebop Recap",
            vec!["Cowboy Bebop".to_owned()],
            Some("SPECIAL"),
        ),
    ] {
        store
            .upsert_provider_item(ProviderItemInput {
                provider: Provider::Bangumi,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: canonical_title.to_owned(),
                aliases,
                format: format.map(str::to_owned),
                release_year: None,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].external_id, "series");
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert_eq!(candidates[1].external_id, "recap");
    assert_eq!(candidates[1].confidence, 800);
    assert_eq!(
        decision,
        AutoMatchDecision::Accepted {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "series".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
        }
    );
}

#[test]
fn local_match_accepts_risky_format_when_canonical_title_is_exact() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "976".to_owned(),
            canonical_title: "Cowboy Bebop Recap".to_owned(),
            aliases: vec!["Cowboy Bebop".to_owned()],
            format: Some("SPECIAL".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:976".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop Recap", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert_eq!(
        decision,
        AutoMatchDecision::Accepted {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "976".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
        }
    );
}

#[test]
fn local_match_accepts_canonical_title_when_punctuation_and_case_differ() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "bocchi".to_owned(),
            canonical_title: "Bocchi the Rock!".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: Some(2022),
            source_payload_hash: "sha256:bocchi".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items(&store, MediaKind::Anime, "bocchi: the rock", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert_eq!(
        decision,
        AutoMatchDecision::Accepted {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "bocchi".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
        }
    );
}

#[test]
fn local_match_accepts_canonical_title_with_apostrophe_after_safe_fts_lookup() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "jojo".to_owned(),
            canonical_title: "JoJo's Bizarre Adventure".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: Some(2012),
            source_payload_hash: "sha256:jojo".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items(&store, MediaKind::Anime, "Jojos Bizarre Adventure", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert!(matches!(decision, AutoMatchDecision::Accepted { .. }));
}

#[test]
fn local_match_accepts_canonical_title_with_multiply_sign_after_safe_fts_lookup() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "spy-family".to_owned(),
            canonical_title: "SPY×FAMILY".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: Some(2022),
            source_payload_hash: "sha256:spy-family".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items(&store, MediaKind::Anime, "Spy x Family", 10)
        .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert!(matches!(decision, AutoMatchDecision::Accepted { .. }));
}

#[test]
fn local_match_accepts_full_width_ascii_canonical_title_after_safe_fts_lookup() {
    let store = SqliteStore::open_in_memory().expect("store should open");

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

    let candidates = match_provider_items_with_metadata(
        &store,
        MediaKind::Anime,
        "Spy x Family",
        Some(2022),
        10,
    )
    .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "fullwidth-spy-family");
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert_eq!(
        decision,
        AutoMatchDecision::Accepted {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "fullwidth-spy-family".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
        }
    );
}

#[test]
fn local_match_accepts_full_width_ascii_alias_after_safe_fts_lookup() {
    let store = SqliteStore::open_in_memory().expect("store should open");

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

    let candidates = match_provider_items_with_metadata(
        &store,
        MediaKind::Anime,
        "Spy x Family",
        Some(2022),
        10,
    )
    .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "fullwidth-spy-family-alias");
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-alias-exact");
    assert_eq!(
        decision,
        AutoMatchDecision::Accepted {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "fullwidth-spy-family-alias".to_owned(),
            confidence: 900,
            match_method: "fts-alias-exact".to_owned(),
        }
    );
}

#[test]
fn local_match_accepts_latin_diacritic_folded_canonical_title() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "pokemon".to_owned(),
            canonical_title: "Pokémon".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: Some(1997),
            source_payload_hash: "sha256:pokemon".to_owned(),
        })
        .expect("provider item should insert");

    let candidates =
        match_provider_items_with_metadata(&store, MediaKind::Anime, "Pokemon", Some(1997), 10)
            .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "pokemon");
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert_eq!(
        decision,
        AutoMatchDecision::Accepted {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "pokemon".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
        }
    );
}

#[test]
fn local_match_accepts_decomposed_latin_diacritic_canonical_title() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "pokemon-decomposed".to_owned(),
            canonical_title: "Poke\u{301}mon".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: Some(1997),
            source_payload_hash: "sha256:pokemon-decomposed".to_owned(),
        })
        .expect("provider item should insert");

    let candidates =
        match_provider_items_with_metadata(&store, MediaKind::Anime, "Pokemon", Some(1997), 10)
            .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "pokemon-decomposed");
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert!(matches!(decision, AutoMatchDecision::Accepted { .. }));
}

#[test]
fn local_match_accepts_latin_diacritic_folded_alias_with_safe_format() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "pokemon-alias".to_owned(),
            canonical_title: "Pocket Monsters".to_owned(),
            aliases: vec!["Pokémon".to_owned()],
            format: Some("TV".to_owned()),
            release_year: Some(1997),
            source_payload_hash: "sha256:pokemon-alias".to_owned(),
        })
        .expect("provider item should insert");

    let candidates =
        match_provider_items_with_metadata(&store, MediaKind::Anime, "Pokemon", Some(1997), 10)
            .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "pokemon-alias");
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-alias-exact");
    assert!(matches!(decision, AutoMatchDecision::Accepted { .. }));
}

#[test]
fn local_match_accepts_unicode_roman_numeral_canonical_title() {
    let store = SqliteStore::open_in_memory().expect("store should open");

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

    let candidates = match_provider_items_with_metadata(
        &store,
        MediaKind::Anime,
        "Mob Psycho 100 II",
        Some(2019),
        10,
    )
    .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "mob-psycho-100-2");
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-title-exact");
    assert!(matches!(decision, AutoMatchDecision::Accepted { .. }));
}

#[test]
fn local_match_limit_is_applied_after_ignore_filtering_and_confidence_ranking() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (external_id, title) in [
        ("ignored", "A Cowboy Bebop"),
        ("exact", "Cowboy Bebop"),
        ("partial", "ZZ Cowboy Bebop"),
    ] {
        store
            .upsert_provider_item(ProviderItemInput {
                provider: Provider::Bangumi,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: title.to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year: None,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    store
        .upsert_manual_mapping(ManualMappingInput {
            work_id: None,
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "ignored".to_owned(),
            decision: ManualMappingDecision::Ignore,
            note: Some("fixture ignore".to_owned()),
        })
        .expect("ignore should insert");

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop", 1)
        .expect("match should work");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "exact");
    assert_eq!(candidates[0].confidence, 900);
}

#[test]
fn local_match_filtering_and_ranking_consider_all_fts_matches_before_limit() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for index in 0..60 {
        let external_id = format!("ignored-{index:02}");
        store
            .upsert_provider_item(ProviderItemInput {
                provider: Provider::Bangumi,
                media_kind: MediaKind::Anime,
                external_id: external_id.clone(),
                canonical_title: format!("A{index:02} Cowboy Bebop"),
                aliases: Vec::new(),
                format: Some("SPECIAL".to_owned()),
                release_year: None,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
        store
            .upsert_manual_mapping(ManualMappingInput {
                work_id: None,
                provider: Provider::Bangumi,
                media_kind: MediaKind::Anime,
                external_id,
                decision: ManualMappingDecision::Ignore,
                note: Some("fixture ignore".to_owned()),
            })
            .expect("ignore should insert");
    }

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "exact".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:exact".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items(&store, MediaKind::Anime, "Cowboy Bebop", 1)
        .expect("match should work");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id, "exact");
    assert_eq!(candidates[0].confidence, 900);
}

#[test]
fn local_match_uses_release_year_to_accept_alias_exact_candidate() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "975".to_owned(),
            canonical_title: "カウボーイビバップ".to_owned(),
            aliases: vec!["Cowboy Bebop".to_owned()],
            format: Some("TV".to_owned()),
            release_year: Some(1998),
            source_payload_hash: "sha256:975".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items_with_metadata(
        &store,
        MediaKind::Anime,
        "Cowboy Bebop",
        Some(1998),
        10,
    )
    .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 900);
    assert_eq!(candidates[0].match_method, "fts-alias-exact");
    assert!(matches!(decision, AutoMatchDecision::Accepted { .. }));
}

#[test]
fn auto_link_provider_item_match_creates_edges_for_unique_catalog_candidates() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (provider, external_id) in [
        (Provider::Bangumi, "253"),
        (Provider::AniList, "1"),
        (Provider::MyAnimeList, "5"),
    ] {
        store
            .upsert_provider_item(ProviderItemInput {
                provider,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: "Cowboy Bebop".to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year: Some(1998),
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let report = auto_link_provider_item_match(&store, Provider::Bangumi, MediaKind::Anime, "253")
        .expect("auto-link should run");

    assert_eq!(report.work_id, Some(1));
    assert_eq!(report.linked_edges.len(), 3);

    let bangumi_work = store
        .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "253")
        .expect("lookup should work");
    let anilist_work = store
        .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
        .expect("lookup should work");
    let mal_work = store
        .find_work_by_external_id(Provider::MyAnimeList, MediaKind::Anime, "5")
        .expect("lookup should work");
    assert_eq!(bangumi_work, report.work_id);
    assert_eq!(anilist_work, report.work_id);
    assert_eq!(mal_work, report.work_id);

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
        .expect("snapshot should import");

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");
    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.actions().len(), 2);
    assert!(plan.actions().iter().all(|action| {
        action.kind == PlannedActionKind::AddEntry
            && action.source_provider == Provider::Bangumi
            && action.status == CollectionStatus::Completed
    }));
    assert!(plan.actions().iter().any(|action| {
        action.target_provider == Provider::AniList && action.target_provider_entry_id == "1"
    }));
    assert!(plan.actions().iter().any(|action| {
        action.target_provider == Provider::MyAnimeList && action.target_provider_entry_id == "5"
    }));
}

#[test]
fn auto_link_provider_item_match_leaves_duplicate_provider_candidates_for_review() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (provider, external_id) in [
        (Provider::Bangumi, "253"),
        (Provider::AniList, "1"),
        (Provider::AniList, "2"),
    ] {
        store
            .upsert_provider_item(ProviderItemInput {
                provider,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: "Cowboy Bebop".to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year: Some(1998),
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let report = auto_link_provider_item_match(&store, Provider::Bangumi, MediaKind::Anime, "253")
        .expect("auto-link should run");

    assert_eq!(report.linked_edges.len(), 0);
    assert_eq!(
        report.review_reason.as_deref(),
        Some("ambiguous-provider-candidates")
    );
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        0,
        "ambiguous auto-link should not create partial identity work"
    );
}

#[test]
fn auto_link_provider_item_match_reviews_conflicting_candidate_release_years() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (provider, external_id, release_year) in [
        (Provider::Bangumi, "253", None),
        (Provider::AniList, "1", Some(1998)),
        (Provider::MyAnimeList, "5", Some(2021)),
    ] {
        store
            .upsert_provider_item(ProviderItemInput {
                provider,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: "Cowboy Bebop".to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let report = auto_link_provider_item_match(&store, Provider::Bangumi, MediaKind::Anime, "253")
        .expect("auto-link should run");

    assert_eq!(report.linked_edges.len(), 0);
    assert_eq!(
        report.review_reason.as_deref(),
        Some("conflicting-candidate-release-years")
    );
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        0,
        "conflicting auto-link should not create partial identity work"
    );
}

#[test]
fn auto_link_provider_item_match_reviews_mixed_known_and_unknown_target_years() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    for (provider, external_id, release_year) in [
        (Provider::Bangumi, "253", None),
        (Provider::AniList, "1", Some(1998)),
        (Provider::MyAnimeList, "5", None),
    ] {
        store
            .upsert_provider_item(ProviderItemInput {
                provider,
                media_kind: MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: "Cowboy Bebop".to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let report = auto_link_provider_item_match(&store, Provider::Bangumi, MediaKind::Anime, "253")
        .expect("auto-link should run");

    assert_eq!(report.linked_edges.len(), 0);
    assert_eq!(
        report.review_reason.as_deref(),
        Some("incomplete-candidate-release-years")
    );
    assert_eq!(
        store.identity_work_count().expect("count should work"),
        0,
        "incomplete-year auto-link should not create partial identity work"
    );
}

#[test]
fn local_match_keeps_year_mismatched_title_exact_candidate_in_review() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "canonical".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: Some(1999),
            source_payload_hash: "sha256:canonical".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items_with_metadata(
        &store,
        MediaKind::Anime,
        "Cowboy Bebop",
        Some(1998),
        10,
    )
    .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 800);
    assert_eq!(candidates[0].match_method, "fts-title-exact-year-mismatch");
    assert_eq!(
        decision,
        AutoMatchDecision::NeedsReview {
            reason: "low-confidence".to_owned()
        }
    );
}

#[test]
fn local_match_keeps_unknown_year_title_exact_candidate_in_review_when_query_year_known() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "unknown-year-title".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            aliases: Vec::new(),
            format: Some("TV".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:unknown-year-title".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items_with_metadata(
        &store,
        MediaKind::Anime,
        "Cowboy Bebop",
        Some(1998),
        10,
    )
    .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 800);
    assert_eq!(candidates[0].match_method, "fts-title-exact-unknown-year");
    assert_eq!(
        decision,
        AutoMatchDecision::NeedsReview {
            reason: "low-confidence".to_owned()
        }
    );
}

#[test]
fn local_match_keeps_year_mismatched_alias_exact_candidate_in_review() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "remake".to_owned(),
            canonical_title: "Cowboy Bebop Remake".to_owned(),
            aliases: vec!["Cowboy Bebop".to_owned()],
            format: Some("TV".to_owned()),
            release_year: Some(2021),
            source_payload_hash: "sha256:remake".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items_with_metadata(
        &store,
        MediaKind::Anime,
        "Cowboy Bebop",
        Some(1998),
        10,
    )
    .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 800);
    assert_eq!(candidates[0].match_method, "fts-alias-exact-year-mismatch");
    assert_eq!(
        decision,
        AutoMatchDecision::NeedsReview {
            reason: "low-confidence".to_owned()
        }
    );
}

#[test]
fn local_match_keeps_unknown_year_alias_exact_candidate_in_review_when_query_year_known() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    store
        .upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "unknown-year".to_owned(),
            canonical_title: "Cowboy Bebop Unknown Year".to_owned(),
            aliases: vec!["Cowboy Bebop".to_owned()],
            format: Some("TV".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:unknown-year".to_owned(),
        })
        .expect("provider item should insert");

    let candidates = match_provider_items_with_metadata(
        &store,
        MediaKind::Anime,
        "Cowboy Bebop",
        Some(1998),
        10,
    )
    .expect("match should work");
    let decision = select_auto_match(&candidates);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].confidence, 800);
    assert_eq!(candidates[0].match_method, "fts-alias-exact-unknown-year");
    assert_eq!(
        decision,
        AutoMatchDecision::NeedsReview {
            reason: "low-confidence".to_owned()
        }
    );
}

#[test]
fn anime_offline_database_imports_provider_items_and_id_crosswalks() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let imported = import_anime_offline_database(
        &store,
        r#"{
            "data": [
                {
                    "title": "Cowboy Bebop",
                    "synonyms": ["カウボーイビバップ"],
                    "type": "TV",
                    "episodes": 26,
                    "status": "FINISHED",
                    "animeSeason": {"season": "SPRING", "year": 1998},
                    "sources": [
                        "https://anilist.co/anime/1",
                        "https://myanimelist.net/anime/1/Cowboy_Bebop"
                    ],
                    "relations": [],
                    "tags": []
                }
            ]
        }"#,
    )
    .expect("anime-offline fixture should import");

    assert_eq!(imported, 1);
    let anilist_work = store
        .find_work_by_external_id(Provider::AniList, MediaKind::Anime, "1")
        .expect("anilist lookup should work")
        .expect("anilist edge should exist");
    let myanimelist_work = store
        .find_work_by_external_id(Provider::MyAnimeList, MediaKind::Anime, "1")
        .expect("myanimelist lookup should work")
        .expect("myanimelist edge should exist");

    assert_eq!(anilist_work, myanimelist_work);
    assert_eq!(
        store
            .provider_item_payload_hash(Provider::AniList, MediaKind::Anime, "1")
            .expect("payload hash lookup should work")
            .as_deref(),
        Some("anime-offline:Cowboy Bebop")
    );

    let candidates = match_provider_items(&store, MediaKind::Anime, "カウボーイビバップ", 1)
        .expect("alias search should work");
    assert_eq!(candidates[0].provider, Provider::AniList);
    assert_eq!(candidates[0].external_id, "1");
    assert_eq!(candidates[0].release_year, Some(1998));
}

#[test]
fn anime_offline_database_preflights_crosswalk_conflicts_before_provider_item_writes() {
    let store = SqliteStore::open_in_memory().expect("store should open");
    let anilist_work = store
        .create_identity_work(MediaKind::Anime, "anilist-only")
        .expect("anilist work should create");
    let mal_work = store
        .create_identity_work(MediaKind::Anime, "mal-only")
        .expect("mal work should create");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: anilist_work,
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("anilist edge should insert");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: mal_work,
            provider: Provider::MyAnimeList,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");

    let error = import_anime_offline_database(
        &store,
        r#"{
            "data": [
                {
                    "title": "Cowboy Bebop",
                    "synonyms": ["カウボーイビバップ"],
                    "type": "TV",
                    "sources": [
                        "https://anilist.co/anime/1",
                        "https://myanimelist.net/anime/1/Cowboy_Bebop"
                    ]
                }
            ]
        }"#,
    )
    .expect_err("conflicting crosswalk should reject import");

    assert!(matches!(
        error,
        sync_core::identity::DatasetImportError::Crosswalk(
            sync_core::identity::CrosswalkImportError::ConflictingExistingWorks { .. }
        )
    ));
    assert_eq!(
        store
            .provider_item_payload_hash(Provider::AniList, MediaKind::Anime, "1")
            .expect("provider item lookup should work"),
        None,
        "failed anime-offline import must not leave provider_item rows behind"
    );
}

#[test]
fn bangumi_dataset_imports_provider_items_without_cross_media_edges() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    let imported = import_bangumi_dataset(
        &store,
        MediaKind::Anime,
        r#"{
            "items": [
                {
                    "title": "Cowboy Bebop",
                    "titleTranslate": {"ja": ["カウボーイビバップ"]},
                    "type": "tv",
                    "begin": "1998-04-03",
                    "sites": [{"site": "bangumi", "id": "253"}]
                }
            ]
        }"#,
    )
    .expect("bangumi fixture should import");

    assert_eq!(imported, 1);
    assert_eq!(
        store
            .provider_item_payload_hash(Provider::Bangumi, MediaKind::Anime, "253")
            .expect("payload hash lookup should work")
            .as_deref(),
        Some("bangumi-data:253")
    );
    let candidates = store
        .search_provider_items(MediaKind::Anime, "カウボーイビバップ", 10)
        .expect("bangumi provider item search should work");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].release_year, Some(1998));
    assert_eq!(
        store
            .find_work_by_external_id(Provider::Bangumi, MediaKind::Anime, "253")
            .expect("edge lookup should work"),
        None,
        "dataset provider item import should not invent identity edges"
    );
}

#[test]
fn dataset_import_auto_link_and_snapshot_import_feed_three_provider_plan() {
    let store = SqliteStore::open_in_memory().expect("store should open");

    import_anime_offline_database(
        &store,
        r#"{
            "data": [
                {
                    "title": "Cowboy Bebop",
                    "synonyms": ["カウボーイビバップ"],
                    "type": "TV",
                    "animeSeason": {"season": "SPRING", "year": 1998},
                    "sources": [
                        "https://anilist.co/anime/1",
                        "https://myanimelist.net/anime/1/Cowboy_Bebop"
                    ]
                }
            ]
        }"#,
    )
    .expect("anime-offline fixture should import");
    import_bangumi_dataset(
        &store,
        MediaKind::Anime,
        r#"{
            "items": [
                {
                    "title": "Cowboy Bebop",
                    "titleTranslate": {"ja": ["カウボーイビバップ"]},
                    "type": "tv",
                    "begin": "1998-04-03",
                    "sites": [{"site": "bangumi", "id": "253"}]
                }
            ]
        }"#,
    )
    .expect("bangumi-data fixture should import");

    let report = auto_link_provider_item_match(&store, Provider::Bangumi, MediaKind::Anime, "253")
        .expect("auto-link should run");
    assert_eq!(report.review_reason, None);
    assert_eq!(
        report.linked_edges.len(),
        1,
        "auto-link should only fill the missing Bangumi edge"
    );
    assert_eq!(report.linked_edges[0].provider, Provider::Bangumi);

    let anilist_edge = store
        .external_id_edge_details(Provider::AniList, MediaKind::Anime, "1")
        .expect("edge lookup should work")
        .expect("anilist idMal edge should still exist");
    assert_eq!(anilist_edge.source, "anilist idMal");
    assert_eq!(anilist_edge.confidence, 1000);
    assert_eq!(anilist_edge.match_method, "anilist-id-mal");

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
        .expect("snapshot should import");
    let entry = store
        .collection_entry_details(
            "fixture-account",
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
        )
        .expect("entry lookup should work")
        .expect("entry should exist");
    assert_eq!(entry.work_id, report.work_id);

    let plan = plan_dry_run(
        &store,
        "fixture-account",
        MediaKind::Anime,
        &[Provider::Bangumi, Provider::AniList, Provider::MyAnimeList],
    )
    .expect("plan should build");

    assert_eq!(plan.skipped_unmatched_count(), 0);
    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.actions().len(), 2);
    assert!(plan.actions().iter().any(|action| {
        action.target_provider == Provider::AniList && action.target_provider_entry_id == "1"
    }));
    assert!(plan.actions().iter().any(|action| {
        action.target_provider == Provider::MyAnimeList && action.target_provider_entry_id == "1"
    }));
}

#[test]
fn auto_match_rejects_ambiguous_or_low_confidence_candidates() {
    let ambiguous = vec![
        sync_core::identity::MatchCandidate {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            release_year: None,
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
        },
        sync_core::identity::MatchCandidate {
            provider: Provider::AniList,
            media_kind: MediaKind::Anime,
            external_id: "2".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            release_year: None,
            confidence: 900,
            match_method: "fts-title-exact".to_owned(),
        },
    ];
    assert_eq!(
        select_auto_match(&ambiguous),
        AutoMatchDecision::NeedsReview {
            reason: "ambiguous-top-confidence".to_owned()
        }
    );

    let low_confidence = vec![sync_core::identity::MatchCandidate {
        provider: Provider::Bangumi,
        media_kind: MediaKind::Anime,
        external_id: "3".to_owned(),
        canonical_title: "Cowboy Bebop Recap".to_owned(),
        release_year: None,
        confidence: 650,
        match_method: "fts-title".to_owned(),
    }];
    assert_eq!(
        select_auto_match(&low_confidence),
        AutoMatchDecision::NeedsReview {
            reason: "low-confidence".to_owned()
        }
    );
}

#[test]
fn auto_match_accepts_single_high_confidence_candidate() {
    let candidates = vec![sync_core::identity::MatchCandidate {
        provider: Provider::Bangumi,
        media_kind: MediaKind::Anime,
        external_id: "1".to_owned(),
        canonical_title: "Cowboy Bebop".to_owned(),
        release_year: None,
        confidence: 900,
        match_method: "fts-title-exact".to_owned(),
    }];

    assert_eq!(
        select_auto_match(&candidates),
        AutoMatchDecision::Accepted {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            external_id: "1".to_owned(),
            confidence: 900,
            match_method: "fts-title-exact".to_owned()
        }
    );
}
