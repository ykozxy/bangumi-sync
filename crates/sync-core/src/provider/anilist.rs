use serde::Deserialize;

use crate::model::{MediaKind, Provider};

use super::fixture::{
    collection_entry, invalid_json, progress_for_kind, score_from_fractional_hundred_point,
    status_from_provider_value, ProviderCollectionSnapshot, ProviderFixtureError,
    ProviderSnapshotIdentityLink,
};

const PROVIDER: Provider = Provider::AniList;

#[derive(Debug, Deserialize)]
struct AniListFixture {
    data: AniListData,
}

#[derive(Debug, Deserialize)]
struct AniListData {
    #[serde(rename = "MediaListCollection")]
    media_list_collection: AniListMediaListCollection,
}

#[derive(Debug, Deserialize)]
struct AniListMediaListCollection {
    lists: Vec<AniListList>,
}

#[derive(Debug, Deserialize)]
struct AniListList {
    entries: Vec<AniListEntry>,
}

#[derive(Debug, Deserialize)]
struct AniListEntry {
    #[serde(rename = "mediaId")]
    media_id: Option<u64>,
    media: AniListMedia,
    status: String,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    progress: u32,
    #[serde(default, rename = "progressVolumes")]
    progress_volumes: Option<u32>,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AniListMedia {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default, rename = "idMal")]
    id_mal: Option<u64>,
    #[serde(rename = "type")]
    media_type: String,
}

pub fn parse_anilist_collection_fixture(
    json: &str,
    expected_media_kind: MediaKind,
) -> Result<ProviderCollectionSnapshot, ProviderFixtureError> {
    let fixture: AniListFixture =
        serde_json::from_str(json).map_err(|error| invalid_json(PROVIDER, error))?;
    let mut entries = Vec::new();
    let mut identity_links = Vec::new();

    for list in fixture.data.media_list_collection.lists {
        for item in list.entries {
            // Existing sync identity uses AniList media id; list-entry ids can be added when write planning needs them.
            let provider_entry_id = item
                .media_id
                .or(item.media.id)
                .map(|id| id.to_string())
                .unwrap_or_default();
            let actual_media_kind = anilist_media_kind(&item.media.media_type, &provider_entry_id)?;
            if actual_media_kind != expected_media_kind {
                return Err(ProviderFixtureError::MediaKindMismatch {
                    provider: PROVIDER,
                    expected: expected_media_kind,
                    actual: actual_media_kind,
                    provider_entry_id,
                });
            }

            if let Some(id_mal) = item.media.id_mal {
                identity_links.push(ProviderSnapshotIdentityLink::new(
                    Provider::AniList,
                    expected_media_kind,
                    provider_entry_id.clone(),
                    Provider::MyAnimeList,
                    expected_media_kind,
                    id_mal.to_string(),
                    None,
                ));
            }

            let status = status_from_provider_value(PROVIDER, &provider_entry_id, &item.status)?;
            let score =
                score_from_fractional_hundred_point(PROVIDER, &provider_entry_id, item.score)?;
            let progress =
                progress_for_kind(expected_media_kind, item.progress, item.progress_volumes);
            let entry = collection_entry(
                PROVIDER,
                provider_entry_id,
                expected_media_kind,
                status,
                score,
                progress,
            )?
            .with_provider_updated_at_epoch_secs(item.updated_at);
            entries.push(entry);
        }
    }

    Ok(
        ProviderCollectionSnapshot::new(PROVIDER, expected_media_kind, json, entries)
            .with_identity_links(identity_links),
    )
}

fn anilist_media_kind(
    value: &str,
    provider_entry_id: &str,
) -> Result<MediaKind, ProviderFixtureError> {
    match value {
        "ANIME" => Ok(MediaKind::Anime),
        "MANGA" => Ok(MediaKind::Manga),
        other => Err(ProviderFixtureError::UnsupportedMediaKind {
            provider: PROVIDER,
            provider_entry_id: provider_entry_id.to_string(),
            media_kind: other.to_string(),
        }),
    }
}
