use sync_core::model::{
    CollectionEntry, CollectionEntryError, CollectionStatus, MediaKind, Progress, Provider, Score,
    ScoreError, SyncField,
};

#[test]
fn media_kind_rejects_cross_domain_identity_by_default() {
    assert_ne!(MediaKind::Anime, MediaKind::Manga);
    assert_eq!(MediaKind::Anime.as_str(), "anime");
    assert_eq!(MediaKind::Manga.as_str(), "manga");
}

#[test]
fn providers_have_stable_wire_names() {
    assert_eq!(Provider::Bangumi.as_str(), "bangumi");
    assert_eq!(Provider::AniList.as_str(), "anilist");
    assert_eq!(Provider::MyAnimeList.as_str(), "myanimelist");
}

#[test]
fn collection_statuses_distinguish_active_and_terminal_states() {
    assert!(CollectionStatus::InProgress.is_active());
    assert!(CollectionStatus::Completed.is_terminal());
    assert!(CollectionStatus::Dropped.is_terminal());
    assert!(!CollectionStatus::Planned.is_terminal());
}

#[test]
fn protected_sync_fields_are_not_writable_by_default() {
    let default_writable = [
        SyncField::Status,
        SyncField::Score,
        SyncField::ProgressEpisodes,
        SyncField::ProgressChapters,
        SyncField::ProgressVolumes,
    ];

    for field in default_writable {
        assert!(field.is_default_writable(), "{field:?} should be writable");
    }

    let protected = [
        SyncField::RepeatCount,
        SyncField::StartedAt,
        SyncField::CompletedAt,
        SyncField::Notes,
        SyncField::Tags,
    ];

    for field in protected {
        assert!(
            !field.is_default_writable(),
            "{field:?} should be protected"
        );
    }
}

#[test]
fn score_uses_hundred_point_internal_scale() {
    let minimum = Score::from_hundred_point(0).expect("zero score should be valid");
    let maximum = Score::from_hundred_point(100).expect("hundred score should be valid");

    assert_eq!(minimum.as_hundred_point(), 0);
    assert_eq!(maximum.as_hundred_point(), 100);
    assert_eq!(
        Score::from_hundred_point(101),
        Err(ScoreError::OutOfRange { value: 101 })
    );
}

#[test]
fn progress_fields_are_media_kind_specific() {
    let anime = Progress::anime(7);
    assert_eq!(anime.episodes(), Some(7));
    assert_eq!(anime.chapters(), None);
    assert_eq!(anime.volumes(), None);
    assert!(anime.is_compatible_with(MediaKind::Anime));
    assert!(!anime.is_compatible_with(MediaKind::Manga));

    let manga = Progress::manga(Some(12), Some(3));
    assert_eq!(manga.episodes(), None);
    assert_eq!(manga.chapters(), Some(12));
    assert_eq!(manga.volumes(), Some(3));
    assert!(manga.is_compatible_with(MediaKind::Manga));
    assert!(!manga.is_compatible_with(MediaKind::Anime));
}

#[test]
fn collection_entry_rejects_cross_media_progress() {
    let result = CollectionEntry::new(
        Provider::Bangumi,
        "123",
        MediaKind::Anime,
        CollectionStatus::InProgress,
        None,
        Progress::manga(Some(12), Some(3)),
    );

    assert_eq!(
        result,
        Err(CollectionEntryError::IncompatibleProgress {
            media_kind: MediaKind::Anime
        })
    );
}

#[test]
fn collection_entry_requires_provider_entry_id() {
    let result = CollectionEntry::new(
        Provider::AniList,
        "",
        MediaKind::Manga,
        CollectionStatus::Planned,
        Some(Score::from_hundred_point(80).expect("score should be valid")),
        Progress::none(),
    );

    assert_eq!(result, Err(CollectionEntryError::MissingProviderEntryId));
}
