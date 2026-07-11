mod apply;
mod collection_read;
mod credential_lifecycle;
mod cycle;
mod planner;

pub use apply::{
    apply_plan_with_writer, apply_plan_with_writer_and_deferred_verifier,
    apply_plan_with_writer_and_verifier, ApplyError, ApplySummary, ProviderPostWriteEntry,
    ProviderPostWriteState, ProviderPostWriteVerifier, ProviderRequestTransport,
    ProviderRequestWriter, ProviderWriteResult, ProviderWriter,
    StoredAccessTokenProviderPostWriteVerifier, StoredAccessTokenProviderWriter,
};
pub use collection_read::{
    fetch_collection_snapshot_pages_with_stored_access_token,
    fetch_collection_snapshot_with_stored_access_token,
    import_collection_snapshot_pages_with_stored_access_token,
    import_collection_snapshot_with_identity_links, StoredProviderCollectionImportError,
    StoredProviderCollectionImportOutcome, StoredProviderCollectionReadError,
    StoredProviderCollectionReadOutcome,
};
pub use credential_lifecycle::{
    acquire_stored_provider_access_token, preflight_stored_provider_credential_secret,
    refresh_stored_provider_credential_if_needed, StoredCredentialRefreshError,
    StoredProviderAccessTokenOutcome, StoredProviderCredentialSecretPreflightOutcome,
};
pub use cycle::{
    auto_link_local_collection_entries, refresh_snapshots_and_plan_dry_run_with_auto_refresh,
    refresh_snapshots_and_plan_dry_run_with_stored_access_tokens, ProviderSnapshotImportSummary,
    ProviderSyncCredentialRefreshConfig, StoredProviderSyncCycleError,
    StoredProviderSyncCycleOutcome,
};
pub use planner::{
    plan_dry_run, plan_dry_run_for_media_kinds, FieldProviderValue, PlanConflict, PlanDiagnostic,
    PlannedAction, PlannedActionKind, PlannedFieldChange, SyncPlan,
};
