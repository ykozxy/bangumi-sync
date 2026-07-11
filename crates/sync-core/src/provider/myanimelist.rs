use serde::Deserialize;

use crate::model::{MediaKind, Provider};

use super::fixture::{
    collection_entry, invalid_json, progress_for_kind, rfc3339_utc_epoch_seconds,
    score_from_ten_point, status_from_provider_value, ProviderCollectionSnapshot,
    ProviderFixtureError,
};

const PROVIDER: Provider = Provider::MyAnimeList;

#[derive(Debug, Deserialize)]
struct MyAnimeListFixture {
    data: Vec<MyAnimeListEntry>,
}

#[derive(Debug, Deserialize)]
struct MyAnimeListEntry {
    node: MyAnimeListNode,
    list_status: MyAnimeListStatus,
}

#[derive(Debug, Deserialize)]
struct MyAnimeListNode {
    id: u64,
    media_type: String,
}

#[derive(Debug, Deserialize)]
struct MyAnimeListStatus {
    status: String,
    #[serde(default)]
    score: u16,
    #[serde(default)]
    num_episodes_watched: u32,
    #[serde(default)]
    num_chapters_read: u32,
    #[serde(default)]
    num_volumes_read: u32,
    #[serde(default)]
    updated_at: Option<String>,
}

pub fn parse_myanimelist_collection_fixture(
    json: &str,
    expected_media_kind: MediaKind,
) -> Result<ProviderCollectionSnapshot, ProviderFixtureError> {
    let fixture: MyAnimeListFixture =
        serde_json::from_str(json).map_err(|error| invalid_json(PROVIDER, error))?;
    let mut entries = Vec::with_capacity(fixture.data.len());

    for item in fixture.data {
        let provider_entry_id = item.node.id.to_string();
        let actual_media_kind = myanimelist_media_kind(&item.node.media_type, &provider_entry_id)?;
        if actual_media_kind != expected_media_kind {
            return Err(ProviderFixtureError::MediaKindMismatch {
                provider: PROVIDER,
                expected: expected_media_kind,
                actual: actual_media_kind,
                provider_entry_id,
            });
        }

        let status =
            status_from_provider_value(PROVIDER, &provider_entry_id, &item.list_status.status)?;
        let score = score_from_ten_point(PROVIDER, &provider_entry_id, item.list_status.score)?;
        let provider_updated_at = rfc3339_utc_epoch_seconds(
            PROVIDER,
            &provider_entry_id,
            item.list_status.updated_at.as_deref(),
        )?;
        let progress = match expected_media_kind {
            MediaKind::Anime => progress_for_kind(
                expected_media_kind,
                item.list_status.num_episodes_watched,
                None,
            ),
            MediaKind::Manga => progress_for_kind(
                expected_media_kind,
                item.list_status.num_chapters_read,
                Some(item.list_status.num_volumes_read),
            ),
        };
        let entry = collection_entry(
            PROVIDER,
            provider_entry_id,
            expected_media_kind,
            status,
            score,
            progress,
        )?
        .with_provider_updated_at_epoch_secs(provider_updated_at);
        entries.push(entry);
    }

    Ok(ProviderCollectionSnapshot::new(
        PROVIDER,
        expected_media_kind,
        json,
        entries,
    ))
}

fn myanimelist_media_kind(
    value: &str,
    provider_entry_id: &str,
) -> Result<MediaKind, ProviderFixtureError> {
    match value {
        "anime" => Ok(MediaKind::Anime),
        "manga" | "novel" | "one_shot" | "doujinshi" | "manhwa" | "manhua" | "oel" => {
            Ok(MediaKind::Manga)
        }
        other => Err(ProviderFixtureError::UnsupportedMediaKind {
            provider: PROVIDER,
            provider_entry_id: provider_entry_id.to_string(),
            media_kind: other.to_string(),
        }),
    }
}
