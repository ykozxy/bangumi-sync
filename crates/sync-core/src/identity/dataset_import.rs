use serde::Deserialize;

use crate::model::{MediaKind, Provider};
use crate::store::{ProviderItemInput, SqliteStore, StoreError};

use super::crosswalk_import::validate_anilist_id_mal_crosswalk;
use super::{import_anilist_id_mal_crosswalk, AnilistMalCrosswalk, CrosswalkImportError};

#[derive(Debug)]
pub enum DatasetImportError {
    InvalidJson { message: String },
    Store(StoreError),
    Crosswalk(CrosswalkImportError),
}

#[derive(Debug, Deserialize)]
struct AnimeOfflineDatabase {
    data: Vec<AnimeOfflineItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnimeOfflineItem {
    title: String,
    #[serde(default)]
    synonyms: Vec<String>,
    #[serde(rename = "type")]
    media_format: Option<String>,
    #[serde(default)]
    anime_season: AnimeOfflineSeason,
    sources: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct AnimeOfflineSeason {
    year: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct BangumiDataset {
    items: Vec<BangumiDatasetItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BangumiDatasetItem {
    title: String,
    #[serde(default)]
    title_translate: BangumiTitleTranslate,
    #[serde(rename = "type")]
    media_format: Option<String>,
    begin: Option<String>,
    sites: Vec<BangumiSite>,
}

#[derive(Debug, Default, Deserialize)]
struct BangumiTitleTranslate {
    #[serde(default)]
    en: Vec<String>,
    #[serde(default)]
    ja: Vec<String>,
    #[serde(default, rename = "zh-Hans")]
    zh_hans: Vec<String>,
    #[serde(default, rename = "zh-Hant")]
    zh_hant: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BangumiSite {
    site: String,
    id: String,
}

pub fn import_anime_offline_database(
    store: &SqliteStore,
    json: &str,
) -> Result<usize, DatasetImportError> {
    let database = serde_json::from_str::<AnimeOfflineDatabase>(json)?;
    let items = database.data;
    let mut crosswalks = Vec::new();

    for item in &items {
        let anilist_id = item
            .sources
            .iter()
            .find_map(|source| capture_numeric_suffix(source, "anilist.co/anime/"));
        let myanimelist_id = item
            .sources
            .iter()
            .find_map(|source| capture_numeric_suffix(source, "myanimelist.net/anime/"));

        if let (Some(anilist_id), Some(myanimelist_id)) = (anilist_id, myanimelist_id) {
            crosswalks.push(AnilistMalCrosswalk {
                anilist_id,
                myanimelist_id,
                title: Some(item.title.clone()),
            });
        }
    }

    validate_anilist_id_mal_crosswalk(store, MediaKind::Anime, &crosswalks)?;

    let mut imported = 0;
    for item in items {
        let anilist_id = item
            .sources
            .iter()
            .find_map(|source| capture_numeric_suffix(source, "anilist.co/anime/"));
        let myanimelist_id = item
            .sources
            .iter()
            .find_map(|source| capture_numeric_suffix(source, "myanimelist.net/anime/"));

        if let Some(anilist_id) = &anilist_id {
            store.upsert_provider_item(ProviderItemInput {
                provider: Provider::AniList,
                media_kind: MediaKind::Anime,
                external_id: anilist_id.clone(),
                canonical_title: item.title.clone(),
                aliases: item.synonyms.clone(),
                format: item.media_format.clone(),
                release_year: item.anime_season.year,
                source_payload_hash: format!("anime-offline:{}", item.title),
            })?;
            imported += 1;
        }

        if let Some(myanimelist_id) = &myanimelist_id {
            store.upsert_provider_item(ProviderItemInput {
                provider: Provider::MyAnimeList,
                media_kind: MediaKind::Anime,
                external_id: myanimelist_id.clone(),
                canonical_title: item.title.clone(),
                aliases: item.synonyms.clone(),
                format: item.media_format.clone(),
                release_year: item.anime_season.year,
                source_payload_hash: format!("anime-offline:{}", item.title),
            })?;
        }
    }

    import_anilist_id_mal_crosswalk(store, MediaKind::Anime, &crosswalks)?;

    Ok(imported)
}

pub fn validate_anime_offline_database_json(json: &str) -> Result<usize, DatasetImportError> {
    let database = serde_json::from_str::<AnimeOfflineDatabase>(json)?;
    Ok(database.data.len())
}

pub fn import_bangumi_dataset(
    store: &SqliteStore,
    media_kind: MediaKind,
    json: &str,
) -> Result<usize, DatasetImportError> {
    let database = serde_json::from_str::<BangumiDataset>(json)?;
    let mut imported = 0;

    for item in database.items {
        let Some(bangumi_id) = item
            .sites
            .iter()
            .find(|site| site.site == "bangumi" && !site.id.trim().is_empty())
            .map(|site| site.id.clone())
        else {
            continue;
        };

        store.upsert_provider_item(ProviderItemInput {
            provider: Provider::Bangumi,
            media_kind,
            external_id: bangumi_id.clone(),
            canonical_title: item.title,
            aliases: item.title_translate.aliases(),
            format: item.media_format,
            release_year: release_year_from_date_prefix(item.begin.as_deref()),
            source_payload_hash: format!("bangumi-data:{bangumi_id}"),
        })?;
        imported += 1;
    }

    Ok(imported)
}

pub fn validate_bangumi_dataset_json(json: &str) -> Result<usize, DatasetImportError> {
    let database = serde_json::from_str::<BangumiDataset>(json)?;
    Ok(database.items.len())
}

impl BangumiTitleTranslate {
    fn aliases(self) -> Vec<String> {
        [self.en, self.ja, self.zh_hans, self.zh_hant]
            .into_iter()
            .flatten()
            .collect()
    }
}

impl From<serde_json::Error> for DatasetImportError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidJson {
            message: error.to_string(),
        }
    }
}

impl From<StoreError> for DatasetImportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<CrosswalkImportError> for DatasetImportError {
    fn from(error: CrosswalkImportError) -> Self {
        Self::Crosswalk(error)
    }
}

fn release_year_from_date_prefix(value: Option<&str>) -> Option<u16> {
    let value = value?;
    let year = value.get(0..4)?;
    if !year.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }

    year.parse::<u16>().ok()
}

fn capture_numeric_suffix(source: &str, marker: &str) -> Option<String> {
    let index = source.find(marker)?;
    let after_marker = &source[index + marker.len()..];
    let numeric = after_marker
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();

    if numeric.is_empty() {
        None
    } else {
        Some(numeric)
    }
}
