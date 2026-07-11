use std::{collections::HashMap, fmt, path::Path};

use rusqlite::{params, params_from_iter, Connection, OpenFlags, OptionalExtension};

use crate::model::{
    stable_hash, CanonicalFieldState, CollectionStatus, MediaKind, Provider, SyncField,
    MISSING_ENTRY_VALUE, UNSET_FIELD_VALUE,
};
use crate::provider::{
    provider_credential_capability, BangumiEpisodeCollectionSnapshot, ProviderAuthBootstrapMode,
    ProviderAuthFlow, ProviderCollectionSnapshot, ProviderCredentialExchangeResult,
    ProviderCredentialState, ProviderRefreshPolicy, ProviderRefreshTokenState,
};
use crate::title::{title_fts_text, title_fts_text_for_values};

use super::migrations::MIGRATIONS;

const PROVIDER_ITEM_FTS_SEARCH_TEXT_VERSION_KEY: &str = "provider_item_fts_search_text_version";
const PROVIDER_ITEM_FTS_SEARCH_TEXT_VERSION: &str = "4";

#[derive(Debug)]
pub struct SqliteStore {
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    InvalidConfidence {
        value: u16,
    },
    ExternalIdEdgeUpsertRejected {
        provider: Provider,
        media_kind: MediaKind,
        external_id: String,
    },
    InvalidProviderCredential {
        provider: Provider,
        reason: String,
    },
    StaleWriteJournalIntent {
        provider: Provider,
        target_provider_entry_id: String,
        field: SyncField,
    },
    AmbiguousWriteJournalIntent {
        provider: Provider,
        target_provider_entry_id: String,
    },
    Sqlite {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdEdgeInput {
    pub work_id: i64,
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub external_id: String,
    pub source: String,
    pub confidence: u16,
    pub match_method: String,
    pub dataset_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdEdgeUpsertInput {
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub external_id: String,
    pub source: String,
    pub confidence: u16,
    pub match_method: String,
    pub dataset_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdEdgeDetails {
    pub work_id: i64,
    pub confidence: u16,
    pub match_method: String,
    pub source: String,
    pub dataset_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderItemInput {
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub external_id: String,
    pub canonical_title: String,
    pub aliases: Vec<String>,
    pub format: Option<String>,
    pub release_year: Option<u16>,
    pub source_payload_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderItemCandidate {
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub external_id: String,
    pub canonical_title: String,
    pub aliases: Vec<String>,
    pub format: Option<String>,
    pub release_year: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionEntryDetails {
    pub id: i64,
    pub work_id: Option<i64>,
    pub status: CollectionStatus,
    pub score_hundred: Option<u8>,
    pub progress_episodes: Option<u32>,
    pub progress_chapters: Option<u32>,
    pub progress_volumes: Option<u32>,
    pub provider_payload_hash: String,
    pub provider_updated_at_epoch_secs: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningCollectionEntry {
    pub collection_entry_id: i64,
    pub work_id: Option<i64>,
    pub provider: Provider,
    pub provider_entry_id: String,
    pub media_kind: MediaKind,
    pub status: CollectionStatus,
    pub score_hundred: Option<u8>,
    pub progress_episodes: Option<u32>,
    pub progress_chapters: Option<u32>,
    pub progress_volumes: Option<u32>,
    pub provider_updated_at_epoch_secs: Option<i64>,
    pub field_observations: HashMap<SyncField, FieldObservationDetails>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldObservationChangeOrigin {
    Initial,
    Unchanged,
    External,
    ToolWrite,
}

impl FieldObservationChangeOrigin {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Unchanged => "unchanged",
            Self::External => "external",
            Self::ToolWrite => "tool_write",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldObservationDetails {
    pub field: SyncField,
    pub observed_value_hash: String,
    pub previous_observed_value_hash: Option<String>,
    pub observation_version: i64,
    pub last_changed_observation_version: i64,
    pub change_origin: FieldObservationChangeOrigin,
    pub attributed_write_journal_field_id: Option<i64>,
    pub attributed_source_provider: Option<Provider>,
    pub attributed_source_value_hash: Option<String>,
    pub externally_cleared: bool,
    pub pending_external_change: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkExternalId {
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub external_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualMappingDecision {
    Link,
    Ignore,
}

impl ManualMappingDecision {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Ignore => "ignore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualMappingInput {
    pub work_id: Option<i64>,
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub external_id: String,
    pub decision: ManualMappingDecision,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteJournalStatus {
    Planned,
    Attempted,
    Succeeded,
    Failed,
}

impl WriteJournalStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Attempted => "attempted",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteJournalIntent {
    pub provider: Provider,
    pub account_id: String,
    pub operation_id: String,
    pub request_body: String,
    pub work_id: i64,
    pub source_provider: Provider,
    pub media_kind: MediaKind,
    pub target_provider_entry_id: String,
    pub fields: Vec<WriteJournalFieldIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteJournalFieldIntent {
    pub field: SyncField,
    pub before_value_hash: String,
    pub source_value_hash: String,
    pub expected_value_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteJournalIntentOutcome {
    Recorded,
    AlreadySucceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteJournalCompletion {
    pub provider: Provider,
    pub account_id: String,
    pub operation_id: String,
    pub result_status: WriteJournalStatus,
    pub response_body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteJournalEntryDetails {
    pub provider: Provider,
    pub account_id: String,
    pub operation_id: String,
    pub request_hash: String,
    pub response_hash: Option<String>,
    pub result_status: WriteJournalStatus,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteJournalFieldDetails {
    pub field: SyncField,
    pub work_id: i64,
    pub media_kind: MediaKind,
    pub source_provider: Provider,
    pub target_provider_entry_id: String,
    pub basis_observation_version: Option<i64>,
    pub basis_snapshot_generation: i64,
    pub before_value_hash: String,
    pub source_value_hash: String,
    pub expected_value_hash: String,
    pub attributed_collection_entry_id: Option<i64>,
    pub attributed_observation_version: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialInput {
    pub provider: Provider,
    pub account_id: String,
    pub auth_flow: ProviderAuthFlow,
    pub bootstrap_mode: ProviderAuthBootstrapMode,
    pub credential_store_ref: String,
    pub access_token_expires_at_epoch_secs: i64,
    pub refresh_token: ProviderRefreshTokenState,
    pub last_refresh_at_epoch_secs: Option<i64>,
    pub last_reauth_request_at_epoch_secs: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialRefreshAttemptStatus {
    Pending,
    Committed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCredentialRefreshAttemptFinalizeStatus {
    Committed,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialRefreshAttemptInput {
    pub provider: Provider,
    pub account_id: String,
    pub previous_credential_store_ref: String,
    pub started_at_epoch_secs: i64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCredentialRefreshAttemptDetails {
    pub id: i64,
    pub provider: Provider,
    pub account_id: String,
    pub previous_credential_store_ref: String,
    pub attempted_credential_store_ref: Option<String>,
    pub status: ProviderCredentialRefreshAttemptStatus,
    pub failure_kind: Option<String>,
    pub started_at_epoch_secs: i64,
    pub completed_at_epoch_secs: Option<i64>,
}

impl fmt::Debug for ProviderCredentialRefreshAttemptDetails {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredentialRefreshAttemptDetails")
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("account_id", &self.account_id)
            .field("previous_credential_store_ref", &"[redacted]")
            .field(
                "attempted_credential_store_ref",
                &self
                    .attempted_credential_store_ref
                    .as_ref()
                    .map(|_| "[redacted]"),
            )
            .field("status", &self.status)
            .field("failure_kind", &self.failure_kind)
            .field("started_at_epoch_secs", &self.started_at_epoch_secs)
            .field("completed_at_epoch_secs", &self.completed_at_epoch_secs)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCredentialDetails {
    pub provider: Provider,
    pub account_id: String,
    pub auth_flow: ProviderAuthFlow,
    pub bootstrap_mode: ProviderAuthBootstrapMode,
    pub credential_store_ref: String,
    pub access_token_expires_at_epoch_secs: i64,
    pub refresh_token: ProviderRefreshTokenState,
    pub last_refresh_at_epoch_secs: Option<i64>,
    pub last_reauth_request_at_epoch_secs: Option<i64>,
}

impl ProviderCredentialDetails {
    pub fn credential_state(&self) -> ProviderCredentialState {
        ProviderCredentialState::available(
            self.provider,
            self.access_token_expires_at_epoch_secs,
            self.refresh_token,
        )
    }
}

impl fmt::Debug for ProviderCredentialDetails {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredentialDetails")
            .field("provider", &self.provider)
            .field("account_id", &self.account_id)
            .field("auth_flow", &self.auth_flow)
            .field("bootstrap_mode", &self.bootstrap_mode)
            .field("credential_store_ref", &"[redacted]")
            .field(
                "access_token_expires_at_epoch_secs",
                &self.access_token_expires_at_epoch_secs,
            )
            .field("refresh_token", &self.refresh_token)
            .field(
                "last_refresh_at_epoch_secs",
                &self.last_refresh_at_epoch_secs,
            )
            .field(
                "last_reauth_request_at_epoch_secs",
                &self.last_reauth_request_at_epoch_secs,
            )
            .finish()
    }
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { connection };
        store.run_migrations()?;
        Ok(store)
    }

    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(Self { connection })
    }

    pub fn open_existing_read_write(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Self { connection })
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { connection };
        store.run_migrations()?;
        Ok(store)
    }

    pub fn schema_objects(&self) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT name
             FROM sqlite_schema
             WHERE name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )?;

        let names = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(names)
    }

    pub fn create_identity_work(
        &self,
        media_kind: MediaKind,
        display_title: &str,
    ) -> Result<i64, StoreError> {
        self.connection.execute(
            "INSERT INTO identity_work(media_kind, display_title)
             VALUES (?1, ?2)",
            params![media_kind.as_str(), display_title],
        )?;

        Ok(self.connection.last_insert_rowid())
    }

    pub fn identity_work_count(&self) -> Result<u64, StoreError> {
        let count = self
            .connection
            .query_row("SELECT COUNT(*) FROM identity_work", [], |row| {
                row.get::<_, u64>(0)
            })?;

        Ok(count)
    }

    pub fn upsert_external_id_edge(&self, input: ExternalIdEdgeInput) -> Result<(), StoreError> {
        if input.confidence > 1000 {
            return Err(StoreError::InvalidConfidence {
                value: input.confidence,
            });
        }
        if let Some(existing) =
            self.external_id_edge_details(input.provider, input.media_kind, &input.external_id)?
        {
            let existing_strength =
                external_id_edge_evidence_strength(&existing.source, &existing.match_method);
            let input_strength =
                external_id_edge_evidence_strength(&input.source, &input.match_method);
            if (existing.confidence, existing_strength) >= (input.confidence, input_strength) {
                return Ok(());
            }
        }

        self.connection.execute(
            "INSERT INTO external_id_edge(
                work_id,
                provider,
                media_kind,
                external_id,
                source,
                confidence,
                match_method,
                dataset_version
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(provider, media_kind, external_id) DO UPDATE SET
                work_id = excluded.work_id,
                source = excluded.source,
                confidence = excluded.confidence,
                match_method = excluded.match_method,
                dataset_version = excluded.dataset_version,
                verified_at = CURRENT_TIMESTAMP,
                stale = 0",
            params![
                input.work_id,
                input.provider.as_str(),
                input.media_kind.as_str(),
                input.external_id,
                input.source,
                input.confidence,
                input.match_method,
                input.dataset_version,
            ],
        )?;

        Ok(())
    }

    pub fn upsert_external_id_edges_atomically(
        &self,
        existing_work_id: Option<i64>,
        media_kind: MediaKind,
        display_title: &str,
        edges: &[ExternalIdEdgeUpsertInput],
    ) -> Result<i64, StoreError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let work_id = match existing_work_id {
                Some(work_id) => work_id,
                None => self.create_identity_work(media_kind, display_title)?,
            };

            for edge in edges {
                self.upsert_external_id_edge(ExternalIdEdgeInput {
                    work_id,
                    provider: edge.provider,
                    media_kind: edge.media_kind,
                    external_id: edge.external_id.clone(),
                    source: edge.source.clone(),
                    confidence: edge.confidence,
                    match_method: edge.match_method.clone(),
                    dataset_version: edge.dataset_version.clone(),
                })?;
                let linked_work_id = self.find_work_by_external_id(
                    edge.provider,
                    edge.media_kind,
                    &edge.external_id,
                )?;
                if linked_work_id != Some(work_id) {
                    return Err(StoreError::ExternalIdEdgeUpsertRejected {
                        provider: edge.provider,
                        media_kind: edge.media_kind,
                        external_id: edge.external_id.clone(),
                    });
                }
            }

            Ok(work_id)
        })();

        match result {
            Ok(work_id) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(work_id)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    pub fn find_work_by_external_id(
        &self,
        provider: Provider,
        media_kind: MediaKind,
        external_id: &str,
    ) -> Result<Option<i64>, StoreError> {
        let work_id = self
            .connection
            .query_row(
                "SELECT work_id
                 FROM external_id_edge
                 WHERE provider = ?1
                   AND media_kind = ?2
                   AND external_id = ?3
                   AND stale = 0",
                params![provider.as_str(), media_kind.as_str(), external_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        Ok(work_id)
    }

    pub fn external_id_edge_details(
        &self,
        provider: Provider,
        media_kind: MediaKind,
        external_id: &str,
    ) -> Result<Option<ExternalIdEdgeDetails>, StoreError> {
        let details = self
            .connection
            .query_row(
                "SELECT work_id, confidence, match_method, source, dataset_version
                 FROM external_id_edge
                 WHERE provider = ?1
                   AND media_kind = ?2
                   AND external_id = ?3
                   AND stale = 0",
                params![provider.as_str(), media_kind.as_str(), external_id],
                |row| {
                    Ok(ExternalIdEdgeDetails {
                        work_id: row.get(0)?,
                        confidence: row.get(1)?,
                        match_method: row.get(2)?,
                        source: row.get(3)?,
                        dataset_version: row.get(4)?,
                    })
                },
            )
            .optional()?;

        Ok(details)
    }

    pub fn upsert_provider_item(&self, input: ProviderItemInput) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO provider_item(
                provider,
                media_kind,
                external_id,
                canonical_title,
                format,
                release_year,
                source_payload_hash
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(provider, media_kind, external_id) DO UPDATE SET
                canonical_title = excluded.canonical_title,
                format = excluded.format,
                release_year = excluded.release_year,
                source_payload_hash = excluded.source_payload_hash,
                updated_at = CURRENT_TIMESTAMP",
            params![
                input.provider.as_str(),
                input.media_kind.as_str(),
                input.external_id,
                input.canonical_title,
                input.format,
                input.release_year,
                input.source_payload_hash,
            ],
        )?;

        let provider_item_id =
            self.provider_item_id(input.provider, input.media_kind, &input.external_id)?;

        self.connection.execute(
            "DELETE FROM provider_item_alias WHERE provider_item_id = ?1",
            params![provider_item_id],
        )?;

        for alias in &input.aliases {
            self.connection.execute(
                "INSERT OR IGNORE INTO provider_item_alias(provider_item_id, alias)
                 VALUES (?1, ?2)",
                params![provider_item_id, alias],
            )?;
        }

        self.connection.execute(
            "DELETE FROM provider_item_fts WHERE provider_item_id = ?1",
            params![provider_item_id],
        )?;

        self.insert_provider_item_fts(
            provider_item_id,
            input.media_kind.as_str(),
            &input.canonical_title,
            &input.aliases,
        )?;

        Ok(())
    }

    pub fn provider_item_payload_hash(
        &self,
        provider: Provider,
        media_kind: MediaKind,
        external_id: &str,
    ) -> Result<Option<String>, StoreError> {
        let hash = self
            .connection
            .query_row(
                "SELECT source_payload_hash
                 FROM provider_item
                 WHERE provider = ?1
                   AND media_kind = ?2
                   AND external_id = ?3",
                params![provider.as_str(), media_kind.as_str(), external_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        Ok(hash)
    }

    pub fn provider_item(
        &self,
        provider: Provider,
        media_kind: MediaKind,
        external_id: &str,
    ) -> Result<Option<ProviderItemCandidate>, StoreError> {
        let item = self
            .connection
            .query_row(
                "SELECT p.provider,
                        p.media_kind,
                        p.external_id,
                        p.canonical_title,
                        p.format,
                        p.release_year,
                        COALESCE((
                            SELECT group_concat(alias, char(31))
                            FROM provider_item_alias
                            WHERE provider_item_id = p.id
                        ), '') AS aliases
                 FROM provider_item p
                 WHERE p.provider = ?1
                   AND p.media_kind = ?2
                   AND p.external_id = ?3",
                params![provider.as_str(), media_kind.as_str(), external_id],
                |row| {
                    Ok(ProviderItemCandidate {
                        provider: provider_from_str(row.get::<_, String>(0)?.as_str()),
                        media_kind: media_kind_from_str(row.get::<_, String>(1)?.as_str()),
                        external_id: row.get(2)?,
                        canonical_title: row.get(3)?,
                        format: row.get(4)?,
                        release_year: row.get(5)?,
                        aliases: split_aliases(&row.get::<_, String>(6)?),
                    })
                },
            )
            .optional()?;

        Ok(item)
    }

    pub fn upsert_collection_snapshot(
        &self,
        account_id: &str,
        snapshot: &ProviderCollectionSnapshot,
    ) -> Result<usize, StoreError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.upsert_collection_snapshot_entries(account_id, snapshot);
        match result {
            Ok(imported) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(imported)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn upsert_collection_snapshot_entries(
        &self,
        account_id: &str,
        snapshot: &ProviderCollectionSnapshot,
    ) -> Result<usize, StoreError> {
        self.remove_entries_missing_from_snapshot(account_id, snapshot)?;

        for entry in snapshot.entries() {
            let work_id = self.find_work_by_external_id(
                entry.provider(),
                entry.media_kind(),
                entry.provider_entry_id(),
            )?;
            let score_hundred = entry
                .score()
                .map(|score| i64::from(score.as_hundred_point()));
            let progress = entry.progress();
            let progress_episodes = progress.episodes().map(i64::from);
            let progress_chapters = progress.chapters().map(i64::from);
            let progress_volumes = progress.volumes().map(i64::from);
            let status = collection_status_to_str(entry.status());

            self.connection.execute(
                "INSERT INTO collection_entry(
                    account_id,
                    work_id,
                    provider,
                    provider_entry_id,
                    media_kind,
                    status,
                    score_hundred,
                    progress_episodes,
                    progress_chapters,
                    progress_volumes,
                    provider_payload_hash,
                    provider_updated_at_epoch_secs
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(account_id, provider, media_kind, provider_entry_id) DO UPDATE SET
                    work_id = COALESCE(excluded.work_id, collection_entry.work_id),
                    status = excluded.status,
                    score_hundred = excluded.score_hundred,
                    progress_episodes = excluded.progress_episodes,
                    progress_chapters = excluded.progress_chapters,
                    progress_volumes = excluded.progress_volumes,
                    provider_payload_hash = excluded.provider_payload_hash,
                    provider_updated_at_epoch_secs = excluded.provider_updated_at_epoch_secs,
                    observed_at = CURRENT_TIMESTAMP",
                params![
                    account_id,
                    work_id,
                    entry.provider().as_str(),
                    entry.provider_entry_id(),
                    entry.media_kind().as_str(),
                    status,
                    score_hundred,
                    progress_episodes,
                    progress_chapters,
                    progress_volumes,
                    snapshot.raw_payload_hash(),
                    entry.provider_updated_at_epoch_secs(),
                ],
            )?;

            let collection_entry_id = self.collection_entry_id(
                account_id,
                entry.provider(),
                entry.media_kind(),
                entry.provider_entry_id(),
            )?;
            let canonical_state = CanonicalFieldState::new(
                entry.status(),
                entry.score().map(|score| score.as_hundred_point()),
                progress.episodes(),
                progress.chapters(),
                progress.volumes(),
            );
            self.refresh_field_provenance(collection_entry_id, entry.provider(), canonical_state)?;
        }

        self.settle_converged_external_changes(account_id, snapshot.media_kind())?;

        self.connection.execute(
            "INSERT INTO collection_snapshot_state(
                account_id,
                provider,
                media_kind,
                generation,
                provider_payload_hash,
                observed_at
             )
             VALUES (?1, ?2, ?3, 1, ?4, CURRENT_TIMESTAMP)
             ON CONFLICT(account_id, provider, media_kind) DO UPDATE SET
                generation = collection_snapshot_state.generation + 1,
                provider_payload_hash = excluded.provider_payload_hash,
                observed_at = CURRENT_TIMESTAMP",
            params![
                account_id,
                snapshot.provider().as_str(),
                snapshot.media_kind().as_str(),
                snapshot.raw_payload_hash(),
            ],
        )?;

        Ok(snapshot.entries().len())
    }

    fn settle_converged_external_changes(
        &self,
        account_id: &str,
        media_kind: MediaKind,
    ) -> Result<(), StoreError> {
        let pending_changes = {
            let mut statement = self.connection.prepare(
                "SELECT fp.id,
                        ce.id,
                        ce.work_id,
                        ce.provider,
                        fp.field_name,
                        fp.observed_value_hash
                 FROM field_provenance fp
                 JOIN collection_entry ce ON ce.id = fp.collection_entry_id
                 WHERE ce.account_id = ?1
                   AND ce.media_kind = ?2
                   AND ce.work_id IS NOT NULL
                   AND fp.pending_external_change = 1
                 ORDER BY fp.id",
            )?;
            let changes = statement
                .query_map(params![account_id, media_kind.as_str()], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            changes
        };

        for (
            provenance_id,
            source_collection_entry_id,
            work_id,
            source_provider,
            field_name,
            source_value_hash,
        ) in pending_changes
        {
            let peer_values = {
                let mut statement = self.connection.prepare(
                    "SELECT wjf.source_provider,
                            wjf.source_value_hash,
                            wjf.work_id,
                            wjf.media_kind,
                            wjf.target_provider_entry_id,
                            peer.provider_entry_id
                     FROM collection_entry peer
                     JOIN field_provenance peer_fp
                       ON peer_fp.collection_entry_id = peer.id
                      AND peer_fp.field_name = ?1
                     LEFT JOIN write_journal_field wjf
                       ON wjf.id = peer_fp.attributed_write_journal_field_id
                     WHERE peer.account_id = ?2
                       AND peer.work_id = ?3
                       AND peer.media_kind = ?4
                       AND peer.id != ?5",
                )?;
                let peers = statement
                    .query_map(
                        params![
                            field_name,
                            account_id,
                            work_id,
                            media_kind.as_str(),
                            source_collection_entry_id,
                        ],
                        |row| {
                            Ok((
                                row.get::<_, Option<String>>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<i64>>(2)?,
                                row.get::<_, Option<String>>(3)?,
                                row.get::<_, Option<String>>(4)?,
                                row.get::<_, String>(5)?,
                            ))
                        },
                    )?
                    .collect::<Result<Vec<_>, _>>()?;
                peers
            };
            let converged = !peer_values.is_empty()
                && peer_values.iter().all(
                    |(
                        attributed_source_provider,
                        attributed_source_value_hash,
                        attributed_work_id,
                        attributed_media_kind,
                        attributed_target_provider_entry_id,
                        peer_provider_entry_id,
                    )| {
                        attributed_source_provider.as_deref() == Some(source_provider.as_str())
                            && attributed_source_value_hash.as_deref()
                                == Some(source_value_hash.as_str())
                            && *attributed_work_id == Some(work_id)
                            && attributed_media_kind.as_deref() == Some(media_kind.as_str())
                            && attributed_target_provider_entry_id.as_deref()
                                == Some(peer_provider_entry_id.as_str())
                    },
                );
            if converged {
                self.connection.execute(
                    "UPDATE field_provenance
                     SET pending_external_change = 0
                     WHERE id = ?1",
                    params![provenance_id],
                )?;
            }
        }

        Ok(())
    }

    fn remove_entries_missing_from_snapshot(
        &self,
        account_id: &str,
        snapshot: &ProviderCollectionSnapshot,
    ) -> Result<(), StoreError> {
        let provider = snapshot.provider().as_str();
        let media_kind = snapshot.media_kind().as_str();
        let provider_entry_ids = snapshot
            .entries()
            .iter()
            .map(|entry| entry.provider_entry_id().to_owned())
            .collect::<Vec<_>>();

        if provider_entry_ids.is_empty() {
            self.connection.execute(
                "DELETE FROM collection_entry
                 WHERE account_id = ?1
                   AND provider = ?2
                   AND media_kind = ?3",
                params![account_id, provider, media_kind],
            )?;
            return Ok(());
        }

        let placeholders = vec!["?"; provider_entry_ids.len()].join(", ");
        let sql = format!(
            "DELETE FROM collection_entry
             WHERE account_id = ?
               AND provider = ?
               AND media_kind = ?
               AND provider_entry_id NOT IN ({placeholders})"
        );
        let mut query_params = vec![
            account_id.to_owned(),
            provider.to_owned(),
            media_kind.to_owned(),
        ];
        query_params.extend(provider_entry_ids);
        self.connection
            .execute(&sql, params_from_iter(query_params.iter()))?;

        Ok(())
    }

    pub fn collection_entry_count(&self, account_id: &str) -> Result<u64, StoreError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*)
             FROM collection_entry
             WHERE account_id = ?1",
            params![account_id],
            |row| row.get::<_, u64>(0),
        )?;

        Ok(count)
    }

    pub fn collection_entry_details(
        &self,
        account_id: &str,
        provider: Provider,
        media_kind: MediaKind,
        provider_entry_id: &str,
    ) -> Result<Option<CollectionEntryDetails>, StoreError> {
        let details = self
            .connection
            .query_row(
                "SELECT id,
                        work_id,
                        status,
                        score_hundred,
                        progress_episodes,
                        progress_chapters,
                        progress_volumes,
                        provider_payload_hash,
                        provider_updated_at_epoch_secs
                 FROM collection_entry
                 WHERE account_id = ?1
                   AND provider = ?2
                   AND media_kind = ?3
                   AND provider_entry_id = ?4",
                params![
                    account_id,
                    provider.as_str(),
                    media_kind.as_str(),
                    provider_entry_id,
                ],
                |row| {
                    let status: String = row.get(2)?;
                    Ok(CollectionEntryDetails {
                        id: row.get(0)?,
                        work_id: row.get(1)?,
                        status: collection_status_from_str(&status),
                        score_hundred: optional_u8(row.get::<_, Option<i64>>(3)?),
                        progress_episodes: optional_u32(row.get::<_, Option<i64>>(4)?),
                        progress_chapters: optional_u32(row.get::<_, Option<i64>>(5)?),
                        progress_volumes: optional_u32(row.get::<_, Option<i64>>(6)?),
                        provider_payload_hash: row.get(7)?,
                        provider_updated_at_epoch_secs: row.get(8)?,
                    })
                },
            )
            .optional()?;

        Ok(details)
    }

    pub fn backfill_collection_entry_work_ids(
        &self,
        account_id: &str,
        media_kind: MediaKind,
        providers: &[Provider],
    ) -> Result<usize, StoreError> {
        let mut updated = 0usize;
        for provider in providers {
            updated += self.connection.execute(
                "UPDATE collection_entry
                 SET work_id = (
                    SELECT edge.work_id
                    FROM external_id_edge edge
                    WHERE edge.provider = collection_entry.provider
                      AND edge.media_kind = collection_entry.media_kind
                      AND edge.external_id = collection_entry.provider_entry_id
                      AND edge.stale = 0
                 )
                 WHERE account_id = ?1
                   AND provider = ?2
                   AND media_kind = ?3
                   AND work_id IS NULL
                   AND EXISTS (
                    SELECT 1
                    FROM external_id_edge edge
                    WHERE edge.provider = collection_entry.provider
                      AND edge.media_kind = collection_entry.media_kind
                      AND edge.external_id = collection_entry.provider_entry_id
                      AND edge.stale = 0
                   )",
                params![account_id, provider.as_str(), media_kind.as_str()],
            )?;
        }

        Ok(updated)
    }

    pub fn upsert_bangumi_episode_collection_snapshot(
        &self,
        account_id: &str,
        snapshot: &BangumiEpisodeCollectionSnapshot,
    ) -> Result<usize, StoreError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.upsert_bangumi_episode_collection_snapshot_entries(account_id, snapshot);

        match result {
            Ok(imported) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(imported)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    pub fn upsert_collection_snapshot_with_bangumi_episode_snapshots(
        &self,
        account_id: &str,
        snapshot: &ProviderCollectionSnapshot,
        episode_snapshots: &[BangumiEpisodeCollectionSnapshot],
    ) -> Result<usize, StoreError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let imported = self.upsert_collection_snapshot_entries(account_id, snapshot)?;
            self.remove_bangumi_episode_collections_missing_from_snapshot(account_id, snapshot)?;
            for episode_snapshot in episode_snapshots {
                self.upsert_bangumi_episode_collection_snapshot_entries(
                    account_id,
                    episode_snapshot,
                )?;
            }
            Ok(imported)
        })();

        match result {
            Ok(imported) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(imported)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn upsert_bangumi_episode_collection_snapshot_entries(
        &self,
        account_id: &str,
        snapshot: &BangumiEpisodeCollectionSnapshot,
    ) -> Result<usize, StoreError> {
        self.connection.execute(
            "DELETE FROM bangumi_episode_collection
             WHERE account_id = ?1
               AND subject_id = ?2",
            params![account_id, snapshot.subject_id()],
        )?;

        for entry in snapshot.entries() {
            self.connection.execute(
                "INSERT INTO bangumi_episode_collection(
                    account_id,
                    subject_id,
                    episode_id,
                    episode_sort,
                    collection_type,
                    updated_at_epoch_secs
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    account_id,
                    snapshot.subject_id(),
                    entry.episode_id() as i64,
                    entry.sort(),
                    i64::from(entry.collection_type()),
                    entry.updated_at_epoch_secs(),
                ],
            )?;
        }

        Ok(snapshot.entries().len())
    }

    fn remove_bangumi_episode_collections_missing_from_snapshot(
        &self,
        account_id: &str,
        snapshot: &ProviderCollectionSnapshot,
    ) -> Result<(), StoreError> {
        if snapshot.provider() != Provider::Bangumi || snapshot.media_kind() != MediaKind::Anime {
            return Ok(());
        }

        let subject_ids = snapshot
            .entries()
            .iter()
            .map(|entry| entry.provider_entry_id().to_owned())
            .collect::<Vec<_>>();

        if subject_ids.is_empty() {
            self.connection.execute(
                "DELETE FROM bangumi_episode_collection
                 WHERE account_id = ?1",
                params![account_id],
            )?;
            return Ok(());
        }

        let placeholders = vec!["?"; subject_ids.len()].join(", ");
        let sql = format!(
            "DELETE FROM bangumi_episode_collection
             WHERE account_id = ?
               AND subject_id NOT IN ({placeholders})"
        );
        let mut query_params = vec![account_id.to_owned()];
        query_params.extend(subject_ids);
        self.connection
            .execute(&sql, params_from_iter(query_params.iter()))?;

        Ok(())
    }

    pub fn bangumi_episode_ids_for_done_prefix(
        &self,
        account_id: &str,
        subject_id: &str,
        count: usize,
    ) -> Result<Vec<u64>, StoreError> {
        self.bangumi_episode_ids_for_prefix(account_id, subject_id, count, true)
    }

    pub fn bangumi_episode_ids_for_progress_prefix(
        &self,
        account_id: &str,
        subject_id: &str,
        count: usize,
    ) -> Result<Vec<u64>, StoreError> {
        self.bangumi_episode_ids_for_prefix(account_id, subject_id, count, false)
    }

    fn bangumi_episode_ids_for_prefix(
        &self,
        account_id: &str,
        subject_id: &str,
        count: usize,
        done_only: bool,
    ) -> Result<Vec<u64>, StoreError> {
        let limit = count as i64;
        let collection_type_filter = if done_only {
            "AND collection_type = 2"
        } else {
            ""
        };
        let mut statement = self.connection.prepare(&format!(
            "SELECT episode_id
             FROM bangumi_episode_collection
             WHERE account_id = ?1
               AND subject_id = ?2
               {collection_type_filter}
             ORDER BY episode_sort ASC, episode_id ASC
             LIMIT ?3"
        ))?;
        let rows = statement
            .query_map(params![account_id, subject_id, limit], |row| {
                row.get::<_, i64>(0).map(|value| value as u64)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn field_provenance_fields(
        &self,
        collection_entry_id: i64,
    ) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT field_name
             FROM field_provenance
             WHERE collection_entry_id = ?1
             ORDER BY field_name",
        )?;
        let fields = statement
            .query_map(params![collection_entry_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(fields)
    }

    pub fn field_observations(
        &self,
        collection_entry_id: i64,
    ) -> Result<HashMap<SyncField, FieldObservationDetails>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT fp.field_name,
                    fp.observed_value_hash,
                    fp.previous_observed_value_hash,
                    fp.observation_version,
                    fp.last_changed_observation_version,
                    fp.change_origin,
                    fp.attributed_write_journal_field_id,
                    fp.externally_cleared,
                    fp.pending_external_change,
                    wjf.source_provider,
                    wjf.source_value_hash
             FROM field_provenance fp
             LEFT JOIN write_journal_field wjf
               ON wjf.id = fp.attributed_write_journal_field_id
             WHERE fp.collection_entry_id = ?1
             ORDER BY fp.field_name",
        )?;
        let observations = statement
            .query_map(params![collection_entry_id], |row| {
                let field_name: String = row.get(0)?;
                let change_origin: String = row.get(5)?;
                let attributed_source_provider: Option<String> = row.get(9)?;
                let field = sync_field_from_str(&field_name);
                Ok((
                    field,
                    FieldObservationDetails {
                        field,
                        observed_value_hash: row.get(1)?,
                        previous_observed_value_hash: row.get(2)?,
                        observation_version: row.get(3)?,
                        last_changed_observation_version: row.get(4)?,
                        change_origin: field_observation_change_origin_from_str(&change_origin),
                        attributed_write_journal_field_id: row.get(6)?,
                        externally_cleared: row.get(7)?,
                        pending_external_change: row.get(8)?,
                        attributed_source_provider: attributed_source_provider
                            .as_deref()
                            .map(provider_from_str),
                        attributed_source_value_hash: row.get(10)?,
                    },
                ))
            })?
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(observations)
    }

    pub fn collection_entries_for_planning(
        &self,
        account_id: &str,
        media_kind: MediaKind,
    ) -> Result<Vec<PlanningCollectionEntry>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id,
                    work_id,
                    provider,
                    provider_entry_id,
                    media_kind,
                    status,
                    score_hundred,
                    progress_episodes,
                    progress_chapters,
                    progress_volumes,
                    provider_updated_at_epoch_secs
             FROM collection_entry
             WHERE account_id = ?1
               AND media_kind = ?2
             ORDER BY work_id, provider, provider_entry_id",
        )?;

        let entries = statement
            .query_map(params![account_id, media_kind.as_str()], |row| {
                let provider: String = row.get(2)?;
                let media_kind: String = row.get(4)?;
                let status: String = row.get(5)?;
                Ok(PlanningCollectionEntry {
                    collection_entry_id: row.get(0)?,
                    work_id: row.get(1)?,
                    provider: provider_from_str(&provider),
                    provider_entry_id: row.get(3)?,
                    media_kind: media_kind_from_str(&media_kind),
                    status: collection_status_from_str(&status),
                    score_hundred: optional_u8(row.get::<_, Option<i64>>(6)?),
                    progress_episodes: optional_u32(row.get::<_, Option<i64>>(7)?),
                    progress_chapters: optional_u32(row.get::<_, Option<i64>>(8)?),
                    progress_volumes: optional_u32(row.get::<_, Option<i64>>(9)?),
                    provider_updated_at_epoch_secs: row.get(10)?,
                    field_observations: HashMap::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut entries = entries;
        for entry in &mut entries {
            entry.field_observations = self.field_observations(entry.collection_entry_id)?;
        }

        Ok(entries)
    }

    pub fn external_ids_for_work(
        &self,
        work_id: i64,
        media_kind: MediaKind,
    ) -> Result<Vec<WorkExternalId>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT provider, media_kind, external_id
             FROM external_id_edge
             WHERE work_id = ?1
               AND media_kind = ?2
               AND stale = 0
             ORDER BY provider, external_id",
        )?;

        let ids = statement
            .query_map(params![work_id, media_kind.as_str()], |row| {
                let provider: String = row.get(0)?;
                let media_kind: String = row.get(1)?;
                Ok(WorkExternalId {
                    provider: provider_from_str(&provider),
                    media_kind: media_kind_from_str(&media_kind),
                    external_id: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ids)
    }

    fn collection_snapshot_generation(
        &self,
        account_id: &str,
        provider: Provider,
        media_kind: MediaKind,
    ) -> Result<i64, StoreError> {
        let generation = self
            .connection
            .query_row(
                "SELECT generation
                 FROM collection_snapshot_state
                 WHERE account_id = ?1
                   AND provider = ?2
                   AND media_kind = ?3",
                params![account_id, provider.as_str(), media_kind.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);

        Ok(generation)
    }

    pub fn search_provider_items(
        &self,
        media_kind: MediaKind,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ProviderItemCandidate>, StoreError> {
        let sql = "SELECT p.provider,
                    p.media_kind,
                    p.external_id,
                    p.canonical_title,
                    p.format,
                    p.release_year,
                    COALESCE((
                        SELECT group_concat(alias, char(31))
                        FROM provider_item_alias
                        WHERE provider_item_id = p.id
                    ), '') AS aliases
             FROM provider_item_fts f
             JOIN provider_item p ON p.id = f.provider_item_id
             WHERE provider_item_fts MATCH ?1
               AND p.media_kind = ?2
             ORDER BY lower(p.canonical_title), p.provider, p.external_id
             LIMIT ?3";
        let mut statement = self.connection.prepare(sql)?;

        self.search_provider_items_with_statement(
            &mut statement,
            params![query, media_kind.as_str(), limit],
        )
    }

    pub fn search_all_provider_items(
        &self,
        media_kind: MediaKind,
        query: &str,
    ) -> Result<Vec<ProviderItemCandidate>, StoreError> {
        let sql = "SELECT p.provider,
                    p.media_kind,
                    p.external_id,
                    p.canonical_title,
                    p.format,
                    p.release_year,
                    COALESCE((
                        SELECT group_concat(alias, char(31))
                        FROM provider_item_alias
                        WHERE provider_item_id = p.id
                    ), '') AS aliases
             FROM provider_item_fts f
             JOIN provider_item p ON p.id = f.provider_item_id
             WHERE provider_item_fts MATCH ?1
               AND p.media_kind = ?2
             ORDER BY lower(p.canonical_title), p.provider, p.external_id
            ";
        let mut statement = self.connection.prepare(sql)?;

        self.search_provider_items_with_statement(
            &mut statement,
            params![query, media_kind.as_str()],
        )
    }

    fn search_provider_items_with_statement<P>(
        &self,
        statement: &mut rusqlite::Statement<'_>,
        params: P,
    ) -> Result<Vec<ProviderItemCandidate>, StoreError>
    where
        P: rusqlite::Params,
    {
        let candidates = statement
            .query_map(params, |row| {
                Ok(ProviderItemCandidate {
                    provider: provider_from_str(row.get::<_, String>(0)?.as_str()),
                    media_kind: media_kind_from_str(row.get::<_, String>(1)?.as_str()),
                    external_id: row.get(2)?,
                    canonical_title: row.get(3)?,
                    format: row.get(4)?,
                    release_year: row.get(5)?,
                    aliases: split_aliases(&row.get::<_, String>(6)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(candidates)
    }

    pub fn upsert_manual_mapping(&self, input: ManualMappingInput) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO manual_mapping(
                work_id,
                provider,
                media_kind,
                external_id,
                decision,
                note
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(provider, media_kind, external_id, decision) DO UPDATE SET
                work_id = excluded.work_id,
                note = excluded.note",
            params![
                input.work_id,
                input.provider.as_str(),
                input.media_kind.as_str(),
                input.external_id,
                input.decision.as_str(),
                input.note,
            ],
        )?;

        Ok(())
    }

    pub fn manual_mapping_decision(
        &self,
        provider: Provider,
        media_kind: MediaKind,
        external_id: &str,
    ) -> Result<Option<ManualMappingDecision>, StoreError> {
        let decision = self
            .connection
            .query_row(
                "SELECT decision
                 FROM manual_mapping
                 WHERE provider = ?1
                   AND media_kind = ?2
                   AND external_id = ?3
                 ORDER BY CASE decision WHEN 'ignore' THEN 0 ELSE 1 END
                 LIMIT 1",
                params![provider.as_str(), media_kind.as_str(), external_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        Ok(decision.map(|value| manual_mapping_decision_from_str(&value)))
    }

    pub fn record_write_journal_intent(
        &self,
        input: WriteJournalIntent,
    ) -> Result<WriteJournalIntentOutcome, StoreError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let request_hash = stable_hash(&input.request_body);
            let existing = self
                .connection
                .query_row(
                    "SELECT id, request_hash, result_status
                     FROM write_journal
                     WHERE provider = ?1
                       AND account_id = ?2
                       AND operation_id = ?3",
                    params![
                        input.provider.as_str(),
                        input.account_id,
                        input.operation_id,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            let existing_request_matches = existing
                .as_ref()
                .is_none_or(|(_, existing_request_hash, _)| existing_request_hash == &request_hash);
            if !existing_request_matches {
                return Err(StoreError::AmbiguousWriteJournalIntent {
                    provider: input.provider,
                    target_provider_entry_id: input.target_provider_entry_id.clone(),
                });
            }
            let already_succeeded = existing
                .as_ref()
                .is_some_and(|(_, _, status)| status == WriteJournalStatus::Succeeded.as_str());
            let ambiguous_attempt = existing
                .as_ref()
                .is_some_and(|(_, _, status)| status == WriteJournalStatus::Attempted.as_str());
            if ambiguous_attempt {
                return Err(StoreError::AmbiguousWriteJournalIntent {
                    provider: input.provider,
                    target_provider_entry_id: input.target_provider_entry_id.clone(),
                });
            }

            let replay_fields = if let Some((journal_id, _, _)) = existing.as_ref() {
                let mut statement = self.connection.prepare(
                    "SELECT field_name,
                            basis_observation_version,
                            basis_snapshot_generation,
                            before_value_hash,
                            source_value_hash,
                            expected_value_hash,
                            consumed_at IS NOT NULL
                     FROM write_journal_field
                     WHERE write_journal_id = ?1",
                )?;
                let fields = statement
                    .query_map(params![journal_id], |row| {
                        Ok((
                            sync_field_from_str(&row.get::<_, String>(0)?),
                            (
                                row.get::<_, Option<i64>>(1)?,
                                row.get::<_, i64>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, String>(4)?,
                                row.get::<_, String>(5)?,
                                row.get::<_, bool>(6)?,
                            ),
                        ))
                    })?
                    .collect::<Result<HashMap<_, _>, _>>()?;
                fields
            } else {
                HashMap::new()
            };

            if already_succeeded
                && input.fields.iter().any(|field| {
                    replay_fields.get(&field.field).is_none_or(
                        |(_, _, before_hash, source_hash, expected_hash, _)| {
                            before_hash != &field.before_value_hash
                                || source_hash != &field.source_value_hash
                                || expected_hash != &field.expected_value_hash
                        },
                    )
                })
            {
                return Err(StoreError::AmbiguousWriteJournalIntent {
                    provider: input.provider,
                    target_provider_entry_id: input.target_provider_entry_id.clone(),
                });
            }

            let target_entry = self
                .connection
                .query_row(
                    "SELECT id, work_id
                     FROM collection_entry
                     WHERE account_id = ?1
                       AND provider = ?2
                       AND media_kind = ?3
                       AND provider_entry_id = ?4",
                    params![
                        input.account_id,
                        input.provider.as_str(),
                        input.media_kind.as_str(),
                        input.target_provider_entry_id,
                    ],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .optional()?;
            let missing_entry_hash = stable_hash(MISSING_ENTRY_VALUE);
            let is_add = input
                .fields
                .iter()
                .all(|field| field.before_value_hash == missing_entry_hash);
            let mixes_add_and_update_fields = input
                .fields
                .iter()
                .any(|field| field.before_value_hash == missing_entry_hash)
                && !is_add;
            let fallback_field = input
                .fields
                .first()
                .map(|field| field.field)
                .unwrap_or(SyncField::Status);
            let current_snapshot_generation = self.collection_snapshot_generation(
                &input.account_id,
                input.provider,
                input.media_kind,
            )?;
            if mixes_add_and_update_fields {
                return Err(StoreError::StaleWriteJournalIntent {
                    provider: input.provider,
                    target_provider_entry_id: input.target_provider_entry_id.clone(),
                    field: fallback_field,
                });
            }
            if is_add {
                let edge_work_id = self
                    .connection
                    .query_row(
                        "SELECT work_id
                         FROM external_id_edge
                         WHERE provider = ?1
                           AND media_kind = ?2
                           AND external_id = ?3
                           AND stale = 0",
                        params![
                            input.provider.as_str(),
                            input.media_kind.as_str(),
                            input.target_provider_entry_id,
                        ],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?;
                if edge_work_id != Some(input.work_id) {
                    return Err(StoreError::StaleWriteJournalIntent {
                        provider: input.provider,
                        target_provider_entry_id: input.target_provider_entry_id.clone(),
                        field: fallback_field,
                    });
                }
            }

            let source_entry_ids = {
                let mut statement = self.connection.prepare(
                    "SELECT id
                     FROM collection_entry
                     WHERE account_id = ?1
                       AND provider = ?2
                       AND media_kind = ?3
                       AND work_id = ?4
                     ORDER BY id
                     LIMIT 2",
                )?;
                let entry_ids = statement
                    .query_map(
                        params![
                            input.account_id,
                            input.source_provider.as_str(),
                            input.media_kind.as_str(),
                            input.work_id,
                        ],
                        |row| row.get::<_, i64>(0),
                    )?
                    .collect::<Result<Vec<_>, _>>()?;
                entry_ids
            };
            if source_entry_ids.len() != 1 {
                return Err(StoreError::StaleWriteJournalIntent {
                    provider: input.provider,
                    target_provider_entry_id: input.target_provider_entry_id.clone(),
                    field: fallback_field,
                });
            }
            let source_entry_id = source_entry_ids[0];
            let mut basis_versions = Vec::with_capacity(input.fields.len());

            for field in &input.fields {
                let source_observed_value_hash = self
                    .connection
                    .query_row(
                        "SELECT observed_value_hash
                         FROM field_provenance
                         WHERE collection_entry_id = ?1
                           AND field_name = ?2
                           AND provider = ?3",
                        params![
                            source_entry_id,
                            field.field.as_str(),
                            input.source_provider.as_str(),
                        ],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                if source_observed_value_hash.as_deref() != Some(field.source_value_hash.as_str()) {
                    return Err(StoreError::StaleWriteJournalIntent {
                        provider: input.provider,
                        target_provider_entry_id: input.target_provider_entry_id.clone(),
                        field: field.field,
                    });
                }

                if is_add {
                    let was_consumed = replay_fields
                        .get(&field.field)
                        .is_some_and(|(_, _, _, _, _, consumed)| *consumed);
                    let Some((collection_entry_id, work_id)) = target_entry else {
                        if already_succeeded {
                            let replay_basis_generation = replay_fields
                                .get(&field.field)
                                .map(|(_, generation, _, _, _, _)| *generation);
                            if was_consumed
                                || replay_basis_generation != Some(current_snapshot_generation)
                            {
                                return Err(StoreError::StaleWriteJournalIntent {
                                    provider: input.provider,
                                    target_provider_entry_id: input
                                        .target_provider_entry_id
                                        .clone(),
                                    field: field.field,
                                });
                            }
                        }
                        basis_versions.push(None);
                        continue;
                    };
                    if !already_succeeded || work_id != Some(input.work_id) {
                        return Err(StoreError::StaleWriteJournalIntent {
                            provider: input.provider,
                            target_provider_entry_id: input.target_provider_entry_id.clone(),
                            field: field.field,
                        });
                    }
                    let observation = self
                        .connection
                        .query_row(
                            "SELECT observed_value_hash, observation_version
                             FROM field_provenance
                             WHERE collection_entry_id = ?1
                               AND field_name = ?2
                               AND provider = ?3",
                            params![
                                collection_entry_id,
                                field.field.as_str(),
                                input.provider.as_str(),
                            ],
                            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                        )
                        .optional()?;
                    let Some((observed_value_hash, observation_version)) = observation else {
                        return Err(StoreError::StaleWriteJournalIntent {
                            provider: input.provider,
                            target_provider_entry_id: input.target_provider_entry_id.clone(),
                            field: field.field,
                        });
                    };
                    if observed_value_hash != field.expected_value_hash {
                        return Err(StoreError::StaleWriteJournalIntent {
                            provider: input.provider,
                            target_provider_entry_id: input.target_provider_entry_id.clone(),
                            field: field.field,
                        });
                    }
                    basis_versions.push(Some(observation_version));
                    continue;
                }

                let Some((collection_entry_id, work_id)) = target_entry else {
                    return Err(StoreError::StaleWriteJournalIntent {
                        provider: input.provider,
                        target_provider_entry_id: input.target_provider_entry_id.clone(),
                        field: field.field,
                    });
                };
                if work_id != Some(input.work_id) {
                    return Err(StoreError::StaleWriteJournalIntent {
                        provider: input.provider,
                        target_provider_entry_id: input.target_provider_entry_id.clone(),
                        field: field.field,
                    });
                }
                let observation = self
                    .connection
                    .query_row(
                        "SELECT observed_value_hash, observation_version
                         FROM field_provenance
                         WHERE collection_entry_id = ?1
                           AND field_name = ?2
                           AND provider = ?3",
                        params![
                            collection_entry_id,
                            field.field.as_str(),
                            input.provider.as_str()
                        ],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                    )
                    .optional()?;
                let Some((observed_value_hash, observation_version)) = observation else {
                    return Err(StoreError::StaleWriteJournalIntent {
                        provider: input.provider,
                        target_provider_entry_id: input.target_provider_entry_id.clone(),
                        field: field.field,
                    });
                };
                let replay_was_consumed = replay_fields
                    .get(&field.field)
                    .is_some_and(|(_, _, _, _, _, consumed)| *consumed);
                let replay_basis_observation_version = replay_fields
                    .get(&field.field)
                    .and_then(|(basis, _, _, _, _, _)| *basis);
                let target_is_original_basis = observed_value_hash == field.before_value_hash;
                let target_is_succeeded_result =
                    already_succeeded && observed_value_hash == field.expected_value_hash;
                if !target_is_original_basis && !target_is_succeeded_result {
                    return Err(StoreError::StaleWriteJournalIntent {
                        provider: input.provider,
                        target_provider_entry_id: input.target_provider_entry_id.clone(),
                        field: field.field,
                    });
                }
                if already_succeeded
                    && target_is_original_basis
                    && (replay_was_consumed
                        || replay_basis_observation_version != Some(observation_version))
                {
                    return Err(StoreError::StaleWriteJournalIntent {
                        provider: input.provider,
                        target_provider_entry_id: input.target_provider_entry_id.clone(),
                        field: field.field,
                    });
                }
                basis_versions.push(Some(observation_version));
            }

            if already_succeeded {
                return Ok(WriteJournalIntentOutcome::AlreadySucceeded);
            }

            self.connection.execute(
                "INSERT INTO write_journal(
                    provider,
                    account_id,
                    operation_id,
                    request_hash,
                    response_hash,
                    completed_at,
                    result_status
                 )
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)
                 ON CONFLICT(provider, account_id, operation_id) DO UPDATE SET
                    request_hash = excluded.request_hash,
                    response_hash = NULL,
                    started_at = CURRENT_TIMESTAMP,
                    completed_at = NULL,
                    result_status = excluded.result_status",
                params![
                    input.provider.as_str(),
                    input.account_id,
                    input.operation_id,
                    request_hash,
                    WriteJournalStatus::Attempted.as_str(),
                ],
            )?;
            let write_journal_id = self.connection.query_row(
                "SELECT id
                 FROM write_journal
                 WHERE provider = ?1
                   AND account_id = ?2
                   AND operation_id = ?3",
                params![
                    input.provider.as_str(),
                    input.account_id,
                    input.operation_id,
                ],
                |row| row.get::<_, i64>(0),
            )?;
            self.connection.execute(
                "DELETE FROM write_journal_field WHERE write_journal_id = ?1",
                params![write_journal_id],
            )?;
            for (field, basis_observation_version) in
                input.fields.iter().zip(basis_versions.into_iter())
            {
                self.connection.execute(
                    "INSERT INTO write_journal_field(
                        write_journal_id,
                        work_id,
                        media_kind,
                        source_provider,
                        target_provider_entry_id,
                        field_name,
                        basis_observation_version,
                        basis_snapshot_generation,
                        before_value_hash,
                        source_value_hash,
                        expected_value_hash
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        write_journal_id,
                        input.work_id,
                        input.media_kind.as_str(),
                        input.source_provider.as_str(),
                        input.target_provider_entry_id,
                        field.field.as_str(),
                        basis_observation_version,
                        current_snapshot_generation,
                        field.before_value_hash,
                        field.source_value_hash,
                        field.expected_value_hash,
                    ],
                )?;
            }

            Ok(WriteJournalIntentOutcome::Recorded)
        })();

        match result {
            Ok(outcome) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(outcome)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    pub fn complete_write_journal(&self, input: WriteJournalCompletion) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE write_journal
             SET response_hash = ?4,
                 completed_at = CURRENT_TIMESTAMP,
                 result_status = ?5
             WHERE provider = ?1
               AND account_id = ?2
               AND operation_id = ?3",
            params![
                input.provider.as_str(),
                input.account_id,
                input.operation_id,
                input.response_body.as_deref().map(stable_hash),
                input.result_status.as_str(),
            ],
        )?;

        Ok(())
    }

    pub fn write_journal_entries(
        &self,
        account_id: &str,
        provider: Provider,
    ) -> Result<Vec<WriteJournalEntryDetails>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT provider,
                    account_id,
                    operation_id,
                    request_hash,
                    response_hash,
                    result_status,
                    completed_at
             FROM write_journal
             WHERE account_id = ?1
               AND provider = ?2
             ORDER BY operation_id",
        )?;

        let entries = statement
            .query_map(params![account_id, provider.as_str()], |row| {
                let provider: String = row.get(0)?;
                let result_status: String = row.get(5)?;
                Ok(WriteJournalEntryDetails {
                    provider: provider_from_str(&provider),
                    account_id: row.get(1)?,
                    operation_id: row.get(2)?,
                    request_hash: row.get(3)?,
                    response_hash: row.get(4)?,
                    result_status: write_journal_status_from_str(&result_status),
                    completed_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    pub fn write_journal_fields(
        &self,
        account_id: &str,
        provider: Provider,
        operation_id: &str,
    ) -> Result<Vec<WriteJournalFieldDetails>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT wjf.field_name,
                    wjf.work_id,
                    wjf.media_kind,
                    wjf.source_provider,
                    wjf.target_provider_entry_id,
                    wjf.basis_observation_version,
                    wjf.basis_snapshot_generation,
                    wjf.before_value_hash,
                    wjf.source_value_hash,
                    wjf.expected_value_hash,
                    wjf.attributed_collection_entry_id,
                    wjf.attributed_observation_version
             FROM write_journal_field wjf
             JOIN write_journal wj ON wj.id = wjf.write_journal_id
             WHERE wj.account_id = ?1
               AND wj.provider = ?2
               AND wj.operation_id = ?3
             ORDER BY wjf.field_name",
        )?;
        let fields = statement
            .query_map(
                params![account_id, provider.as_str(), operation_id],
                |row| {
                    let field_name: String = row.get(0)?;
                    let media_kind: String = row.get(2)?;
                    let source_provider: String = row.get(3)?;
                    Ok(WriteJournalFieldDetails {
                        field: sync_field_from_str(&field_name),
                        work_id: row.get(1)?,
                        media_kind: media_kind_from_str(&media_kind),
                        source_provider: provider_from_str(&source_provider),
                        target_provider_entry_id: row.get(4)?,
                        basis_observation_version: row.get(5)?,
                        basis_snapshot_generation: row.get(6)?,
                        before_value_hash: row.get(7)?,
                        source_value_hash: row.get(8)?,
                        expected_value_hash: row.get(9)?,
                        attributed_collection_entry_id: row.get(10)?,
                        attributed_observation_version: row.get(11)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(fields)
    }

    pub fn upsert_provider_credential(
        &self,
        input: ProviderCredentialInput,
    ) -> Result<(), StoreError> {
        validate_provider_credential(&input)?;

        let (refresh_token_state, refresh_token_expires_at_epoch_secs) =
            refresh_token_state_to_sql(input.refresh_token);
        self.connection.execute(
            "INSERT INTO provider_credential(
                provider,
                account_id,
                auth_flow,
                bootstrap_mode,
                credential_store_ref,
                access_token_expires_at_epoch_secs,
                refresh_token_state,
                refresh_token_expires_at_epoch_secs,
                last_refresh_at_epoch_secs,
                last_reauth_request_at_epoch_secs
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(provider, account_id) DO UPDATE SET
                auth_flow = excluded.auth_flow,
                bootstrap_mode = excluded.bootstrap_mode,
                credential_store_ref = excluded.credential_store_ref,
                access_token_expires_at_epoch_secs = excluded.access_token_expires_at_epoch_secs,
                refresh_token_state = excluded.refresh_token_state,
                refresh_token_expires_at_epoch_secs = excluded.refresh_token_expires_at_epoch_secs,
                last_refresh_at_epoch_secs = excluded.last_refresh_at_epoch_secs,
                last_reauth_request_at_epoch_secs = excluded.last_reauth_request_at_epoch_secs,
                updated_at = CURRENT_TIMESTAMP",
            params![
                input.provider.as_str(),
                input.account_id,
                auth_flow_to_str(input.auth_flow),
                bootstrap_mode_to_str(input.bootstrap_mode),
                input.credential_store_ref,
                input.access_token_expires_at_epoch_secs,
                refresh_token_state,
                refresh_token_expires_at_epoch_secs,
                input.last_refresh_at_epoch_secs,
                input.last_reauth_request_at_epoch_secs,
            ],
        )?;

        Ok(())
    }

    pub fn upsert_provider_credential_exchange(
        &self,
        result: ProviderCredentialExchangeResult,
    ) -> Result<(), StoreError> {
        self.upsert_provider_credential(ProviderCredentialInput {
            provider: result.provider,
            account_id: result.account_id,
            auth_flow: result.auth_flow,
            bootstrap_mode: result.bootstrap_mode,
            credential_store_ref: result.credential_store_ref,
            access_token_expires_at_epoch_secs: result.access_token_expires_at_epoch_secs,
            refresh_token: result.refresh_token,
            last_refresh_at_epoch_secs: result.last_refresh_at_epoch_secs,
            last_reauth_request_at_epoch_secs: None,
        })
    }

    pub fn begin_provider_credential_refresh_attempt(
        &self,
        input: ProviderCredentialRefreshAttemptInput,
    ) -> Result<i64, StoreError> {
        self.connection.execute(
            "INSERT INTO provider_credential_refresh_attempt(
                provider,
                account_id,
                previous_credential_store_ref,
                status,
                started_at_epoch_secs
             )
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.provider.as_str(),
                input.account_id,
                input.previous_credential_store_ref,
                refresh_attempt_status_to_str(ProviderCredentialRefreshAttemptStatus::Pending),
                input.started_at_epoch_secs,
            ],
        )?;

        Ok(self.connection.last_insert_rowid())
    }

    pub fn finalize_provider_credential_refresh_attempt(
        &self,
        attempt_id: i64,
        result: ProviderCredentialExchangeResult,
        completed_at_epoch_secs: i64,
    ) -> Result<ProviderCredentialRefreshAttemptFinalizeStatus, StoreError> {
        let Some(attempt) = self.provider_credential_refresh_attempt(attempt_id)? else {
            return Err(StoreError::InvalidProviderCredential {
                provider: result.provider,
                reason: "refresh attempt does not exist".to_owned(),
            });
        };
        if attempt.status != ProviderCredentialRefreshAttemptStatus::Pending {
            return Err(StoreError::InvalidProviderCredential {
                provider: result.provider,
                reason: "refresh attempt is not pending".to_owned(),
            });
        }
        if attempt.provider != result.provider || attempt.account_id != result.account_id {
            return Err(StoreError::InvalidProviderCredential {
                provider: result.provider,
                reason: "refresh attempt does not match exchange result".to_owned(),
            });
        }

        validate_provider_credential(&ProviderCredentialInput {
            provider: result.provider,
            account_id: result.account_id.clone(),
            auth_flow: result.auth_flow,
            bootstrap_mode: result.bootstrap_mode,
            credential_store_ref: result.credential_store_ref.clone(),
            access_token_expires_at_epoch_secs: result.access_token_expires_at_epoch_secs,
            refresh_token: result.refresh_token,
            last_refresh_at_epoch_secs: result.last_refresh_at_epoch_secs,
            last_reauth_request_at_epoch_secs: None,
        })?;

        let (refresh_token_state, refresh_token_expires_at_epoch_secs) =
            refresh_token_state_to_sql(result.refresh_token);

        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let finalize_result = (|| {
            let updated = self.connection.execute(
                "UPDATE provider_credential
                 SET auth_flow = ?1,
                     bootstrap_mode = ?2,
                     credential_store_ref = ?3,
                     access_token_expires_at_epoch_secs = ?4,
                     refresh_token_state = ?5,
                     refresh_token_expires_at_epoch_secs = ?6,
                     last_refresh_at_epoch_secs = ?7,
                     last_reauth_request_at_epoch_secs = NULL,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE provider = ?8
                   AND account_id = ?9
                   AND credential_store_ref = ?10",
                params![
                    auth_flow_to_str(result.auth_flow),
                    bootstrap_mode_to_str(result.bootstrap_mode),
                    result.credential_store_ref,
                    result.access_token_expires_at_epoch_secs,
                    refresh_token_state,
                    refresh_token_expires_at_epoch_secs,
                    result.last_refresh_at_epoch_secs,
                    result.provider.as_str(),
                    result.account_id,
                    attempt.previous_credential_store_ref,
                ],
            )?;

            let (status, failure_kind, outcome) = if updated == 1 {
                (
                    ProviderCredentialRefreshAttemptStatus::Committed,
                    None,
                    ProviderCredentialRefreshAttemptFinalizeStatus::Committed,
                )
            } else {
                (
                    ProviderCredentialRefreshAttemptStatus::Failed,
                    Some("credential_store_ref_conflict"),
                    ProviderCredentialRefreshAttemptFinalizeStatus::Conflict,
                )
            };

            self.connection.execute(
                "UPDATE provider_credential_refresh_attempt
                 SET attempted_credential_store_ref = ?1,
                     access_token_expires_at_epoch_secs = ?2,
                     refresh_token_state = ?3,
                     refresh_token_expires_at_epoch_secs = ?4,
                     last_refresh_at_epoch_secs = ?5,
                     status = ?6,
                     failure_kind = ?7,
                     completed_at_epoch_secs = ?8,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?9",
                params![
                    result.credential_store_ref,
                    result.access_token_expires_at_epoch_secs,
                    refresh_token_state,
                    refresh_token_expires_at_epoch_secs,
                    result.last_refresh_at_epoch_secs,
                    refresh_attempt_status_to_str(status),
                    failure_kind,
                    completed_at_epoch_secs,
                    attempt_id,
                ],
            )?;

            Ok(outcome)
        })();

        match finalize_result {
            Ok(outcome) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(outcome)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    pub fn fail_provider_credential_refresh_attempt(
        &self,
        attempt_id: i64,
        failure_kind: &str,
        completed_at_epoch_secs: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE provider_credential_refresh_attempt
             SET status = ?1,
                 failure_kind = ?2,
                 completed_at_epoch_secs = ?3,
                 updated_at = CURRENT_TIMESTAMP
             WHERE id = ?4",
            params![
                refresh_attempt_status_to_str(ProviderCredentialRefreshAttemptStatus::Failed),
                failure_kind,
                completed_at_epoch_secs,
                attempt_id,
            ],
        )?;

        Ok(())
    }

    pub fn provider_credential_refresh_attempt(
        &self,
        attempt_id: i64,
    ) -> Result<Option<ProviderCredentialRefreshAttemptDetails>, StoreError> {
        let attempt = self
            .connection
            .query_row(
                "SELECT id,
                        provider,
                        account_id,
                        previous_credential_store_ref,
                        attempted_credential_store_ref,
                        status,
                        failure_kind,
                        started_at_epoch_secs,
                        completed_at_epoch_secs
                 FROM provider_credential_refresh_attempt
                 WHERE id = ?1",
                params![attempt_id],
                |row| {
                    let provider: String = row.get(1)?;
                    let status: String = row.get(5)?;
                    Ok(ProviderCredentialRefreshAttemptDetails {
                        id: row.get(0)?,
                        provider: provider_from_str(&provider),
                        account_id: row.get(2)?,
                        previous_credential_store_ref: row.get(3)?,
                        attempted_credential_store_ref: row.get(4)?,
                        status: refresh_attempt_status_from_str(&status),
                        failure_kind: row.get(6)?,
                        started_at_epoch_secs: row.get(7)?,
                        completed_at_epoch_secs: row.get(8)?,
                    })
                },
            )
            .optional()?;

        Ok(attempt)
    }

    pub fn provider_credential(
        &self,
        provider: Provider,
        account_id: &str,
    ) -> Result<Option<ProviderCredentialDetails>, StoreError> {
        let details = self
            .connection
            .query_row(
                "SELECT provider,
                        account_id,
                        auth_flow,
                        bootstrap_mode,
                        credential_store_ref,
                        access_token_expires_at_epoch_secs,
                        refresh_token_state,
                        refresh_token_expires_at_epoch_secs,
                        last_refresh_at_epoch_secs,
                        last_reauth_request_at_epoch_secs
                 FROM provider_credential
                 WHERE provider = ?1
                   AND account_id = ?2",
                params![provider.as_str(), account_id],
                |row| {
                    let provider: String = row.get(0)?;
                    let auth_flow: String = row.get(2)?;
                    let bootstrap_mode: String = row.get(3)?;
                    let refresh_token_state: String = row.get(6)?;
                    let refresh_token_expires_at_epoch_secs: Option<i64> = row.get(7)?;
                    Ok(ProviderCredentialDetails {
                        provider: provider_from_str(&provider),
                        account_id: row.get(1)?,
                        auth_flow: auth_flow_from_str(&auth_flow),
                        bootstrap_mode: bootstrap_mode_from_str(&bootstrap_mode),
                        credential_store_ref: row.get(4)?,
                        access_token_expires_at_epoch_secs: row.get(5)?,
                        refresh_token: refresh_token_state_from_sql(
                            &refresh_token_state,
                            refresh_token_expires_at_epoch_secs,
                        ),
                        last_refresh_at_epoch_secs: row.get(8)?,
                        last_reauth_request_at_epoch_secs: row.get(9)?,
                    })
                },
            )
            .optional()?;

        Ok(details)
    }

    pub fn provider_credential_state(
        &self,
        provider: Provider,
        account_id: &str,
    ) -> Result<Option<ProviderCredentialState>, StoreError> {
        let state = self
            .connection
            .query_row(
                "SELECT provider,
                        access_token_expires_at_epoch_secs,
                        refresh_token_state,
                        refresh_token_expires_at_epoch_secs
                 FROM provider_credential
                 WHERE provider = ?1
                   AND account_id = ?2",
                params![provider.as_str(), account_id],
                |row| {
                    let provider: String = row.get(0)?;
                    let refresh_token_state: String = row.get(2)?;
                    let refresh_token_expires_at_epoch_secs: Option<i64> = row.get(3)?;
                    Ok(ProviderCredentialState::available(
                        provider_from_str(&provider),
                        row.get(1)?,
                        refresh_token_state_from_sql(
                            &refresh_token_state,
                            refresh_token_expires_at_epoch_secs,
                        ),
                    ))
                },
            )
            .optional()?;

        Ok(state)
    }

    fn run_migrations(&self) -> Result<(), StoreError> {
        if self.schema_object_exists("provider_item")? {
            self.ensure_optional_schema()?;
            return Ok(());
        }

        for migration in MIGRATIONS {
            self.connection.execute_batch(migration)?;
        }

        self.ensure_optional_schema()?;
        Ok(())
    }

    fn provider_item_id(
        &self,
        provider: Provider,
        media_kind: MediaKind,
        external_id: &str,
    ) -> Result<i64, StoreError> {
        let id = self.connection.query_row(
            "SELECT id
             FROM provider_item
             WHERE provider = ?1
               AND media_kind = ?2
               AND external_id = ?3",
            params![provider.as_str(), media_kind.as_str(), external_id],
            |row| row.get::<_, i64>(0),
        )?;

        Ok(id)
    }

    fn schema_object_exists(&self, name: &str) -> Result<bool, StoreError> {
        let exists = self.connection.query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM sqlite_schema
                WHERE name = ?1
             )",
            params![name],
            |row| row.get::<_, bool>(0),
        )?;

        Ok(exists)
    }

    fn ensure_optional_schema(&self) -> Result<(), StoreError> {
        if !self.column_exists("collection_entry", "provider_updated_at_epoch_secs")? {
            self.connection.execute(
                "ALTER TABLE collection_entry
                 ADD COLUMN provider_updated_at_epoch_secs INTEGER",
                [],
            )?;
        }

        if !self.column_exists("provider_item", "release_year")? {
            self.connection.execute(
                "ALTER TABLE provider_item
                 ADD COLUMN release_year INTEGER",
                [],
            )?;
        }

        self.ensure_provider_credential_table()?;
        self.ensure_provider_credential_refresh_attempt_table()?;
        self.ensure_bangumi_episode_collection_table()?;
        self.ensure_historical_planner_schema()?;
        self.ensure_store_metadata_table()?;
        self.ensure_provider_item_fts_search_text_version()?;
        Ok(())
    }

    fn ensure_provider_credential_table(&self) -> Result<(), StoreError> {
        if self.schema_object_exists("provider_credential")? {
            return Ok(());
        }

        self.connection.execute_batch(include_str!(
            "../../migrations/0003_provider_credentials.sql"
        ))?;
        Ok(())
    }

    fn ensure_provider_credential_refresh_attempt_table(&self) -> Result<(), StoreError> {
        if self.schema_object_exists("provider_credential_refresh_attempt")? {
            return Ok(());
        }

        self.connection.execute_batch(include_str!(
            "../../migrations/0005_provider_credential_refresh_attempt.sql"
        ))?;
        Ok(())
    }

    fn ensure_bangumi_episode_collection_table(&self) -> Result<(), StoreError> {
        if self.schema_object_exists("bangumi_episode_collection")? {
            return Ok(());
        }

        self.connection.execute_batch(include_str!(
            "../../migrations/0006_bangumi_episode_collection.sql"
        ))?;
        Ok(())
    }

    fn ensure_historical_planner_schema(&self) -> Result<(), StoreError> {
        if !self.column_exists("field_provenance", "previous_observed_value_hash")? {
            self.connection.execute(
                "ALTER TABLE field_provenance
                 ADD COLUMN previous_observed_value_hash TEXT",
                [],
            )?;
        }
        if !self.column_exists("field_provenance", "observation_version")? {
            self.connection.execute(
                "ALTER TABLE field_provenance
                 ADD COLUMN observation_version INTEGER NOT NULL DEFAULT 1
                    CHECK (observation_version >= 1)",
                [],
            )?;
        }
        if !self.column_exists("field_provenance", "last_changed_observation_version")? {
            self.connection.execute(
                "ALTER TABLE field_provenance
                 ADD COLUMN last_changed_observation_version INTEGER",
                [],
            )?;
        }
        self.connection.execute(
            "UPDATE field_provenance
             SET last_changed_observation_version = observation_version
             WHERE last_changed_observation_version IS NULL",
            [],
        )?;
        if !self.column_exists("field_provenance", "change_origin")? {
            self.connection.execute(
                "ALTER TABLE field_provenance
                 ADD COLUMN change_origin TEXT NOT NULL DEFAULT 'initial'
                    CHECK (change_origin IN ('initial', 'unchanged', 'external', 'tool_write'))",
                [],
            )?;
        }
        if !self.column_exists("field_provenance", "attributed_write_journal_field_id")? {
            self.connection.execute(
                "ALTER TABLE field_provenance
                 ADD COLUMN attributed_write_journal_field_id INTEGER",
                [],
            )?;
        }
        if !self.column_exists("field_provenance", "externally_cleared")? {
            self.connection.execute(
                "ALTER TABLE field_provenance
                 ADD COLUMN externally_cleared INTEGER NOT NULL DEFAULT 0
                    CHECK (externally_cleared IN (0, 1))",
                [],
            )?;
        }
        if !self.column_exists("field_provenance", "pending_external_change")? {
            self.connection.execute(
                "ALTER TABLE field_provenance
                 ADD COLUMN pending_external_change INTEGER",
                [],
            )?;
        }
        self.connection.execute(
            "UPDATE field_provenance
             SET pending_external_change = CASE
                    WHEN change_origin = 'external'
                     AND previous_observed_value_hash IS NOT NULL
                     AND previous_observed_value_hash != observed_value_hash
                    THEN 1
                    ELSE 0
                 END
             WHERE pending_external_change IS NULL",
            [],
        )?;

        self.connection.execute(
            "UPDATE field_provenance
             SET field_name = 'score'
             WHERE field_name = 'score_hundred'",
            [],
        )?;

        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS write_journal_field (
                id INTEGER PRIMARY KEY,
                write_journal_id INTEGER NOT NULL REFERENCES write_journal(id) ON DELETE CASCADE,
                work_id INTEGER NOT NULL REFERENCES identity_work(id) ON DELETE CASCADE,
                media_kind TEXT NOT NULL,
                source_provider TEXT NOT NULL,
                target_provider_entry_id TEXT NOT NULL,
                field_name TEXT NOT NULL,
                basis_observation_version INTEGER,
                basis_snapshot_generation INTEGER NOT NULL,
                before_value_hash TEXT NOT NULL,
                source_value_hash TEXT NOT NULL,
                expected_value_hash TEXT NOT NULL,
                attributed_collection_entry_id INTEGER REFERENCES collection_entry(id) ON DELETE SET NULL,
                attributed_observation_version INTEGER,
                consumed_at TEXT,
                UNIQUE (write_journal_id, field_name)
             );
             CREATE TABLE IF NOT EXISTS collection_snapshot_state (
                account_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                media_kind TEXT NOT NULL,
                generation INTEGER NOT NULL DEFAULT 0 CHECK (generation >= 0),
                provider_payload_hash TEXT NOT NULL,
                observed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (account_id, provider, media_kind)
             );
             CREATE INDEX IF NOT EXISTS idx_write_journal_field_journal
                ON write_journal_field(write_journal_id);
             CREATE INDEX IF NOT EXISTS idx_write_journal_field_target
                ON write_journal_field(
                    work_id,
                    media_kind,
                    target_provider_entry_id,
                    field_name
                );",
        )?;

        if !self.column_exists("write_journal_field", "source_value_hash")? {
            self.connection.execute(
                "ALTER TABLE write_journal_field
                 ADD COLUMN source_value_hash TEXT",
                [],
            )?;
        }
        self.connection.execute(
            "UPDATE write_journal_field
             SET source_value_hash = expected_value_hash
             WHERE source_value_hash IS NULL",
            [],
        )?;
        if !self.column_exists("write_journal_field", "basis_snapshot_generation")? {
            self.connection.execute(
                "ALTER TABLE write_journal_field
                 ADD COLUMN basis_snapshot_generation INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !self.column_exists("write_journal_field", "consumed_at")? {
            self.connection.execute(
                "ALTER TABLE write_journal_field
                 ADD COLUMN consumed_at TEXT",
                [],
            )?;
        }
        self.connection.execute(
            "UPDATE write_journal_field
             SET consumed_at = COALESCE(consumed_at, CURRENT_TIMESTAMP)
             WHERE attributed_collection_entry_id IS NOT NULL
                OR attributed_observation_version IS NOT NULL",
            [],
        )?;

        self.backfill_current_field_provenance()?;

        Ok(())
    }

    fn backfill_current_field_provenance(&self) -> Result<(), StoreError> {
        let rows = {
            let mut statement = self.connection.prepare(
                "SELECT id,
                        provider,
                        status,
                        score_hundred,
                        progress_episodes,
                        progress_chapters,
                        progress_volumes
                 FROM collection_entry
                 ORDER BY id",
            )?;
            let rows = statement
                .query_map([], |row| {
                    let provider: String = row.get(1)?;
                    let status: String = row.get(2)?;
                    Ok((
                        row.get::<_, i64>(0)?,
                        provider_from_str(&provider),
                        CanonicalFieldState::new(
                            collection_status_from_str(&status),
                            optional_u8(row.get::<_, Option<i64>>(3)?),
                            optional_u32(row.get::<_, Option<i64>>(4)?),
                            optional_u32(row.get::<_, Option<i64>>(5)?),
                            optional_u32(row.get::<_, Option<i64>>(6)?),
                        ),
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };

        for (collection_entry_id, provider, state) in rows {
            for field in default_writable_fields() {
                let value_hash = state
                    .hash(field)
                    .expect("default-writable fields have canonical hashes");
                self.connection.execute(
                    "INSERT OR IGNORE INTO field_provenance(
                        collection_entry_id,
                        field_name,
                        provider,
                        observed_value_hash,
                        source_reliability,
                        observation_version,
                        last_changed_observation_version,
                        change_origin,
                        pending_external_change
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, 1, 1, ?6, 0)",
                    params![
                        collection_entry_id,
                        field.as_str(),
                        provider.as_str(),
                        value_hash,
                        "provider_snapshot",
                        FieldObservationChangeOrigin::Initial.as_str(),
                    ],
                )?;
            }
        }

        Ok(())
    }

    fn ensure_store_metadata_table(&self) -> Result<(), StoreError> {
        self.connection.execute(
            "CREATE TABLE IF NOT EXISTS store_metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        Ok(())
    }

    fn ensure_provider_item_fts_search_text_version(&self) -> Result<(), StoreError> {
        let version = self
            .connection
            .query_row(
                "SELECT value
                 FROM store_metadata
                 WHERE key = ?1",
                params![PROVIDER_ITEM_FTS_SEARCH_TEXT_VERSION_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        if version.as_deref() == Some(PROVIDER_ITEM_FTS_SEARCH_TEXT_VERSION) {
            return Ok(());
        }

        self.rebuild_provider_item_fts_search_text()
    }

    fn rebuild_provider_item_fts_search_text(&self) -> Result<(), StoreError> {
        let rows = {
            let mut statement = self.connection.prepare(
                "SELECT p.id,
                        p.media_kind,
                        p.canonical_title,
                        COALESCE((
                            SELECT group_concat(alias, char(31))
                            FROM provider_item_alias
                            WHERE provider_item_id = p.id
                        ), '') AS aliases
                 FROM provider_item p
                 ORDER BY p.id",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        split_aliases(&row.get::<_, String>(3)?),
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };

        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            self.connection
                .execute("DELETE FROM provider_item_fts", [])?;
            for (provider_item_id, media_kind, canonical_title, aliases) in rows {
                self.insert_provider_item_fts(
                    provider_item_id,
                    &media_kind,
                    &canonical_title,
                    &aliases,
                )?;
            }
            self.connection.execute(
                "INSERT INTO store_metadata(key, value, updated_at)
                 VALUES (?1, ?2, CURRENT_TIMESTAMP)
                 ON CONFLICT(key) DO UPDATE SET
                    value = excluded.value,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    PROVIDER_ITEM_FTS_SEARCH_TEXT_VERSION_KEY,
                    PROVIDER_ITEM_FTS_SEARCH_TEXT_VERSION,
                ],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool, StoreError> {
        let sql = format!("PRAGMA table_info({table})");
        let mut statement = self.connection.prepare(&sql)?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(columns.iter().any(|name| name == column))
    }

    fn insert_provider_item_fts(
        &self,
        provider_item_id: i64,
        media_kind: &str,
        canonical_title: &str,
        aliases: &[String],
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO provider_item_fts(
                provider_item_id,
                media_kind,
                canonical_title,
                aliases
             )
             VALUES (?1, ?2, ?3, ?4)",
            params![
                provider_item_id,
                media_kind,
                title_fts_text(canonical_title),
                title_fts_text_for_values(aliases),
            ],
        )?;
        Ok(())
    }

    fn collection_entry_id(
        &self,
        account_id: &str,
        provider: Provider,
        media_kind: MediaKind,
        provider_entry_id: &str,
    ) -> Result<i64, StoreError> {
        let id = self.connection.query_row(
            "SELECT id
             FROM collection_entry
             WHERE account_id = ?1
               AND provider = ?2
               AND media_kind = ?3
               AND provider_entry_id = ?4",
            params![
                account_id,
                provider.as_str(),
                media_kind.as_str(),
                provider_entry_id,
            ],
            |row| row.get::<_, i64>(0),
        )?;

        Ok(id)
    }

    fn refresh_field_provenance(
        &self,
        collection_entry_id: i64,
        provider: Provider,
        state: CanonicalFieldState,
    ) -> Result<(), StoreError> {
        for field in default_writable_fields() {
            let observed_value_hash = state
                .hash(field)
                .expect("default-writable fields have canonical hashes");
            self.upsert_field_provenance(
                collection_entry_id,
                provider,
                field,
                &observed_value_hash,
            )?;
        }

        Ok(())
    }

    fn upsert_field_provenance(
        &self,
        collection_entry_id: i64,
        provider: Provider,
        field: SyncField,
        observed_value_hash: &str,
    ) -> Result<(), StoreError> {
        let previous = self
            .connection
            .query_row(
                "SELECT observed_value_hash,
                        observation_version,
                        last_changed_observation_version,
                        attributed_write_journal_field_id,
                        externally_cleared,
                        pending_external_change
                 FROM field_provenance
                 WHERE collection_entry_id = ?1
                   AND field_name = ?2
                   AND provider = ?3",
                params![collection_entry_id, field.as_str(), provider.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, bool>(4)?,
                        row.get::<_, bool>(5)?,
                    ))
                },
            )
            .optional()?;

        let (
            previous_hash,
            previous_observation_version,
            previous_last_changed_observation_version,
            previous_attribution,
            previous_externally_cleared,
            previous_pending_external_change,
            observation_version,
            last_changed_observation_version,
            default_origin,
        ) = match previous {
            Some((
                previous_hash,
                previous_version,
                previous_last_changed_version,
                previous_attribution,
                previous_externally_cleared,
                previous_pending_external_change,
            )) => {
                let origin = if previous_hash == observed_value_hash {
                    FieldObservationChangeOrigin::Unchanged
                } else {
                    FieldObservationChangeOrigin::External
                };
                let observation_version = previous_version + 1;
                let last_changed_observation_version =
                    if origin == FieldObservationChangeOrigin::Unchanged {
                        previous_last_changed_version
                    } else {
                        observation_version
                    };
                (
                    Some(previous_hash),
                    Some(previous_version),
                    previous_last_changed_version,
                    previous_attribution,
                    previous_externally_cleared,
                    previous_pending_external_change,
                    observation_version,
                    last_changed_observation_version,
                    origin,
                )
            }
            None => (
                None,
                None,
                1,
                None,
                false,
                false,
                1,
                1,
                FieldObservationChangeOrigin::Initial,
            ),
        };

        let matched_attribution = if default_origin == FieldObservationChangeOrigin::Unchanged {
            None
        } else {
            self.matching_succeeded_write_journal_field(
                collection_entry_id,
                field,
                previous_hash.as_deref(),
                observed_value_hash,
                previous_observation_version,
                previous_last_changed_observation_version,
            )?
        };
        let attribution = if default_origin == FieldObservationChangeOrigin::Unchanged {
            previous_attribution
        } else {
            matched_attribution
        };
        let change_origin = if matched_attribution.is_some() {
            FieldObservationChangeOrigin::ToolWrite
        } else {
            default_origin
        };
        let externally_cleared = if default_origin == FieldObservationChangeOrigin::Unchanged {
            previous_externally_cleared
        } else {
            matched_attribution.is_none()
                && previous_hash.is_some()
                && observed_value_hash == stable_hash(UNSET_FIELD_VALUE)
        };
        let pending_external_change = if default_origin == FieldObservationChangeOrigin::Unchanged {
            previous_pending_external_change
        } else {
            matched_attribution.is_none() && previous_hash.is_some()
        };

        self.connection.execute(
            "INSERT INTO field_provenance(
                collection_entry_id,
                field_name,
                provider,
                observed_value_hash,
                previous_observed_value_hash,
                observation_version,
                last_changed_observation_version,
                change_origin,
                attributed_write_journal_field_id,
                externally_cleared,
                pending_external_change,
                source_reliability
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(collection_entry_id, field_name, provider) DO UPDATE SET
                previous_observed_value_hash = excluded.previous_observed_value_hash,
                observed_value_hash = excluded.observed_value_hash,
                observation_version = excluded.observation_version,
                last_changed_observation_version = excluded.last_changed_observation_version,
                change_origin = excluded.change_origin,
                attributed_write_journal_field_id = excluded.attributed_write_journal_field_id,
                externally_cleared = excluded.externally_cleared,
                pending_external_change = excluded.pending_external_change,
                observed_at = CURRENT_TIMESTAMP,
                source_reliability = excluded.source_reliability",
            params![
                collection_entry_id,
                field.as_str(),
                provider.as_str(),
                observed_value_hash,
                previous_hash,
                observation_version,
                last_changed_observation_version,
                change_origin.as_str(),
                attribution,
                externally_cleared,
                pending_external_change,
                "provider_snapshot",
            ],
        )?;

        if let Some(write_journal_field_id) = matched_attribution {
            self.connection.execute(
                "UPDATE write_journal_field
                 SET attributed_collection_entry_id = ?2,
                     attributed_observation_version = ?3,
                     consumed_at = COALESCE(consumed_at, CURRENT_TIMESTAMP)
                 WHERE id = ?1",
                params![
                    write_journal_field_id,
                    collection_entry_id,
                    observation_version,
                ],
            )?;
        }

        Ok(())
    }

    fn matching_succeeded_write_journal_field(
        &self,
        collection_entry_id: i64,
        field: SyncField,
        previous_observed_value_hash: Option<&str>,
        observed_value_hash: &str,
        previous_observation_version: Option<i64>,
        previous_last_changed_observation_version: i64,
    ) -> Result<Option<i64>, StoreError> {
        let expected_before_hash = previous_observed_value_hash.unwrap_or({
            // A missing provenance row is only attributable to an AddEntry
            // journal field, whose explicit before state is a missing entry.
            MISSING_ENTRY_VALUE
        });
        let expected_before_hash = if previous_observed_value_hash.is_some() {
            expected_before_hash.to_owned()
        } else {
            stable_hash(expected_before_hash)
        };

        let write_journal_field_id = self
            .connection
            .query_row(
                "SELECT wjf.id
                 FROM write_journal_field wjf
                 JOIN write_journal wj ON wj.id = wjf.write_journal_id
                 JOIN collection_entry ce ON ce.id = ?1
                 LEFT JOIN collection_snapshot_state css
                   ON css.account_id = ce.account_id
                  AND css.provider = ce.provider
                  AND css.media_kind = ce.media_kind
                 WHERE wj.provider = ce.provider
                   AND wj.account_id = ce.account_id
                   AND wj.result_status = 'succeeded'
                   AND wjf.work_id = ce.work_id
                   AND wjf.media_kind = ce.media_kind
                   AND wjf.target_provider_entry_id = ce.provider_entry_id
                   AND wjf.field_name = ?2
                   AND wjf.expected_value_hash = ?3
                   AND wjf.before_value_hash = ?4
                   AND (
                        (
                            wjf.basis_observation_version IS NULL
                            AND ?5 IS NULL
                            AND wjf.basis_snapshot_generation = COALESCE(css.generation, 0)
                        )
                        OR (
                            wjf.basis_observation_version <= ?5
                            AND ?6 <= wjf.basis_observation_version
                        )
                   )
                   AND wjf.consumed_at IS NULL
                 ORDER BY wj.id DESC
                 LIMIT 1",
                params![
                    collection_entry_id,
                    field.as_str(),
                    observed_value_hash,
                    expected_before_hash,
                    previous_observation_version,
                    previous_last_changed_observation_version,
                ],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        Ok(write_journal_field_id)
    }
}

fn default_writable_fields() -> [SyncField; 5] {
    [
        SyncField::Status,
        SyncField::Score,
        SyncField::ProgressEpisodes,
        SyncField::ProgressChapters,
        SyncField::ProgressVolumes,
    ]
}

fn split_aliases(value: &str) -> Vec<String> {
    value
        .split('\u{1f}')
        .filter(|alias| !alias.is_empty())
        .map(str::to_owned)
        .collect()
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite {
            message: error.to_string(),
        }
    }
}

fn external_id_edge_evidence_strength(source: &str, match_method: &str) -> u8 {
    let source = source.to_ascii_lowercase();
    let match_method = match_method.to_ascii_lowercase();
    if match_method == "manual" || source.contains("manual") {
        3
    } else if match_method.contains("id-mal")
        || match_method.contains("crosswalk")
        || source.contains("idmal")
        || source.contains("crosswalk")
    {
        2
    } else if source == "auto-match" || match_method.starts_with("fts-") {
        1
    } else {
        0
    }
}

fn provider_from_str(value: &str) -> Provider {
    match value {
        "bangumi" => Provider::Bangumi,
        "anilist" => Provider::AniList,
        "myanimelist" => Provider::MyAnimeList,
        other => panic!("unknown provider stored in sqlite: {other}"),
    }
}

fn media_kind_from_str(value: &str) -> MediaKind {
    match value {
        "anime" => MediaKind::Anime,
        "manga" => MediaKind::Manga,
        other => panic!("unknown media kind stored in sqlite: {other}"),
    }
}

fn collection_status_to_str(value: CollectionStatus) -> &'static str {
    match value {
        CollectionStatus::InProgress => "in_progress",
        CollectionStatus::Completed => "completed",
        CollectionStatus::Paused => "paused",
        CollectionStatus::Dropped => "dropped",
        CollectionStatus::Planned => "planned",
    }
}

fn collection_status_from_str(value: &str) -> CollectionStatus {
    match value {
        "in_progress" => CollectionStatus::InProgress,
        "completed" => CollectionStatus::Completed,
        "paused" => CollectionStatus::Paused,
        "dropped" => CollectionStatus::Dropped,
        "planned" => CollectionStatus::Planned,
        other => panic!("unknown collection status stored in sqlite: {other}"),
    }
}

fn sync_field_from_str(value: &str) -> SyncField {
    match value {
        "status" => SyncField::Status,
        "score" | "score_hundred" => SyncField::Score,
        "progress_episodes" => SyncField::ProgressEpisodes,
        "progress_chapters" => SyncField::ProgressChapters,
        "progress_volumes" => SyncField::ProgressVolumes,
        "repeat_count" => SyncField::RepeatCount,
        "started_at" => SyncField::StartedAt,
        "completed_at" => SyncField::CompletedAt,
        "notes" => SyncField::Notes,
        "tags" => SyncField::Tags,
        other => panic!("unknown sync field stored in sqlite: {other}"),
    }
}

fn field_observation_change_origin_from_str(value: &str) -> FieldObservationChangeOrigin {
    match value {
        "initial" => FieldObservationChangeOrigin::Initial,
        "unchanged" => FieldObservationChangeOrigin::Unchanged,
        "external" => FieldObservationChangeOrigin::External,
        "tool_write" => FieldObservationChangeOrigin::ToolWrite,
        other => panic!("unknown field observation change origin stored in sqlite: {other}"),
    }
}

fn manual_mapping_decision_from_str(value: &str) -> ManualMappingDecision {
    match value {
        "link" => ManualMappingDecision::Link,
        "ignore" => ManualMappingDecision::Ignore,
        other => panic!("unknown manual mapping decision stored in sqlite: {other}"),
    }
}

fn write_journal_status_from_str(value: &str) -> WriteJournalStatus {
    match value {
        "planned" => WriteJournalStatus::Planned,
        "attempted" => WriteJournalStatus::Attempted,
        "succeeded" => WriteJournalStatus::Succeeded,
        "failed" => WriteJournalStatus::Failed,
        other => panic!("unknown write journal status stored in sqlite: {other}"),
    }
}

fn refresh_attempt_status_to_str(value: ProviderCredentialRefreshAttemptStatus) -> &'static str {
    match value {
        ProviderCredentialRefreshAttemptStatus::Pending => "pending",
        ProviderCredentialRefreshAttemptStatus::Committed => "committed",
        ProviderCredentialRefreshAttemptStatus::Failed => "failed",
    }
}

fn refresh_attempt_status_from_str(value: &str) -> ProviderCredentialRefreshAttemptStatus {
    match value {
        "pending" => ProviderCredentialRefreshAttemptStatus::Pending,
        "committed" => ProviderCredentialRefreshAttemptStatus::Committed,
        "failed" => ProviderCredentialRefreshAttemptStatus::Failed,
        other => panic!("unknown credential refresh attempt status stored in sqlite: {other}"),
    }
}

fn validate_provider_credential(input: &ProviderCredentialInput) -> Result<(), StoreError> {
    let capability = provider_credential_capability(input.provider);

    if input.auth_flow != capability.auth_flow {
        return Err(StoreError::InvalidProviderCredential {
            provider: input.provider,
            reason: "auth flow is not supported by provider".to_owned(),
        });
    }

    if !capability.bootstrap_modes.contains(&input.bootstrap_mode) {
        return Err(StoreError::InvalidProviderCredential {
            provider: input.provider,
            reason: "bootstrap mode is not supported by provider".to_owned(),
        });
    }

    if capability.refresh_policy == ProviderRefreshPolicy::ReauthorizeOnly
        && input.refresh_token != ProviderRefreshTokenState::absent()
    {
        return Err(StoreError::InvalidProviderCredential {
            provider: input.provider,
            reason: "refresh token state is not supported by provider".to_owned(),
        });
    }

    Ok(())
}

fn auth_flow_to_str(value: ProviderAuthFlow) -> &'static str {
    match value {
        ProviderAuthFlow::AuthorizationCode => "authorization_code",
        ProviderAuthFlow::AuthorizationCodePkcePlain => "authorization_code_pkce_plain",
    }
}

fn auth_flow_from_str(value: &str) -> ProviderAuthFlow {
    match value {
        "authorization_code" => ProviderAuthFlow::AuthorizationCode,
        "authorization_code_pkce_plain" => ProviderAuthFlow::AuthorizationCodePkcePlain,
        other => panic!("unknown auth flow stored in sqlite: {other}"),
    }
}

fn bootstrap_mode_to_str(value: ProviderAuthBootstrapMode) -> &'static str {
    match value {
        ProviderAuthBootstrapMode::AuthBroker => "auth_broker",
        ProviderAuthBootstrapMode::LocalCallback => "local_callback",
        ProviderAuthBootstrapMode::ManualPin => "manual_pin",
    }
}

fn bootstrap_mode_from_str(value: &str) -> ProviderAuthBootstrapMode {
    match value {
        "auth_broker" => ProviderAuthBootstrapMode::AuthBroker,
        "local_callback" => ProviderAuthBootstrapMode::LocalCallback,
        "manual_pin" => ProviderAuthBootstrapMode::ManualPin,
        other => panic!("unknown auth bootstrap mode stored in sqlite: {other}"),
    }
}

fn refresh_token_state_to_sql(value: ProviderRefreshTokenState) -> (&'static str, Option<i64>) {
    match value {
        ProviderRefreshTokenState::Absent => ("absent", None),
        ProviderRefreshTokenState::PresentWithUnknownExpiry => ("present_unknown", None),
        ProviderRefreshTokenState::PresentExpiresAt {
            expires_at_unix_seconds,
        } => ("present_expires_at", Some(expires_at_unix_seconds)),
    }
}

fn refresh_token_state_from_sql(
    value: &str,
    expires_at_unix_seconds: Option<i64>,
) -> ProviderRefreshTokenState {
    match value {
        "absent" => ProviderRefreshTokenState::absent(),
        "present_unknown" => ProviderRefreshTokenState::present_with_unknown_expiry(),
        "present_expires_at" => ProviderRefreshTokenState::present_expires_at(
            expires_at_unix_seconds
                .expect("sqlite constraint requires refresh token expiry for present_expires_at"),
        ),
        other => panic!("unknown refresh token state stored in sqlite: {other}"),
    }
}

fn optional_u8(value: Option<i64>) -> Option<u8> {
    value.map(|value| {
        u8::try_from(value).unwrap_or_else(|_| panic!("sqlite value out of u8 range: {value}"))
    })
}

fn optional_u32(value: Option<i64>) -> Option<u32> {
    value.map(|value| {
        u32::try_from(value).unwrap_or_else(|_| panic!("sqlite value out of u32 range: {value}"))
    })
}
