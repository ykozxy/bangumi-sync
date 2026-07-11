use std::collections::{HashMap, HashSet};

use crate::model::{MediaKind, Provider};
use crate::store::{ExternalIdEdgeInput, SqliteStore, StoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnilistMalCrosswalk {
    pub anilist_id: String,
    pub myanimelist_id: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrosswalkImportError {
    EmptyExternalId {
        provider: Provider,
    },
    ConflictingExistingWorks {
        anilist_work_id: i64,
        myanimelist_work_id: i64,
    },
    ConflictingBatchRows {
        provider: Provider,
        external_id: String,
    },
    Store(StoreError),
}

pub fn import_anilist_id_mal_crosswalk(
    store: &SqliteStore,
    media_kind: MediaKind,
    rows: &[AnilistMalCrosswalk],
) -> Result<usize, CrosswalkImportError> {
    validate_anilist_id_mal_crosswalk(store, media_kind, rows)?;

    let unique_rows = unique_anilist_id_mal_crosswalks(rows);
    let mut planned_rows = Vec::with_capacity(unique_rows.len());
    for row in &unique_rows {
        let anilist_work =
            store.find_work_by_external_id(Provider::AniList, media_kind, &row.anilist_id)?;
        let myanimelist_work = store.find_work_by_external_id(
            Provider::MyAnimeList,
            media_kind,
            &row.myanimelist_id,
        )?;

        let work_id = match (anilist_work, myanimelist_work) {
            (Some(work_id), _) | (None, Some(work_id)) => work_id,
            (None, None) => {
                planned_rows.push((row, None));
                continue;
            }
        };

        planned_rows.push((row, Some(work_id)));
    }

    let imported = planned_rows.len();
    for (row, planned_work_id) in planned_rows {
        let work_id = match planned_work_id {
            Some(work_id) => work_id,
            None => {
                let display_title = row.title.as_deref().unwrap_or("anilist idMal crosswalk");
                store.create_identity_work(media_kind, display_title)?
            }
        };
        for (provider, external_id) in [
            (Provider::AniList, row.anilist_id.as_str()),
            (Provider::MyAnimeList, row.myanimelist_id.as_str()),
        ] {
            store.upsert_external_id_edge(ExternalIdEdgeInput {
                work_id,
                provider,
                media_kind,
                external_id: external_id.to_owned(),
                source: "anilist idMal".to_owned(),
                confidence: 1000,
                match_method: "anilist-id-mal".to_owned(),
                dataset_version: None,
            })?;
        }
    }

    Ok(imported)
}

pub(crate) fn validate_anilist_id_mal_crosswalk(
    store: &SqliteStore,
    media_kind: MediaKind,
    rows: &[AnilistMalCrosswalk],
) -> Result<(), CrosswalkImportError> {
    for row in rows {
        if row.anilist_id.trim().is_empty() {
            return Err(CrosswalkImportError::EmptyExternalId {
                provider: Provider::AniList,
            });
        }
        if row.myanimelist_id.trim().is_empty() {
            return Err(CrosswalkImportError::EmptyExternalId {
                provider: Provider::MyAnimeList,
            });
        }
    }

    let mut anilist_to_mal = HashMap::new();
    let mut mal_to_anilist = HashMap::new();
    for row in rows {
        if let Some(existing_mal_id) =
            anilist_to_mal.insert(row.anilist_id.as_str(), row.myanimelist_id.as_str())
        {
            if existing_mal_id != row.myanimelist_id.as_str() {
                return Err(CrosswalkImportError::ConflictingBatchRows {
                    provider: Provider::AniList,
                    external_id: row.anilist_id.clone(),
                });
            }
        }
        if let Some(existing_anilist_id) =
            mal_to_anilist.insert(row.myanimelist_id.as_str(), row.anilist_id.as_str())
        {
            if existing_anilist_id != row.anilist_id.as_str() {
                return Err(CrosswalkImportError::ConflictingBatchRows {
                    provider: Provider::MyAnimeList,
                    external_id: row.myanimelist_id.clone(),
                });
            }
        }
    }

    for row in rows {
        let anilist_work =
            store.find_work_by_external_id(Provider::AniList, media_kind, &row.anilist_id)?;
        let myanimelist_work = store.find_work_by_external_id(
            Provider::MyAnimeList,
            media_kind,
            &row.myanimelist_id,
        )?;

        match (anilist_work, myanimelist_work) {
            (Some(anilist_work_id), Some(myanimelist_work_id))
                if anilist_work_id != myanimelist_work_id =>
            {
                return Err(CrosswalkImportError::ConflictingExistingWorks {
                    anilist_work_id,
                    myanimelist_work_id,
                });
            }
            _ => {}
        }
    }

    Ok(())
}

fn unique_anilist_id_mal_crosswalks(rows: &[AnilistMalCrosswalk]) -> Vec<&AnilistMalCrosswalk> {
    let mut seen = HashSet::new();
    let mut unique_rows = Vec::new();

    for row in rows {
        if seen.insert((row.anilist_id.as_str(), row.myanimelist_id.as_str())) {
            unique_rows.push(row);
        }
    }

    unique_rows
}

impl From<StoreError> for CrosswalkImportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}
