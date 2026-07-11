use sync_core::{
    model::{CollectionStatus, MediaKind, Provider},
    provider::{
        parse_anilist_collection_fixture, parse_bangumi_collection_fixture,
        parse_myanimelist_collection_fixture, ProviderFixtureError,
    },
};

#[test]
fn bangumi_fixture_normalizes_anime_and_manga_entries() {
    let anime_fixture = r#"{
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

    let anime = parse_bangumi_collection_fixture(anime_fixture, MediaKind::Anime).unwrap();
    assert_eq!(anime.provider(), Provider::Bangumi);
    assert_eq!(anime.media_kind(), MediaKind::Anime);
    assert!(anime.raw_payload_hash().starts_with("fnv1a64:"));
    assert!(anime
        .audit()
        .iter()
        .any(|line| line.contains("bangumi:253")));

    let entry = &anime.entries()[0];
    assert_eq!(entry.provider_entry_id(), "253");
    assert_eq!(entry.status(), CollectionStatus::Completed);
    assert_eq!(entry.score().unwrap().as_hundred_point(), 100);
    assert_eq!(entry.progress().episodes(), Some(26));
    assert_eq!(entry.progress().chapters(), None);
    assert_eq!(entry.progress().volumes(), None);

    let manga_fixture = r#"{
        "data": [
            {
                "subject_id": 9001,
                "subject_type": "book",
                "collection_type": "do",
                "rate": 8,
                "ep_status": 12,
                "vol_status": 3,
                "updated_at": "2026-06-02T00:00:00Z"
            }
        ]
    }"#;

    let manga = parse_bangumi_collection_fixture(manga_fixture, MediaKind::Manga).unwrap();
    assert_eq!(manga.provider(), Provider::Bangumi);
    assert_eq!(manga.media_kind(), MediaKind::Manga);

    let entry = &manga.entries()[0];
    assert_eq!(entry.provider_entry_id(), "9001");
    assert_eq!(entry.status(), CollectionStatus::InProgress);
    assert_eq!(entry.score().unwrap().as_hundred_point(), 80);
    assert_eq!(entry.progress().episodes(), None);
    assert_eq!(entry.progress().chapters(), Some(12));
    assert_eq!(entry.progress().volumes(), Some(3));
}

#[test]
fn anilist_fixture_normalizes_anime_and_manga_entries() {
    let anime_fixture = r#"{
        "data": {
            "MediaListCollection": {
                "lists": [
                    {
                        "entries": [
                            {
                                "mediaId": 1,
                                "media": { "type": "ANIME", "idMal": 5114 },
                                "status": "COMPLETED",
                                "score": 95,
                                "progress": 26,
                                "progressVolumes": null,
                                "updatedAt": 1780272000
                            }
                        ]
                    }
                ]
            }
        }
    }"#;

    let anime = parse_anilist_collection_fixture(anime_fixture, MediaKind::Anime).unwrap();
    assert_eq!(anime.provider(), Provider::AniList);
    assert_eq!(anime.media_kind(), MediaKind::Anime);
    assert!(anime.raw_payload_hash().starts_with("fnv1a64:"));
    assert!(anime.audit().iter().any(|line| line.contains("anilist:1")));
    assert_eq!(anime.identity_links().len(), 1);
    let link = &anime.identity_links()[0];
    assert_eq!(link.source_provider(), Provider::AniList);
    assert_eq!(link.source_media_kind(), MediaKind::Anime);
    assert_eq!(link.source_external_id(), "1");
    assert_eq!(link.target_provider(), Provider::MyAnimeList);
    assert_eq!(link.target_media_kind(), MediaKind::Anime);
    assert_eq!(link.target_external_id(), "5114");

    let entry = &anime.entries()[0];
    assert_eq!(entry.provider_entry_id(), "1");
    assert_eq!(entry.status(), CollectionStatus::Completed);
    assert_eq!(entry.score().unwrap().as_hundred_point(), 95);
    assert_eq!(entry.progress().episodes(), Some(26));
    assert_eq!(entry.provider_updated_at_epoch_secs(), Some(1_780_272_000));

    let manga_fixture = r#"{
        "data": {
            "MediaListCollection": {
                "lists": [
                    {
                        "entries": [
                            {
                                "mediaId": 2,
                                "media": { "type": "MANGA" },
                                "status": "CURRENT",
                                "score": 87.5,
                                "progress": 12,
                                "progressVolumes": null,
                                "updatedAt": 1780358400
                            }
                        ]
                    }
                ]
            }
        }
    }"#;

    let manga = parse_anilist_collection_fixture(manga_fixture, MediaKind::Manga).unwrap();
    assert_eq!(manga.provider(), Provider::AniList);
    assert_eq!(manga.media_kind(), MediaKind::Manga);

    let entry = &manga.entries()[0];
    assert_eq!(entry.provider_entry_id(), "2");
    assert_eq!(entry.status(), CollectionStatus::InProgress);
    assert_eq!(entry.score().unwrap().as_hundred_point(), 88);
    assert_eq!(entry.progress().episodes(), None);
    assert_eq!(entry.progress().chapters(), Some(12));
    assert_eq!(entry.progress().volumes(), None);
}

#[test]
fn myanimelist_fixture_normalizes_anime_and_manga_entries() {
    let anime_fixture = r#"{
        "data": [
            {
                "node": { "id": 5114, "media_type": "anime" },
                "list_status": {
                    "status": "completed",
                    "score": 10,
                    "num_episodes_watched": 64,
                    "updated_at": "2026-06-01T00:00:00Z"
                }
            }
        ]
    }"#;

    let anime = parse_myanimelist_collection_fixture(anime_fixture, MediaKind::Anime).unwrap();
    assert_eq!(anime.provider(), Provider::MyAnimeList);
    assert_eq!(anime.media_kind(), MediaKind::Anime);
    assert!(anime.raw_payload_hash().starts_with("fnv1a64:"));
    assert!(anime
        .audit()
        .iter()
        .any(|line| line.contains("myanimelist:5114")));

    let entry = &anime.entries()[0];
    assert_eq!(entry.provider_entry_id(), "5114");
    assert_eq!(entry.status(), CollectionStatus::Completed);
    assert_eq!(entry.score().unwrap().as_hundred_point(), 100);
    assert_eq!(entry.progress().episodes(), Some(64));
    assert_eq!(entry.provider_updated_at_epoch_secs(), Some(1_780_272_000));

    let manga_fixture = r#"{
        "data": [
            {
                "node": { "id": 30013, "media_type": "manhwa" },
                "list_status": {
                    "status": "reading",
                    "score": 8,
                    "num_chapters_read": 12,
                    "num_volumes_read": 3,
                    "updated_at": "2026-06-02T00:00:00Z"
                }
            }
        ]
    }"#;

    let manga = parse_myanimelist_collection_fixture(manga_fixture, MediaKind::Manga).unwrap();
    assert_eq!(manga.provider(), Provider::MyAnimeList);
    assert_eq!(manga.media_kind(), MediaKind::Manga);

    let entry = &manga.entries()[0];
    assert_eq!(entry.provider_entry_id(), "30013");
    assert_eq!(entry.status(), CollectionStatus::InProgress);
    assert_eq!(entry.score().unwrap().as_hundred_point(), 80);
    assert_eq!(entry.progress().episodes(), None);
    assert_eq!(entry.progress().chapters(), Some(12));
    assert_eq!(entry.progress().volumes(), Some(3));
}

#[test]
fn provider_fixtures_reject_media_kind_mismatches_and_bad_scores() {
    let bangumi_book_fixture = r#"{
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
    }"#;

    let err = parse_bangumi_collection_fixture(bangumi_book_fixture, MediaKind::Anime).unwrap_err();
    assert_eq!(
        err,
        ProviderFixtureError::MediaKindMismatch {
            provider: Provider::Bangumi,
            expected: MediaKind::Anime,
            actual: MediaKind::Manga,
            provider_entry_id: "9001".to_string(),
        }
    );

    let mal_bad_score_fixture = r#"{
        "data": [
            {
                "node": { "id": 5114, "media_type": "anime" },
                "list_status": {
                    "status": "completed",
                    "score": 11,
                    "num_episodes_watched": 64
                }
            }
        ]
    }"#;

    let err =
        parse_myanimelist_collection_fixture(mal_bad_score_fixture, MediaKind::Anime).unwrap_err();
    assert_eq!(
        err,
        ProviderFixtureError::InvalidScore {
            provider: Provider::MyAnimeList,
            provider_entry_id: "5114".to_string(),
            value: 110,
        }
    );
}
