use serde::Deserialize;

use crate::model::{MediaKind, Provider};

use super::fixture::{
    collection_entry, invalid_json, progress_for_kind, rfc3339_utc_epoch_seconds,
    score_from_ten_point, status_from_provider_value, ProviderCollectionSnapshot,
    ProviderFixtureError,
};

const PROVIDER: Provider = Provider::Bangumi;

#[derive(Debug, Deserialize)]
struct BangumiCollectionFixture {
    data: Vec<BangumiCollectionItem>,
}

#[derive(Debug, Deserialize)]
struct BangumiCollectionItem {
    subject_id: u64,
    subject_type: FlexibleValue,
    #[serde(alias = "type")]
    collection_type: FlexibleValue,
    #[serde(default)]
    rate: u16,
    #[serde(default)]
    ep_status: u32,
    #[serde(default)]
    vol_status: u32,
    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FlexibleValue {
    Number(u64),
    Text(String),
}

pub fn parse_bangumi_collection_fixture(
    json: &str,
    expected_media_kind: MediaKind,
) -> Result<ProviderCollectionSnapshot, ProviderFixtureError> {
    let fixture: BangumiCollectionFixture =
        serde_json::from_str(json).map_err(|error| invalid_json(PROVIDER, error))?;
    let mut entries = Vec::with_capacity(fixture.data.len());

    for item in fixture.data {
        let provider_entry_id = item.subject_id.to_string();
        let actual_media_kind = bangumi_subject_kind(&item.subject_type, &provider_entry_id)?;
        if actual_media_kind != expected_media_kind {
            return Err(ProviderFixtureError::MediaKindMismatch {
                provider: PROVIDER,
                expected: expected_media_kind,
                actual: actual_media_kind,
                provider_entry_id,
            });
        }

        let status_value = flexible_value_to_status_key(&item.collection_type);
        let status = status_from_provider_value(PROVIDER, &provider_entry_id, &status_value)?;
        let score = score_from_ten_point(PROVIDER, &provider_entry_id, item.rate)?;
        let provider_updated_at =
            rfc3339_utc_epoch_seconds(PROVIDER, &provider_entry_id, item.updated_at.as_deref())?;
        let progress =
            progress_for_kind(expected_media_kind, item.ep_status, Some(item.vol_status));
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

fn bangumi_subject_kind(
    value: &FlexibleValue,
    provider_entry_id: &str,
) -> Result<MediaKind, ProviderFixtureError> {
    match value {
        FlexibleValue::Number(1) => Ok(MediaKind::Manga),
        FlexibleValue::Number(2) => Ok(MediaKind::Anime),
        FlexibleValue::Number(number) => Err(ProviderFixtureError::UnsupportedMediaKind {
            provider: PROVIDER,
            provider_entry_id: provider_entry_id.to_string(),
            media_kind: number.to_string(),
        }),
        FlexibleValue::Text(text) => match text.as_str() {
            "book" | "manga" => Ok(MediaKind::Manga),
            "anime" => Ok(MediaKind::Anime),
            other => Err(ProviderFixtureError::UnsupportedMediaKind {
                provider: PROVIDER,
                provider_entry_id: provider_entry_id.to_string(),
                media_kind: other.to_string(),
            }),
        },
    }
}

fn flexible_value_to_status_key(value: &FlexibleValue) -> String {
    match value {
        FlexibleValue::Number(number) => number.to_string(),
        FlexibleValue::Text(text) => text.to_string(),
    }
}
