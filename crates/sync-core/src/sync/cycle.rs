use crate::identity::auto_link_provider_item_match;
use crate::model::{MediaKind, Provider};
use crate::provider::{
    AuthorizedProviderReadTransport, ProviderCredentialAccessTokenStore,
    ProviderCredentialSecretStore, ProviderOAuthTokenTransport,
};
use crate::store::{SqliteStore, StoreError};

use super::{
    import_collection_snapshot_pages_with_stored_access_token,
    planner::{plan_dry_run_for_media_kinds, SyncPlan},
    refresh_stored_provider_credential_if_needed, StoredCredentialRefreshError,
    StoredProviderCollectionImportError, StoredProviderCollectionImportOutcome,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSnapshotImportSummary {
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub outcome: StoredProviderCollectionImportOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredProviderSyncCycleOutcome {
    Planned {
        imports: Vec<ProviderSnapshotImportSummary>,
        plan: SyncPlan,
    },
    CredentialsRequired {
        imports: Vec<ProviderSnapshotImportSummary>,
    },
}

#[derive(Debug)]
pub enum StoredProviderSyncCycleError {
    CredentialRefresh(StoredCredentialRefreshError),
    Import(StoredProviderCollectionImportError),
    Plan(StoreError),
}

impl From<StoredCredentialRefreshError> for StoredProviderSyncCycleError {
    fn from(error: StoredCredentialRefreshError) -> Self {
        Self::CredentialRefresh(error)
    }
}

impl From<StoredProviderCollectionImportError> for StoredProviderSyncCycleError {
    fn from(error: StoredProviderCollectionImportError) -> Self {
        Self::Import(error)
    }
}

impl From<StoreError> for StoredProviderSyncCycleError {
    fn from(error: StoreError) -> Self {
        Self::Plan(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSyncCredentialRefreshConfig {
    pub provider: Provider,
    pub client_id: String,
    pub client_secret: Option<String>,
}

// The cycle coordinator intentionally receives each side-effect boundary rather
// than constructing network or credential implementations internally.
#[allow(clippy::too_many_arguments)]
pub fn refresh_snapshots_and_plan_dry_run_with_auto_refresh<S, R, T>(
    store: &SqliteStore,
    account_id: &str,
    media_kinds: &[MediaKind],
    providers: &[Provider],
    auto_link_local: bool,
    refresh_configs: &[ProviderSyncCredentialRefreshConfig],
    limit: u16,
    now_unix_seconds: i64,
    token_transport: &mut R,
    secret_store: &mut S,
    read_transport: &mut T,
) -> Result<StoredProviderSyncCycleOutcome, StoredProviderSyncCycleError>
where
    S: ProviderCredentialAccessTokenStore + ProviderCredentialSecretStore,
    R: ProviderOAuthTokenTransport,
    T: AuthorizedProviderReadTransport,
{
    for provider in providers {
        if let Some(config) = refresh_configs
            .iter()
            .find(|config| config.provider == *provider)
        {
            let _ = refresh_stored_provider_credential_if_needed(
                store,
                *provider,
                account_id,
                config.client_id.clone(),
                config.client_secret.clone(),
                now_unix_seconds,
                token_transport,
                secret_store,
            )?;
        }
    }

    refresh_snapshots_and_plan_dry_run_with_stored_access_tokens(
        store,
        account_id,
        media_kinds,
        providers,
        auto_link_local,
        limit,
        now_unix_seconds,
        secret_store,
        read_transport,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn refresh_snapshots_and_plan_dry_run_with_stored_access_tokens<S, T>(
    store: &SqliteStore,
    account_id: &str,
    media_kinds: &[MediaKind],
    providers: &[Provider],
    auto_link_local: bool,
    limit: u16,
    now_unix_seconds: i64,
    secret_store: &mut S,
    read_transport: &mut T,
) -> Result<StoredProviderSyncCycleOutcome, StoredProviderSyncCycleError>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderReadTransport,
{
    let mut imports = Vec::new();

    for media_kind in media_kinds {
        for provider in providers {
            let outcome = import_collection_snapshot_pages_with_stored_access_token(
                store,
                *provider,
                *media_kind,
                account_id,
                limit,
                now_unix_seconds,
                secret_store,
                read_transport,
            )?;
            imports.push(ProviderSnapshotImportSummary {
                provider: *provider,
                media_kind: *media_kind,
                outcome,
            });
        }
    }

    if imports.iter().any(|summary| {
        !matches!(
            summary.outcome,
            StoredProviderCollectionImportOutcome::Imported { .. }
        )
    }) {
        return Ok(StoredProviderSyncCycleOutcome::CredentialsRequired { imports });
    }

    if auto_link_local {
        auto_link_local_collection_entries(store, account_id, media_kinds, providers)?;
    }

    let plan = plan_dry_run_for_media_kinds(store, account_id, media_kinds, providers)?;
    Ok(StoredProviderSyncCycleOutcome::Planned { imports, plan })
}

pub fn auto_link_local_collection_entries(
    store: &SqliteStore,
    account_id: &str,
    media_kinds: &[MediaKind],
    providers: &[Provider],
) -> Result<(), StoreError> {
    for media_kind in media_kinds {
        let entries = store.collection_entries_for_planning(account_id, *media_kind)?;
        for entry in entries {
            if providers.contains(&entry.provider) {
                let _ = auto_link_provider_item_match(
                    store,
                    entry.provider,
                    entry.media_kind,
                    &entry.provider_entry_id,
                )?;
            }
        }
        let _ = store.backfill_collection_entry_work_ids(account_id, *media_kind, providers)?;
    }

    Ok(())
}
