use serde::Deserialize;

use crate::model::{MediaKind, Provider};
use crate::store::{
    ExternalIdEdgeInput, ManualMappingDecision, ManualMappingInput, SqliteStore, StoreError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyImportError {
    InvalidJson {
        message: String,
    },
    InvalidManualRelationLength {
        index: usize,
        len: usize,
    },
    ConflictingManualRelation {
        index: usize,
        bangumi_work_id: i64,
        anilist_work_id: i64,
    },
    Store(StoreError),
}

#[derive(Debug, Deserialize)]
struct LegacyIgnoreEntries {
    bangumi: Vec<u64>,
    anilist: Vec<u64>,
    mal: Vec<u64>,
}

pub fn import_legacy_manual_relations(
    store: &SqliteStore,
    media_kind: MediaKind,
    json: &str,
) -> Result<usize, LegacyImportError> {
    let relations = parse_legacy_manual_relations(json)?;

    for (index, relation) in relations.iter().enumerate() {
        let (bangumi_id, anilist_id) = relation;
        let existing_bangumi =
            store.find_work_by_external_id(Provider::Bangumi, media_kind, bangumi_id)?;
        let existing_anilist =
            store.find_work_by_external_id(Provider::AniList, media_kind, anilist_id)?;

        if let (Some(bangumi_work_id), Some(anilist_work_id)) = (existing_bangumi, existing_anilist)
        {
            if bangumi_work_id != anilist_work_id {
                return Err(LegacyImportError::ConflictingManualRelation {
                    index,
                    bangumi_work_id,
                    anilist_work_id,
                });
            }
        }
    }

    for relation in &relations {
        let (bangumi_id, anilist_id) = relation;
        let existing_bangumi =
            store.find_work_by_external_id(Provider::Bangumi, media_kind, bangumi_id)?;
        let existing_anilist =
            store.find_work_by_external_id(Provider::AniList, media_kind, anilist_id)?;
        let work_id = match (existing_bangumi, existing_anilist) {
            (Some(work_id), _) | (None, Some(work_id)) => work_id,
            (None, None) => {
                let display_title = format!("manual bangumi:{bangumi_id} anilist:{anilist_id}");
                store.create_identity_work(media_kind, &display_title)?
            }
        };

        for (provider, external_id) in [
            (Provider::Bangumi, bangumi_id.as_str()),
            (Provider::AniList, anilist_id.as_str()),
        ] {
            store.upsert_external_id_edge(ExternalIdEdgeInput {
                work_id,
                provider,
                media_kind,
                external_id: external_id.to_owned(),
                source: "legacy manual_relations.json".to_owned(),
                confidence: 1000,
                match_method: "manual".to_owned(),
                dataset_version: None,
            })?;
            store.upsert_manual_mapping(ManualMappingInput {
                work_id: Some(work_id),
                provider,
                media_kind,
                external_id: external_id.to_owned(),
                decision: ManualMappingDecision::Link,
                note: Some("legacy manual_relations.json".to_owned()),
            })?;
        }
    }

    Ok(relations.len())
}

pub fn validate_legacy_manual_relations_json(json: &str) -> Result<usize, LegacyImportError> {
    Ok(parse_legacy_manual_relations(json)?.len())
}

fn parse_legacy_manual_relations(json: &str) -> Result<Vec<(String, String)>, LegacyImportError> {
    let relations = serde_json::from_str::<Vec<Vec<u64>>>(json)?;

    for (index, relation) in relations.iter().enumerate() {
        if relation.len() != 2 {
            return Err(LegacyImportError::InvalidManualRelationLength {
                index,
                len: relation.len(),
            });
        }
    }

    Ok(relations
        .into_iter()
        .map(|relation| (relation[0].to_string(), relation[1].to_string()))
        .collect())
}

pub fn import_legacy_ignore_entries(
    store: &SqliteStore,
    media_kind: MediaKind,
    json: &str,
) -> Result<usize, LegacyImportError> {
    let entries = serde_json::from_str::<LegacyIgnoreEntries>(json)?;
    let mut imported = 0;

    for (provider, external_ids) in [
        (Provider::Bangumi, entries.bangumi),
        (Provider::AniList, entries.anilist),
        (Provider::MyAnimeList, entries.mal),
    ] {
        for external_id in external_ids {
            store.upsert_manual_mapping(ManualMappingInput {
                work_id: None,
                provider,
                media_kind,
                external_id: external_id.to_string(),
                decision: ManualMappingDecision::Ignore,
                note: Some("legacy ignore_entries.json".to_owned()),
            })?;
            imported += 1;
        }
    }

    Ok(imported)
}

pub fn validate_legacy_ignore_entries_json(json: &str) -> Result<usize, LegacyImportError> {
    let entries = serde_json::from_str::<LegacyIgnoreEntries>(json)?;

    Ok(entries.bangumi.len() + entries.anilist.len() + entries.mal.len())
}

impl From<serde_json::Error> for LegacyImportError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidJson {
            message: error.to_string(),
        }
    }
}

impl From<StoreError> for LegacyImportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}
