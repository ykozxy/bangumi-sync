use crate::identity::{import_anilist_id_mal_crosswalk, AnilistMalCrosswalk, CrosswalkImportError};
use crate::model::{MediaKind, Provider};
use crate::provider::{
    fetch_authorized_bangumi_episode_collection_pages_with_transport,
    fetch_authorized_collection_snapshot_pages_with_transport,
    fetch_authorized_collection_snapshot_with_transport, AuthorizedProviderReadTransport,
    BangumiEpisodeCollectionSnapshot, ProviderAccessToken, ProviderAuthBootstrapMode,
    ProviderCollectionSnapshot, ProviderCredentialAccessTokenStore, ProviderReadError,
};
use crate::store::{SqliteStore, StoreError};

use super::{
    acquire_stored_provider_access_token, StoredCredentialRefreshError,
    StoredProviderAccessTokenOutcome,
};

#[derive(Debug)]
pub enum StoredProviderCollectionReadError {
    Credential(StoredCredentialRefreshError),
    Read(ProviderReadError),
}

#[derive(Debug)]
pub enum StoredProviderCollectionImportError {
    Credential(StoredCredentialRefreshError),
    Read(ProviderReadError),
    Identity(CrosswalkImportError),
    Store(StoreError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredProviderCollectionReadOutcome {
    Snapshot(ProviderCollectionSnapshot),
    RefreshRequired,
    Reauthorize {
        preferred_mode: ProviderAuthBootstrapMode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredProviderCollectionImportOutcome {
    Imported {
        imported: usize,
    },
    RefreshRequired,
    Reauthorize {
        preferred_mode: ProviderAuthBootstrapMode,
    },
}

impl From<StoredCredentialRefreshError> for StoredProviderCollectionReadError {
    fn from(error: StoredCredentialRefreshError) -> Self {
        Self::Credential(error)
    }
}

impl From<ProviderReadError> for StoredProviderCollectionReadError {
    fn from(error: ProviderReadError) -> Self {
        Self::Read(error)
    }
}

impl From<StoredCredentialRefreshError> for StoredProviderCollectionImportError {
    fn from(error: StoredCredentialRefreshError) -> Self {
        Self::Credential(error)
    }
}

impl From<ProviderReadError> for StoredProviderCollectionImportError {
    fn from(error: ProviderReadError) -> Self {
        Self::Read(error)
    }
}

impl From<CrosswalkImportError> for StoredProviderCollectionImportError {
    fn from(error: CrosswalkImportError) -> Self {
        Self::Identity(error)
    }
}

impl From<StoreError> for StoredProviderCollectionImportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

pub fn fetch_collection_snapshot_with_stored_access_token<S, T>(
    store: &SqliteStore,
    provider: Provider,
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
    offset: u32,
    now_unix_seconds: i64,
    secret_store: &mut S,
    read_transport: &mut T,
) -> Result<StoredProviderCollectionReadOutcome, StoredProviderCollectionReadError>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderReadTransport,
{
    match acquire_stored_provider_access_token(
        store,
        provider,
        account_id,
        now_unix_seconds,
        secret_store,
    )? {
        StoredProviderAccessTokenOutcome::Authorized(token) => {
            let snapshot = fetch_authorized_collection_snapshot_with_transport(
                read_transport,
                &token,
                provider,
                media_kind,
                account_id,
                limit,
                offset,
            )?;
            Ok(StoredProviderCollectionReadOutcome::Snapshot(snapshot))
        }
        StoredProviderAccessTokenOutcome::RefreshRequired => {
            Ok(StoredProviderCollectionReadOutcome::RefreshRequired)
        }
        StoredProviderAccessTokenOutcome::Reauthorize { preferred_mode } => {
            Ok(StoredProviderCollectionReadOutcome::Reauthorize { preferred_mode })
        }
    }
}

pub fn import_collection_snapshot_pages_with_stored_access_token<S, T>(
    store: &SqliteStore,
    provider: Provider,
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
    now_unix_seconds: i64,
    secret_store: &mut S,
    read_transport: &mut T,
) -> Result<StoredProviderCollectionImportOutcome, StoredProviderCollectionImportError>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderReadTransport,
{
    match acquire_stored_provider_access_token(
        store,
        provider,
        account_id,
        now_unix_seconds,
        secret_store,
    )? {
        StoredProviderAccessTokenOutcome::Authorized(token) => {
            let snapshot = fetch_authorized_collection_snapshot_pages_with_transport(
                read_transport,
                &token,
                provider,
                media_kind,
                account_id,
                limit,
            )?;
            let imported = if snapshot.provider() == Provider::Bangumi
                && snapshot.media_kind() == MediaKind::Anime
            {
                let episode_snapshots = fetch_bangumi_anime_episode_collections_for_snapshot(
                    &snapshot,
                    limit,
                    &token,
                    read_transport,
                )?;
                store.upsert_collection_snapshot_with_bangumi_episode_snapshots(
                    account_id,
                    &snapshot,
                    &episode_snapshots,
                )?
            } else {
                import_collection_snapshot_with_identity_links(store, account_id, &snapshot)?
            };
            Ok(StoredProviderCollectionImportOutcome::Imported { imported })
        }
        StoredProviderAccessTokenOutcome::RefreshRequired => {
            Ok(StoredProviderCollectionImportOutcome::RefreshRequired)
        }
        StoredProviderAccessTokenOutcome::Reauthorize { preferred_mode } => {
            Ok(StoredProviderCollectionImportOutcome::Reauthorize { preferred_mode })
        }
    }
}

pub fn import_collection_snapshot_with_identity_links(
    store: &SqliteStore,
    account_id: &str,
    snapshot: &ProviderCollectionSnapshot,
) -> Result<usize, StoredProviderCollectionImportError> {
    let imported_identity_links = import_snapshot_identity_links(store, snapshot)?;
    let imported = store.upsert_collection_snapshot(account_id, snapshot)?;
    if imported_identity_links > 0 {
        store.backfill_collection_entry_work_ids(
            account_id,
            snapshot.media_kind(),
            &[Provider::AniList, Provider::MyAnimeList],
        )?;
    }

    Ok(imported)
}

fn import_snapshot_identity_links(
    store: &SqliteStore,
    snapshot: &ProviderCollectionSnapshot,
) -> Result<usize, CrosswalkImportError> {
    let rows = snapshot
        .identity_links()
        .iter()
        .filter_map(|link| {
            if link.source_provider() == Provider::AniList
                && link.source_media_kind() == snapshot.media_kind()
                && link.target_provider() == Provider::MyAnimeList
                && link.target_media_kind() == snapshot.media_kind()
            {
                Some(AnilistMalCrosswalk {
                    anilist_id: link.source_external_id().to_owned(),
                    myanimelist_id: link.target_external_id().to_owned(),
                    title: link.title().map(ToOwned::to_owned),
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        return Ok(0);
    }

    import_anilist_id_mal_crosswalk(store, snapshot.media_kind(), &rows)
}

fn fetch_bangumi_anime_episode_collections_for_snapshot<T>(
    snapshot: &ProviderCollectionSnapshot,
    limit: u16,
    token: &ProviderAccessToken,
    read_transport: &mut T,
) -> Result<Vec<BangumiEpisodeCollectionSnapshot>, StoredProviderCollectionImportError>
where
    T: AuthorizedProviderReadTransport,
{
    let mut episode_snapshots = Vec::new();
    for entry in snapshot.entries() {
        let episode_snapshot = fetch_authorized_bangumi_episode_collection_pages_with_transport(
            read_transport,
            token,
            entry.provider_entry_id(),
            limit,
        )?;
        episode_snapshots.push(episode_snapshot);
    }

    Ok(episode_snapshots)
}

pub fn fetch_collection_snapshot_pages_with_stored_access_token<S, T>(
    store: &SqliteStore,
    provider: Provider,
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
    now_unix_seconds: i64,
    secret_store: &mut S,
    read_transport: &mut T,
) -> Result<StoredProviderCollectionReadOutcome, StoredProviderCollectionReadError>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderReadTransport,
{
    match acquire_stored_provider_access_token(
        store,
        provider,
        account_id,
        now_unix_seconds,
        secret_store,
    )? {
        StoredProviderAccessTokenOutcome::Authorized(token) => {
            let snapshot = fetch_authorized_collection_snapshot_pages_with_transport(
                read_transport,
                &token,
                provider,
                media_kind,
                account_id,
                limit,
            )?;
            Ok(StoredProviderCollectionReadOutcome::Snapshot(snapshot))
        }
        StoredProviderAccessTokenOutcome::RefreshRequired => {
            Ok(StoredProviderCollectionReadOutcome::RefreshRequired)
        }
        StoredProviderAccessTokenOutcome::Reauthorize { preferred_mode } => {
            Ok(StoredProviderCollectionReadOutcome::Reauthorize { preferred_mode })
        }
    }
}
