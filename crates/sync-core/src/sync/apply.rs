use std::collections::HashMap;

use serde_json::json;

use crate::model::{
    stable_hash, CanonicalFieldState, CollectionEntry, CollectionStatus, MediaKind, Provider,
    SyncField, MISSING_ENTRY_VALUE, UNSET_FIELD_VALUE,
};
use crate::provider::{
    authorize_provider_write_request, build_provider_write_request,
    AuthorizedProviderReadTransport, AuthorizedProviderWriteTransport, ProviderAuthBootstrapMode,
    ProviderCredentialAccessTokenStore, ProviderWriteRequest, ProviderWriteRequestError,
    ProviderWriteRequestMethod,
};
use crate::store::{
    SqliteStore, StoreError, WriteJournalCompletion, WriteJournalFieldIntent, WriteJournalIntent,
    WriteJournalIntentOutcome, WriteJournalStatus,
};

use super::collection_read::{
    fetch_collection_snapshot_pages_with_stored_access_token, StoredProviderCollectionReadError,
    StoredProviderCollectionReadOutcome,
};
use super::credential_lifecycle::{
    acquire_stored_provider_access_token, StoredCredentialRefreshError,
    StoredProviderAccessTokenOutcome,
};
use super::{PlannedAction, PlannedActionKind, SyncPlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    ConflictsPresent {
        count: usize,
    },
    ProtectedField {
        field: SyncField,
    },
    ProviderCredentialRefreshRequired {
        provider: Provider,
    },
    ProviderCredentialReauthorizeRequired {
        provider: Provider,
        preferred_mode: ProviderAuthBootstrapMode,
    },
    Provider {
        provider: Provider,
        message: String,
    },
    PostWriteVerification {
        provider: Provider,
        target_provider_entry_id: String,
        field: Option<SyncField>,
        expected: String,
        actual: Option<String>,
    },
    Store(StoreError),
}

impl From<StoreError> for ApplyError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWriteResult {
    pub response_body: String,
}

pub trait ProviderWriter {
    fn journal_request_body(&self, action: &PlannedAction) -> String {
        request_body_for_action(action)
    }

    fn apply_collection_action(
        &mut self,
        account_id: &str,
        action: &PlannedAction,
    ) -> Result<ProviderWriteResult, ApplyError>;
}

pub trait ProviderRequestTransport {
    fn send_provider_write_request(
        &mut self,
        request: ProviderWriteRequest,
    ) -> Result<String, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPostWriteEntry {
    pub status: CollectionStatus,
    pub score_hundred: Option<u8>,
    pub progress_episodes: Option<u32>,
    pub progress_chapters: Option<u32>,
    pub progress_volumes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderPostWriteState {
    Found(ProviderPostWriteEntry),
    Missing,
    Skipped { reason: String },
}

pub trait ProviderPostWriteVerifier {
    fn read_collection_entry_after_write(
        &mut self,
        account_id: &str,
        action: &PlannedAction,
        result: &ProviderWriteResult,
    ) -> Result<ProviderPostWriteState, ApplyError>;
}

pub struct NoopProviderPostWriteVerifier;

impl ProviderPostWriteVerifier for NoopProviderPostWriteVerifier {
    fn read_collection_entry_after_write(
        &mut self,
        _account_id: &str,
        _action: &PlannedAction,
        _result: &ProviderWriteResult,
    ) -> Result<ProviderPostWriteState, ApplyError> {
        Ok(ProviderPostWriteState::Skipped {
            reason: "post-write verification not configured".to_owned(),
        })
    }
}

pub struct ProviderRequestWriter<'a, T>
where
    T: ProviderRequestTransport,
{
    transport: &'a mut T,
}

impl<'a, T> ProviderRequestWriter<'a, T>
where
    T: ProviderRequestTransport,
{
    pub fn new(transport: &'a mut T) -> Self {
        Self { transport }
    }
}

impl<T> ProviderWriter for ProviderRequestWriter<'_, T>
where
    T: ProviderRequestTransport,
{
    fn journal_request_body(&self, action: &PlannedAction) -> String {
        match build_provider_write_request(action) {
            Ok(request) => canonical_provider_write_request_body(&request),
            Err(error) => provider_request_build_error_body(action, &error),
        }
    }

    fn apply_collection_action(
        &mut self,
        _account_id: &str,
        action: &PlannedAction,
    ) -> Result<ProviderWriteResult, ApplyError> {
        let request =
            build_provider_write_request(action).map_err(|error| ApplyError::Provider {
                provider: action.target_provider,
                message: format!("failed to build provider request: {error:?}"),
            })?;
        let response_body = self
            .transport
            .send_provider_write_request(request)
            .map_err(|message| ApplyError::Provider {
                provider: action.target_provider,
                message,
            })?;

        Ok(ProviderWriteResult { response_body })
    }
}

pub struct StoredAccessTokenProviderWriter<'a, S, T>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderWriteTransport,
{
    store: &'a SqliteStore,
    now_unix_seconds: i64,
    secret_store: &'a mut S,
    write_transport: &'a mut T,
}

impl<'a, S, T> StoredAccessTokenProviderWriter<'a, S, T>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderWriteTransport,
{
    pub fn new(
        store: &'a SqliteStore,
        now_unix_seconds: i64,
        secret_store: &'a mut S,
        write_transport: &'a mut T,
    ) -> Self {
        Self {
            store,
            now_unix_seconds,
            secret_store,
            write_transport,
        }
    }
}

pub struct StoredAccessTokenProviderPostWriteVerifier<'a, S, T>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderReadTransport,
{
    store: &'a SqliteStore,
    now_unix_seconds: i64,
    secret_store: &'a mut S,
    read_transport: &'a mut T,
    page_limit: u16,
    snapshot_cache:
        Option<HashMap<PostWriteSnapshotCacheKey, HashMap<String, ProviderPostWriteEntry>>>,
}

impl<'a, S, T> StoredAccessTokenProviderPostWriteVerifier<'a, S, T>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderReadTransport,
{
    pub fn new(
        store: &'a SqliteStore,
        now_unix_seconds: i64,
        secret_store: &'a mut S,
        read_transport: &'a mut T,
        page_limit: u16,
    ) -> Self {
        Self {
            store,
            now_unix_seconds,
            secret_store,
            read_transport,
            page_limit,
            snapshot_cache: None,
        }
    }

    pub fn new_with_snapshot_cache(
        store: &'a SqliteStore,
        now_unix_seconds: i64,
        secret_store: &'a mut S,
        read_transport: &'a mut T,
        page_limit: u16,
    ) -> Self {
        Self {
            store,
            now_unix_seconds,
            secret_store,
            read_transport,
            page_limit,
            snapshot_cache: Some(HashMap::new()),
        }
    }

    fn read_post_write_entries(
        &mut self,
        account_id: &str,
        action: &PlannedAction,
    ) -> Result<HashMap<String, ProviderPostWriteEntry>, ApplyError> {
        let outcome = fetch_collection_snapshot_pages_with_stored_access_token(
            self.store,
            action.target_provider,
            action.media_kind,
            account_id,
            self.page_limit,
            self.now_unix_seconds,
            self.secret_store,
            self.read_transport,
        )
        .map_err(|error| {
            stored_collection_read_error_to_apply_error(action.target_provider, error)
        })?;

        match outcome {
            StoredProviderCollectionReadOutcome::Snapshot(snapshot) => Ok(snapshot
                .entries()
                .iter()
                .map(|entry| {
                    (
                        entry.provider_entry_id().to_owned(),
                        provider_post_write_entry_from_collection_entry(entry),
                    )
                })
                .collect()),
            StoredProviderCollectionReadOutcome::RefreshRequired => {
                Err(ApplyError::ProviderCredentialRefreshRequired {
                    provider: action.target_provider,
                })
            }
            StoredProviderCollectionReadOutcome::Reauthorize { preferred_mode } => {
                Err(ApplyError::ProviderCredentialReauthorizeRequired {
                    provider: action.target_provider,
                    preferred_mode,
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PostWriteSnapshotCacheKey {
    account_id: String,
    provider: Provider,
    media_kind: MediaKind,
    page_limit: u16,
}

impl<S, T> ProviderPostWriteVerifier for StoredAccessTokenProviderPostWriteVerifier<'_, S, T>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderReadTransport,
{
    fn read_collection_entry_after_write(
        &mut self,
        account_id: &str,
        action: &PlannedAction,
        _result: &ProviderWriteResult,
    ) -> Result<ProviderPostWriteState, ApplyError> {
        let target_provider_entry_id = action.target_provider_entry_id.as_str();
        let post_write_entry = if self.snapshot_cache.is_some() {
            let key = PostWriteSnapshotCacheKey {
                account_id: account_id.to_owned(),
                provider: action.target_provider,
                media_kind: action.media_kind,
                page_limit: self.page_limit,
            };
            if !self
                .snapshot_cache
                .as_ref()
                .expect("cache should exist")
                .contains_key(&key)
            {
                let entries = self.read_post_write_entries(account_id, action)?;
                self.snapshot_cache
                    .as_mut()
                    .expect("cache should exist")
                    .insert(key.clone(), entries);
            }
            self.snapshot_cache
                .as_ref()
                .expect("cache should exist")
                .get(&key)
                .and_then(|entries| entries.get(target_provider_entry_id))
                .cloned()
        } else {
            self.read_post_write_entries(account_id, action)?
                .get(target_provider_entry_id)
                .cloned()
        };

        Ok(post_write_entry
            .map(ProviderPostWriteState::Found)
            .unwrap_or(ProviderPostWriteState::Missing))
    }
}

impl<S, T> ProviderWriter for StoredAccessTokenProviderWriter<'_, S, T>
where
    S: ProviderCredentialAccessTokenStore,
    T: AuthorizedProviderWriteTransport,
{
    fn journal_request_body(&self, action: &PlannedAction) -> String {
        match build_provider_write_request(action) {
            Ok(request) => canonical_provider_write_request_body(&request),
            Err(error) => provider_request_build_error_body(action, &error),
        }
    }

    fn apply_collection_action(
        &mut self,
        account_id: &str,
        action: &PlannedAction,
    ) -> Result<ProviderWriteResult, ApplyError> {
        let request =
            build_provider_write_request(action).map_err(|error| ApplyError::Provider {
                provider: action.target_provider,
                message: format!("failed to build provider request: {error:?}"),
            })?;
        let token = match acquire_stored_provider_access_token(
            self.store,
            action.target_provider,
            account_id,
            self.now_unix_seconds,
            self.secret_store,
        )
        .map_err(|error| stored_credential_error_to_apply_error(action.target_provider, error))?
        {
            StoredProviderAccessTokenOutcome::Authorized(token) => token,
            StoredProviderAccessTokenOutcome::RefreshRequired => {
                return Err(ApplyError::ProviderCredentialRefreshRequired {
                    provider: action.target_provider,
                });
            }
            StoredProviderAccessTokenOutcome::Reauthorize { preferred_mode } => {
                return Err(ApplyError::ProviderCredentialReauthorizeRequired {
                    provider: action.target_provider,
                    preferred_mode,
                });
            }
        };
        let authorized_request =
            authorize_provider_write_request(&request, &token).map_err(|error| {
                ApplyError::Provider {
                    provider: action.target_provider,
                    message: format!("failed to authorize provider write request: {error:?}"),
                }
            })?;
        let response_body = self
            .write_transport
            .send_authorized_provider_write_request(authorized_request)
            .map_err(|message| ApplyError::Provider {
                provider: action.target_provider,
                message,
            })?;

        Ok(ProviderWriteResult { response_body })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplySummary {
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub post_write_verified: usize,
    pub post_write_skipped: usize,
}

pub fn apply_plan_with_writer<W>(
    store: &SqliteStore,
    account_id: &str,
    plan: &SyncPlan,
    writer: &mut W,
) -> Result<ApplySummary, ApplyError>
where
    W: ProviderWriter,
{
    let mut verifier = NoopProviderPostWriteVerifier;
    apply_plan_with_writer_and_verifier(store, account_id, plan, writer, &mut verifier)
}

pub fn apply_plan_with_writer_and_verifier<W, V>(
    store: &SqliteStore,
    account_id: &str,
    plan: &SyncPlan,
    writer: &mut W,
    verifier: &mut V,
) -> Result<ApplySummary, ApplyError>
where
    W: ProviderWriter,
    V: ProviderPostWriteVerifier,
{
    validate_applicable_plan(plan)?;

    let mut summary = ApplySummary::default();

    for action in plan.actions() {
        summary.attempted += 1;
        let request_body = writer.journal_request_body(action);
        let request_hash = stable_hash(&request_body);
        let operation_id = operation_id_for_action(action, &request_hash);
        let journal_outcome = store.record_write_journal_intent(
            write_journal_intent_for_action(account_id, action, operation_id.clone(), request_body),
        )?;
        if journal_outcome == WriteJournalIntentOutcome::AlreadySucceeded {
            summary.succeeded += 1;
            summary.post_write_skipped += 1;
            continue;
        }

        match writer.apply_collection_action(account_id, action) {
            Ok(result) => {
                match verifier
                    .read_collection_entry_after_write(account_id, action, &result)
                    .and_then(|state| verify_post_write_state(action, state))
                {
                    Ok(ProviderPostWriteVerification::Verified { journal_body }) => {
                        store.complete_write_journal(WriteJournalCompletion {
                            provider: action.target_provider,
                            account_id: account_id.to_owned(),
                            operation_id,
                            result_status: WriteJournalStatus::Succeeded,
                            response_body: Some(journal_body.unwrap_or(result.response_body)),
                        })?;
                        summary.succeeded += 1;
                        summary.post_write_verified += 1;
                    }
                    Ok(ProviderPostWriteVerification::Skipped) => {
                        store.complete_write_journal(WriteJournalCompletion {
                            provider: action.target_provider,
                            account_id: account_id.to_owned(),
                            operation_id,
                            result_status: WriteJournalStatus::Succeeded,
                            response_body: Some(result.response_body),
                        })?;
                        summary.succeeded += 1;
                        summary.post_write_skipped += 1;
                    }
                    Err(error) => {
                        store.complete_write_journal(WriteJournalCompletion {
                            provider: action.target_provider,
                            account_id: account_id.to_owned(),
                            operation_id,
                            result_status: WriteJournalStatus::Failed,
                            response_body: Some(error_response_body(&error)),
                        })?;
                        summary.failed += 1;
                        return Err(error);
                    }
                }
            }
            Err(error) => {
                store.complete_write_journal(WriteJournalCompletion {
                    provider: action.target_provider,
                    account_id: account_id.to_owned(),
                    operation_id,
                    result_status: WriteJournalStatus::Failed,
                    response_body: Some(error_response_body(&error)),
                })?;
                summary.failed += 1;
                return Err(error);
            }
        }
    }

    Ok(summary)
}

pub fn apply_plan_with_writer_and_deferred_verifier<W, V>(
    store: &SqliteStore,
    account_id: &str,
    plan: &SyncPlan,
    writer: &mut W,
    verifier: &mut V,
) -> Result<ApplySummary, ApplyError>
where
    W: ProviderWriter,
    V: ProviderPostWriteVerifier,
{
    validate_applicable_plan(plan)?;

    let mut summary = ApplySummary::default();
    let mut pending_writes = Vec::new();

    for action in plan.actions() {
        summary.attempted += 1;
        let request_body = writer.journal_request_body(action);
        let request_hash = stable_hash(&request_body);
        let operation_id = operation_id_for_action(action, &request_hash);
        let journal_outcome = store.record_write_journal_intent(
            write_journal_intent_for_action(account_id, action, operation_id.clone(), request_body),
        )?;
        if journal_outcome == WriteJournalIntentOutcome::AlreadySucceeded {
            summary.succeeded += 1;
            summary.post_write_skipped += 1;
            continue;
        }

        match writer.apply_collection_action(account_id, action) {
            Ok(result) => pending_writes.push(PendingProviderWrite {
                action,
                operation_id,
                result,
            }),
            Err(error) => {
                store.complete_write_journal(WriteJournalCompletion {
                    provider: action.target_provider,
                    account_id: account_id.to_owned(),
                    operation_id,
                    result_status: WriteJournalStatus::Failed,
                    response_body: Some(error_response_body(&error)),
                })?;
                summary.failed += 1;
                complete_deferred_post_write_verifications(
                    store,
                    account_id,
                    pending_writes,
                    verifier,
                    &mut summary,
                )?;
                return Err(error);
            }
        }
    }

    complete_deferred_post_write_verifications(
        store,
        account_id,
        pending_writes,
        verifier,
        &mut summary,
    )?;

    Ok(summary)
}

struct PendingProviderWrite<'a> {
    action: &'a PlannedAction,
    operation_id: String,
    result: ProviderWriteResult,
}

fn validate_applicable_plan(plan: &SyncPlan) -> Result<(), ApplyError> {
    if !plan.conflicts().is_empty() {
        return Err(ApplyError::ConflictsPresent {
            count: plan.conflicts().len(),
        });
    }

    for action in plan.actions() {
        for field in &action.field_updates {
            if !field.is_default_writable() {
                return Err(ApplyError::ProtectedField { field: *field });
            }
        }
    }

    Ok(())
}

fn complete_deferred_post_write_verifications<V>(
    store: &SqliteStore,
    account_id: &str,
    pending_writes: Vec<PendingProviderWrite<'_>>,
    verifier: &mut V,
    summary: &mut ApplySummary,
) -> Result<(), ApplyError>
where
    V: ProviderPostWriteVerifier,
{
    let mut first_error = None;

    for pending in pending_writes {
        if let Some(error) = first_error.as_ref() {
            store.complete_write_journal(WriteJournalCompletion {
                provider: pending.action.target_provider,
                account_id: account_id.to_owned(),
                operation_id: pending.operation_id,
                result_status: WriteJournalStatus::Failed,
                response_body: Some(format!(
                    "skipped after deferred post-write verification failure: {}",
                    error_response_body(error)
                )),
            })?;
            summary.failed += 1;
            continue;
        }

        match verifier
            .read_collection_entry_after_write(account_id, pending.action, &pending.result)
            .and_then(|state| verify_post_write_state(pending.action, state))
        {
            Ok(ProviderPostWriteVerification::Verified { journal_body }) => {
                store.complete_write_journal(WriteJournalCompletion {
                    provider: pending.action.target_provider,
                    account_id: account_id.to_owned(),
                    operation_id: pending.operation_id,
                    result_status: WriteJournalStatus::Succeeded,
                    response_body: Some(journal_body.unwrap_or(pending.result.response_body)),
                })?;
                summary.succeeded += 1;
                summary.post_write_verified += 1;
            }
            Ok(ProviderPostWriteVerification::Skipped) => {
                store.complete_write_journal(WriteJournalCompletion {
                    provider: pending.action.target_provider,
                    account_id: account_id.to_owned(),
                    operation_id: pending.operation_id,
                    result_status: WriteJournalStatus::Succeeded,
                    response_body: Some(pending.result.response_body),
                })?;
                summary.succeeded += 1;
                summary.post_write_skipped += 1;
            }
            Err(error) => {
                store.complete_write_journal(WriteJournalCompletion {
                    provider: pending.action.target_provider,
                    account_id: account_id.to_owned(),
                    operation_id: pending.operation_id,
                    result_status: WriteJournalStatus::Failed,
                    response_body: Some(error_response_body(&error)),
                })?;
                summary.failed += 1;
                first_error = Some(error);
            }
        }
    }

    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProviderPostWriteVerification {
    Verified { journal_body: Option<String> },
    Skipped,
}

fn verify_post_write_state(
    action: &PlannedAction,
    state: ProviderPostWriteState,
) -> Result<ProviderPostWriteVerification, ApplyError> {
    let ProviderPostWriteState::Found(entry) = state else {
        return match state {
            ProviderPostWriteState::Skipped { .. } => Ok(ProviderPostWriteVerification::Skipped),
            ProviderPostWriteState::Missing => Err(ApplyError::PostWriteVerification {
                provider: action.target_provider,
                target_provider_entry_id: action.target_provider_entry_id.clone(),
                field: None,
                expected: "entry present".to_owned(),
                actual: None,
            }),
            ProviderPostWriteState::Found(_) => unreachable!("handled by let-else"),
        };
    };

    for field in &action.field_updates {
        let expected = expected_post_write_read_back_field_value(action, *field);
        let actual = post_write_entry_field_value(&entry, *field);
        if actual.as_deref() != Some(expected.as_str()) {
            return Err(ApplyError::PostWriteVerification {
                provider: action.target_provider,
                target_provider_entry_id: action.target_provider_entry_id.clone(),
                field: Some(*field),
                expected,
                actual,
            });
        }
    }

    Ok(ProviderPostWriteVerification::Verified {
        journal_body: Some(post_write_verification_body(action, &entry)),
    })
}

fn expected_post_write_read_back_field_value(action: &PlannedAction, field: SyncField) -> String {
    action_field_state(action)
        .target_value(action.target_provider, field)
        .unwrap_or_else(|| "<protected>".to_owned())
}

fn post_write_entry_field_value(
    entry: &ProviderPostWriteEntry,
    field: SyncField,
) -> Option<String> {
    CanonicalFieldState::new(
        entry.status,
        entry.score_hundred,
        entry.progress_episodes,
        entry.progress_chapters,
        entry.progress_volumes,
    )
    .value(field)
    .or_else(|| Some("<protected>".to_owned()))
}

fn action_field_state(action: &PlannedAction) -> CanonicalFieldState {
    CanonicalFieldState::new(
        action.status,
        action.score_hundred,
        action.progress_episodes,
        action.progress_chapters,
        action.progress_volumes,
    )
}

fn post_write_verification_body(action: &PlannedAction, entry: &ProviderPostWriteEntry) -> String {
    let verified_fields = action
        .field_updates
        .iter()
        .map(|field| {
            json!({
                "field": field.as_str(),
                "expected": expected_post_write_read_back_field_value(action, *field),
                "actual": post_write_entry_field_value(entry, *field),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "post_write_verification": "verified",
        "target_provider": action.target_provider.as_str(),
        "target_provider_entry_id": action.target_provider_entry_id.as_str(),
        "fields": verified_fields,
    })
    .to_string()
}

fn operation_id_for_action(action: &PlannedAction, request_hash: &str) -> String {
    let kind = match action.kind {
        PlannedActionKind::AddEntry => "add",
        PlannedActionKind::UpdateEntry => "update",
    };
    let fields = action
        .field_updates
        .iter()
        .map(|field| field.as_str())
        .collect::<Vec<_>>()
        .join("+");
    format!(
        "work-{}-{}-{}-{}-{}-{}-{}",
        action.work_id,
        kind,
        action.target_provider.as_str(),
        action.media_kind.as_str(),
        action.target_provider_entry_id,
        fields,
        request_hash
    )
}

fn write_journal_intent_for_action(
    account_id: &str,
    action: &PlannedAction,
    operation_id: String,
    request_body: String,
) -> WriteJournalIntent {
    let state = action_field_state(action);
    let fields = action
        .field_updates
        .iter()
        .map(|field| {
            let before_value = if action.kind == PlannedActionKind::AddEntry {
                MISSING_ENTRY_VALUE
            } else {
                action
                    .field_changes
                    .iter()
                    .find(|change| change.field == *field)
                    .and_then(|change| change.old_value.as_deref())
                    .unwrap_or(UNSET_FIELD_VALUE)
            };
            let source_value = state
                .value(*field)
                .expect("applicable plan fields have canonical values");
            let expected_value = state
                .target_value(action.target_provider, *field)
                .expect("applicable plan fields have canonical target values");
            WriteJournalFieldIntent {
                field: *field,
                before_value_hash: stable_hash(before_value),
                source_value_hash: stable_hash(&source_value),
                expected_value_hash: stable_hash(&expected_value),
            }
        })
        .collect();

    WriteJournalIntent {
        provider: action.target_provider,
        account_id: account_id.to_owned(),
        operation_id,
        request_body,
        work_id: action.work_id,
        source_provider: action.source_provider,
        media_kind: action.media_kind,
        target_provider_entry_id: action.target_provider_entry_id.clone(),
        fields,
    }
}

fn request_body_for_action(action: &PlannedAction) -> String {
    let kind = match action.kind {
        PlannedActionKind::AddEntry => "add",
        PlannedActionKind::UpdateEntry => "update",
    };
    let fields = action
        .field_updates
        .iter()
        .map(|field| field.as_str())
        .collect::<Vec<_>>();
    let field_changes = action
        .field_changes
        .iter()
        .map(|change| {
            json!({
                "field": change.field.as_str(),
                "old": change.old_value.as_deref().unwrap_or("<unset>"),
                "new": change.new_value.as_str(),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "kind": kind,
        "work_id": action.work_id,
        "source_provider": action.source_provider.as_str(),
        "target_provider": action.target_provider.as_str(),
        "target_provider_entry_id": action.target_provider_entry_id.as_str(),
        "media_kind": action.media_kind.as_str(),
        "fields": fields,
        "field_changes": field_changes,
        "status": format!("{:?}", action.status),
        "score_hundred": action.score_hundred,
        "progress_episodes": action.progress_episodes,
        "progress_chapters": action.progress_chapters,
        "progress_volumes": action.progress_volumes,
        "reason": action.reason.as_str(),
    })
    .to_string()
}

fn canonical_provider_write_request_body(request: &ProviderWriteRequest) -> String {
    json!({
        "method": provider_write_request_method_as_str(request.method),
        "url": request.url.as_str(),
        "content_type": request.content_type,
        "body": request.body.as_str(),
    })
    .to_string()
}

fn provider_request_build_error_body(
    action: &PlannedAction,
    error: &ProviderWriteRequestError,
) -> String {
    json!({
        "kind": "provider_request_build_error",
        "target_provider": action.target_provider.as_str(),
        "target_provider_entry_id": action.target_provider_entry_id.as_str(),
        "media_kind": action.media_kind.as_str(),
        "error": format!("{error:?}"),
    })
    .to_string()
}

fn provider_write_request_method_as_str(method: ProviderWriteRequestMethod) -> &'static str {
    match method {
        ProviderWriteRequestMethod::Post => "POST",
        ProviderWriteRequestMethod::Patch => "PATCH",
        ProviderWriteRequestMethod::Put => "PUT",
    }
}

fn error_response_body(error: &ApplyError) -> String {
    match error {
        ApplyError::Provider { message, .. } => message.clone(),
        ApplyError::PostWriteVerification {
            provider,
            target_provider_entry_id,
            field,
            expected,
            actual,
        } => {
            let field = field.map(SyncField::as_str).unwrap_or("entry");
            format!(
                "post-write verification failed for provider={} entry={} field={} expected={} actual={}",
                provider.as_str(),
                target_provider_entry_id,
                field,
                expected,
                actual.as_deref().unwrap_or("<missing>")
            )
        }
        ApplyError::ConflictsPresent { count } => format!("conflicts present: {count}"),
        ApplyError::ProtectedField { field } => format!("protected field: {}", field.as_str()),
        ApplyError::ProviderCredentialRefreshRequired { provider } => {
            format!(
                "provider credential refresh required for {}",
                provider.as_str()
            )
        }
        ApplyError::ProviderCredentialReauthorizeRequired {
            provider,
            preferred_mode,
        } => {
            format!(
                "provider credential reauthorization required for {} via {}",
                provider.as_str(),
                provider_auth_bootstrap_mode_as_str(*preferred_mode)
            )
        }
        ApplyError::Store(error) => format!("{error:?}"),
    }
}

fn stored_credential_error_to_apply_error(
    provider: Provider,
    error: StoredCredentialRefreshError,
) -> ApplyError {
    match error {
        StoredCredentialRefreshError::Store(error) => ApplyError::Store(error),
        StoredCredentialRefreshError::Auth(error) => ApplyError::Provider {
            provider,
            message: format!("provider credential access token failed: {error:?}"),
        },
        StoredCredentialRefreshError::RefreshCommitConflict {
            provider,
            account_id,
            attempt_id,
        } => ApplyError::Provider {
            provider,
            message: format!(
                "provider credential refresh commit conflict for account_id={account_id}, attempt_id={attempt_id}"
            ),
        },
    }
}

fn stored_collection_read_error_to_apply_error(
    provider: Provider,
    error: StoredProviderCollectionReadError,
) -> ApplyError {
    match error {
        StoredProviderCollectionReadError::Credential(error) => {
            stored_credential_error_to_apply_error(provider, error)
        }
        StoredProviderCollectionReadError::Read(error) => ApplyError::Provider {
            provider,
            message: format!("post-write verification read failed: {error:?}"),
        },
    }
}

fn provider_post_write_entry_from_collection_entry(
    entry: &CollectionEntry,
) -> ProviderPostWriteEntry {
    let progress = entry.progress();
    ProviderPostWriteEntry {
        status: entry.status(),
        score_hundred: entry.score().map(|score| score.as_hundred_point()),
        progress_episodes: progress.episodes(),
        progress_chapters: progress.chapters(),
        progress_volumes: progress.volumes(),
    }
}

fn provider_auth_bootstrap_mode_as_str(mode: ProviderAuthBootstrapMode) -> &'static str {
    match mode {
        ProviderAuthBootstrapMode::AuthBroker => "auth_broker",
        ProviderAuthBootstrapMode::LocalCallback => "local_callback",
        ProviderAuthBootstrapMode::ManualPin => "manual_pin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CollectionStatus, MediaKind, SyncField};
    use crate::sync::{PlannedAction, PlannedActionKind, PlannedFieldChange, SyncPlan};

    #[test]
    fn apply_rejects_protected_fields_before_writer_call() {
        let store = SqliteStore::open_in_memory().expect("store should open");
        let plan = SyncPlan::from_parts_for_test(
            vec![PlannedAction {
                kind: PlannedActionKind::UpdateEntry,
                work_id: 1,
                source_provider: Provider::Bangumi,
                target_provider: Provider::AniList,
                target_provider_entry_id: "1".to_owned(),
                media_kind: MediaKind::Anime,
                status: CollectionStatus::Completed,
                score_hundred: None,
                progress_episodes: None,
                progress_chapters: None,
                progress_volumes: None,
                field_updates: vec![SyncField::Notes],
                field_changes: vec![PlannedFieldChange {
                    field: SyncField::Notes,
                    old_value: Some("old".to_owned()),
                    new_value: "new".to_owned(),
                }],
                reason: "test protected field".to_owned(),
            }],
            Vec::new(),
            0,
        );
        let mut writer = CountingWriter::default();

        let error = apply_plan_with_writer(&store, "fixture-account", &plan, &mut writer)
            .expect_err("protected fields should not apply");

        assert_eq!(
            error,
            ApplyError::ProtectedField {
                field: SyncField::Notes
            }
        );
        assert_eq!(writer.calls, 0);
        assert!(store
            .write_journal_entries("fixture-account", Provider::AniList)
            .expect("journal lookup should work")
            .is_empty());
    }

    #[derive(Default)]
    struct CountingWriter {
        calls: usize,
    }

    impl ProviderWriter for CountingWriter {
        fn apply_collection_action(
            &mut self,
            _account_id: &str,
            _action: &PlannedAction,
        ) -> Result<ProviderWriteResult, ApplyError> {
            self.calls += 1;
            Ok(ProviderWriteResult {
                response_body: "{}".to_owned(),
            })
        }
    }
}
