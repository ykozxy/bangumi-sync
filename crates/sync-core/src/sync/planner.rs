use std::collections::{HashMap, HashSet};

use crate::model::{
    CanonicalFieldState, CollectionStatus, MediaKind, Provider, SyncField, UNSET_FIELD_VALUE,
};
use crate::store::{
    FieldObservationChangeOrigin, PlanningCollectionEntry, SqliteStore, StoreError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    dry_run: bool,
    actions: Vec<PlannedAction>,
    conflicts: Vec<PlanConflict>,
    diagnostics: Vec<PlanDiagnostic>,
    skipped_unmatched_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedActionKind {
    AddEntry,
    UpdateEntry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedAction {
    pub kind: PlannedActionKind,
    pub work_id: i64,
    pub source_provider: Provider,
    pub target_provider: Provider,
    pub target_provider_entry_id: String,
    pub media_kind: MediaKind,
    pub status: CollectionStatus,
    pub score_hundred: Option<u8>,
    pub progress_episodes: Option<u32>,
    pub progress_chapters: Option<u32>,
    pub progress_volumes: Option<u32>,
    pub field_updates: Vec<SyncField>,
    pub field_changes: Vec<PlannedFieldChange>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFieldChange {
    pub field: SyncField,
    pub old_value: Option<String>,
    pub new_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanConflict {
    pub work_id: i64,
    pub field: SyncField,
    pub values: Vec<FieldProviderValue>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDiagnostic {
    pub work_id: i64,
    pub source_provider: Provider,
    pub target_provider: Provider,
    pub target_provider_entry_id: String,
    pub media_kind: MediaKind,
    pub field: SyncField,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProviderValue {
    pub provider: Provider,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldResolutionEvidence {
    LocalHistory,
    WriteJournal,
    TimestampBootstrap,
}

impl FieldResolutionEvidence {
    const fn as_str(self) -> &'static str {
        match self {
            Self::LocalHistory => "local-history",
            Self::WriteJournal => "write-journal",
            Self::TimestampBootstrap => "timestamp-bootstrap",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldResolution {
    field: SyncField,
    source_provider: Provider,
    evidence: FieldResolutionEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoricalFieldResolution {
    Resolved(FieldResolution),
    Conflict,
    BootstrapUnavailable,
}

impl SyncPlan {
    fn new(
        actions: Vec<PlannedAction>,
        conflicts: Vec<PlanConflict>,
        diagnostics: Vec<PlanDiagnostic>,
        skipped_unmatched_count: usize,
    ) -> Self {
        Self {
            dry_run: true,
            actions,
            conflicts,
            diagnostics,
            skipped_unmatched_count,
        }
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn actions(&self) -> &[PlannedAction] {
        &self.actions
    }

    pub fn conflicts(&self) -> &[PlanConflict] {
        &self.conflicts
    }

    pub fn diagnostics(&self) -> &[PlanDiagnostic] {
        &self.diagnostics
    }

    pub fn skipped_unmatched_count(&self) -> usize {
        self.skipped_unmatched_count
    }

    #[cfg(test)]
    pub(crate) fn from_parts_for_test(
        actions: Vec<PlannedAction>,
        conflicts: Vec<PlanConflict>,
        skipped_unmatched_count: usize,
    ) -> Self {
        Self::new(actions, conflicts, Vec::new(), skipped_unmatched_count)
    }
}

pub fn plan_dry_run(
    store: &SqliteStore,
    account_id: &str,
    media_kind: MediaKind,
    providers: &[Provider],
) -> Result<SyncPlan, StoreError> {
    let selected_providers = providers.iter().copied().collect::<HashSet<_>>();
    let entries = store
        .collection_entries_for_planning(account_id, media_kind)?
        .into_iter()
        .filter(|entry| selected_providers.contains(&entry.provider))
        .collect::<Vec<_>>();
    let skipped_unmatched_count = entries
        .iter()
        .filter(|entry| entry.work_id.is_none())
        .count();
    let mut grouped: HashMap<i64, Vec<PlanningCollectionEntry>> = HashMap::new();

    for entry in entries {
        if let Some(work_id) = entry.work_id {
            grouped.entry(work_id).or_default().push(entry);
        }
    }

    let mut work_ids = grouped.keys().copied().collect::<Vec<_>>();
    work_ids.sort_unstable();

    let mut actions = Vec::new();
    let mut conflicts = Vec::new();
    let mut diagnostics = Vec::new();

    for work_id in work_ids {
        let entries = grouped
            .get(&work_id)
            .expect("work id collected from grouped entries");
        let use_field_level_planning =
            has_exactly_one_entry_per_selected_provider(entries, providers)
                && (providers.len() >= 3 || has_historical_field_evidence(entries));
        if use_field_level_planning {
            let field_resolutions = field_level_source_providers(entries);
            let resolved_fields = field_resolutions
                .iter()
                .map(|resolution| resolution.field)
                .collect::<HashSet<_>>();
            let work_conflicts = conflicts_for_work(work_id, entries)
                .into_iter()
                .filter(|conflict| !resolved_fields.contains(&conflict.field))
                .collect::<Vec<_>>();
            let has_unresolved_conflicts = !work_conflicts.is_empty();
            if has_unresolved_conflicts {
                conflicts.extend(work_conflicts);
            }

            if !field_resolutions.is_empty() {
                let (field_level_actions, field_level_diagnostics) =
                    field_level_update_actions(work_id, entries, providers, &field_resolutions);
                actions.extend(field_level_actions);
                diagnostics.extend(field_level_diagnostics);
                continue;
            }

            if has_unresolved_conflicts {
                continue;
            }
        }

        let work_conflicts = conflicts_for_work(work_id, entries);
        let authoritative = if work_conflicts.is_empty() {
            None
        } else {
            newest_reliable_entry(entries)
        };

        if !work_conflicts.is_empty() && authoritative.is_none() {
            conflicts.extend(work_conflicts);
            continue;
        }

        let Some(source) = choose_source_entry(entries, providers) else {
            continue;
        };
        let action_source = authoritative.unwrap_or(source);
        let present_providers = entries
            .iter()
            .map(|entry| entry.provider)
            .collect::<HashSet<_>>();
        let external_ids = store.external_ids_for_work(work_id, media_kind)?;

        for target_provider in providers {
            let Some(target) = entries
                .iter()
                .find(|entry| entry.provider == *target_provider)
            else {
                continue;
            };
            let field_updates = if let Some(authoritative) = authoritative {
                fields_different_from_authoritative(target, authoritative)
            } else {
                missing_writable_fields(target, entries)
            };
            diagnostics.extend(unsupported_write_diagnostics_for_existing_target(
                work_id,
                entries,
                providers,
                target,
                &field_updates,
                authoritative,
            ));
            let field_updates = target_writable_fields(*target_provider, media_kind, field_updates);
            if field_updates.is_empty() {
                continue;
            }
            let source_provider = if let Some(authoritative) = authoritative {
                authoritative.provider
            } else {
                let Some(source_provider) = source_provider_for_fields(
                    entries,
                    providers,
                    *target_provider,
                    &field_updates,
                ) else {
                    continue;
                };
                source_provider
            };
            let reason = if authoritative.is_some() {
                "newer reliable provider timestamp; update target default-writable fields"
            } else {
                "missing target field; fill agreed default-writable value"
            };
            let field_changes = planned_field_changes(Some(target), action_source, &field_updates);

            actions.push(PlannedAction {
                kind: PlannedActionKind::UpdateEntry,
                work_id,
                source_provider,
                target_provider: *target_provider,
                target_provider_entry_id: target.provider_entry_id.clone(),
                media_kind,
                status: action_source.status,
                score_hundred: action_source.score_hundred,
                progress_episodes: action_source.progress_episodes,
                progress_chapters: action_source.progress_chapters,
                progress_volumes: action_source.progress_volumes,
                field_updates,
                field_changes,
                reason: reason.to_owned(),
            });
        }

        for target_provider in providers {
            if present_providers.contains(target_provider) {
                continue;
            }

            let Some(target_external_id) = external_ids
                .iter()
                .find(|edge| edge.provider == *target_provider)
                .map(|edge| edge.external_id.as_str())
            else {
                continue;
            };
            let field_updates = if authoritative.is_some() {
                writable_fields_present_for_entry(action_source)
            } else {
                writable_fields_present(entries)
            };
            diagnostics.extend(unsupported_write_diagnostics_for_missing_target(
                work_id,
                action_source.provider,
                *target_provider,
                target_external_id,
                media_kind,
                &field_updates,
            ));
            let field_updates = target_writable_fields(*target_provider, media_kind, field_updates);
            if field_updates.is_empty() {
                continue;
            }
            let field_changes = planned_field_changes(None, action_source, &field_updates);

            actions.push(PlannedAction {
                kind: PlannedActionKind::AddEntry,
                work_id,
                source_provider: action_source.provider,
                target_provider: *target_provider,
                target_provider_entry_id: target_external_id.to_owned(),
                media_kind,
                status: action_source.status,
                score_hundred: action_source.score_hundred,
                progress_episodes: action_source.progress_episodes,
                progress_chapters: action_source.progress_chapters,
                progress_volumes: action_source.progress_volumes,
                field_updates,
                field_changes,
                reason: format!(
                    "missing target provider entry; copy agreed default-writable fields from {}",
                    action_source.provider.as_str()
                ),
            });
        }
    }

    Ok(SyncPlan::new(
        actions,
        conflicts,
        diagnostics,
        skipped_unmatched_count,
    ))
}

pub fn plan_dry_run_for_media_kinds(
    store: &SqliteStore,
    account_id: &str,
    media_kinds: &[MediaKind],
    providers: &[Provider],
) -> Result<SyncPlan, StoreError> {
    let mut actions = Vec::new();
    let mut conflicts = Vec::new();
    let mut diagnostics = Vec::new();
    let mut skipped_unmatched_count = 0;

    for media_kind in media_kinds {
        let plan = plan_dry_run(store, account_id, *media_kind, providers)?;
        actions.extend(plan.actions().iter().cloned());
        conflicts.extend(plan.conflicts().iter().cloned());
        diagnostics.extend(plan.diagnostics().iter().cloned());
        skipped_unmatched_count += plan.skipped_unmatched_count();
    }

    Ok(SyncPlan::new(
        actions,
        conflicts,
        diagnostics,
        skipped_unmatched_count,
    ))
}

fn choose_source_entry<'a>(
    entries: &'a [PlanningCollectionEntry],
    providers: &[Provider],
) -> Option<&'a PlanningCollectionEntry> {
    for provider in providers {
        if let Some(entry) = entries.iter().find(|entry| entry.provider == *provider) {
            return Some(entry);
        }
    }

    entries.first()
}

fn source_provider_for_fields(
    entries: &[PlanningCollectionEntry],
    providers: &[Provider],
    target_provider: Provider,
    fields: &[SyncField],
) -> Option<Provider> {
    for provider in providers {
        if *provider == target_provider {
            continue;
        }
        if let Some(entry) = entries.iter().find(|entry| entry.provider == *provider) {
            if fields.iter().any(|field| has_field_value(entry, *field)) {
                return Some(*provider);
            }
        }
    }

    entries
        .iter()
        .find(|entry| {
            entry.provider != target_provider
                && fields.iter().any(|field| has_field_value(entry, *field))
        })
        .map(|entry| entry.provider)
}

fn conflicts_for_work(work_id: i64, entries: &[PlanningCollectionEntry]) -> Vec<PlanConflict> {
    [
        SyncField::Status,
        SyncField::Score,
        SyncField::ProgressEpisodes,
        SyncField::ProgressChapters,
        SyncField::ProgressVolumes,
    ]
    .into_iter()
    .filter_map(|field| conflict_for_field(work_id, entries, field))
    .collect()
}

fn has_exactly_one_entry_per_selected_provider(
    entries: &[PlanningCollectionEntry],
    providers: &[Provider],
) -> bool {
    if entries.len() != providers.len() || providers.len() < 2 {
        return false;
    }

    providers.iter().all(|provider| {
        entries
            .iter()
            .filter(|entry| entry.provider == *provider)
            .count()
            == 1
    })
}

fn has_historical_field_evidence(entries: &[PlanningCollectionEntry]) -> bool {
    [
        SyncField::Status,
        SyncField::Score,
        SyncField::ProgressEpisodes,
        SyncField::ProgressChapters,
        SyncField::ProgressVolumes,
    ]
    .into_iter()
    .any(|field| {
        let observations = entries
            .iter()
            .map(|entry| entry.field_observations.get(&field))
            .collect::<Option<Vec<_>>>();
        let Some(observations) = observations else {
            return false;
        };
        observations.iter().all(|observation| {
            observation.previous_observed_value_hash.is_some()
                || observation.change_origin == FieldObservationChangeOrigin::ToolWrite
        }) || observations.iter().any(|observation| {
            matches!(
                observation.change_origin,
                FieldObservationChangeOrigin::External | FieldObservationChangeOrigin::ToolWrite
            ) || observation.attributed_write_journal_field_id.is_some()
                || observation.pending_external_change
        })
    })
}

fn field_level_source_providers(entries: &[PlanningCollectionEntry]) -> Vec<FieldResolution> {
    [
        SyncField::Status,
        SyncField::Score,
        SyncField::ProgressEpisodes,
        SyncField::ProgressChapters,
        SyncField::ProgressVolumes,
    ]
    .into_iter()
    .filter_map(|field| match historical_field_resolution(entries, field) {
        HistoricalFieldResolution::Resolved(resolution) => Some(resolution),
        HistoricalFieldResolution::Conflict => None,
        HistoricalFieldResolution::BootstrapUnavailable => {
            unique_reliable_outlier_source(entries, field).map(|source| FieldResolution {
                field,
                source_provider: source.provider,
                evidence: FieldResolutionEvidence::TimestampBootstrap,
            })
        }
    })
    .collect()
}

fn historical_field_resolution(
    entries: &[PlanningCollectionEntry],
    field: SyncField,
) -> HistoricalFieldResolution {
    let observations = entries
        .iter()
        .map(|entry| {
            entry
                .field_observations
                .get(&field)
                .map(|observation| (entry, observation))
        })
        .collect::<Option<Vec<_>>>();
    let Some(observations) = observations else {
        return HistoricalFieldResolution::BootstrapUnavailable;
    };

    let distinct_values = observations
        .iter()
        .map(|(_, observation)| observation.observed_value_hash.as_str())
        .collect::<HashSet<_>>();
    if distinct_values.len() <= 1 {
        return HistoricalFieldResolution::Conflict;
    }

    let external_changes = observations
        .iter()
        .filter(|(_, observation)| observation.pending_external_change)
        .collect::<Vec<_>>();
    if external_changes.len() > 1 {
        return HistoricalFieldResolution::Conflict;
    }
    if let Some((source, _)) = external_changes.first() {
        if has_field_value(source, field) {
            return HistoricalFieldResolution::Resolved(FieldResolution {
                field,
                source_provider: source.provider,
                evidence: FieldResolutionEvidence::LocalHistory,
            });
        }
        return HistoricalFieldResolution::Conflict;
    }

    let tool_writes = observations
        .iter()
        .filter(|(_, observation)| observation.attributed_write_journal_field_id.is_some())
        .collect::<Vec<_>>();
    if !tool_writes.is_empty() {
        if tool_writes.iter().any(|(_, observation)| {
            observation.attributed_source_provider.is_none()
                || observation.attributed_source_value_hash.is_none()
        }) {
            return HistoricalFieldResolution::Conflict;
        }
        let source_providers = tool_writes
            .iter()
            .filter_map(|(_, observation)| observation.attributed_source_provider)
            .collect::<HashSet<_>>();
        if source_providers.len() != 1 {
            return HistoricalFieldResolution::Conflict;
        }
        let source_provider = *source_providers
            .iter()
            .next()
            .expect("single journal source provider");
        let Some(source) = entries
            .iter()
            .find(|entry| entry.provider == source_provider)
        else {
            return HistoricalFieldResolution::Conflict;
        };
        let Some(source_hash) = canonical_state(source).hash(field) else {
            return HistoricalFieldResolution::Conflict;
        };
        if !has_field_value(source, field)
            || tool_writes.iter().any(|(_, observation)| {
                observation.attributed_source_value_hash.as_deref() != Some(source_hash.as_str())
            })
        {
            return HistoricalFieldResolution::Conflict;
        }
        return HistoricalFieldResolution::Resolved(FieldResolution {
            field,
            source_provider,
            evidence: FieldResolutionEvidence::WriteJournal,
        });
    }

    if observations
        .iter()
        .all(|(_, observation)| observation.previous_observed_value_hash.is_some())
    {
        HistoricalFieldResolution::Conflict
    } else {
        HistoricalFieldResolution::BootstrapUnavailable
    }
}

fn unique_reliable_outlier_source(
    entries: &[PlanningCollectionEntry],
    field: SyncField,
) -> Option<&PlanningCollectionEntry> {
    let mut value_counts: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        let Some(value) = field_value_opt(entry, field) else {
            continue;
        };
        *value_counts.entry(value).or_insert(0) += 1;
    }

    if value_counts.len() != 2 || !value_counts.values().any(|count| *count >= 2) {
        return None;
    }

    let mut candidates = entries.iter().filter(|entry| {
        is_high_reliability_timestamp_provider(entry.provider)
            && field_value_opt(entry, field).and_then(|value| value_counts.get(&value).copied())
                == Some(1)
    });
    let candidate = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    let candidate_timestamp = candidate.provider_updated_at_epoch_secs?;
    let older_than_any_timestamped_peer = entries
        .iter()
        .filter(|entry| entry.provider != candidate.provider)
        .filter_map(|entry| entry.provider_updated_at_epoch_secs)
        .any(|timestamp| candidate_timestamp < timestamp);
    if older_than_any_timestamped_peer {
        return None;
    }

    Some(candidate)
}

fn field_level_update_actions(
    work_id: i64,
    entries: &[PlanningCollectionEntry],
    providers: &[Provider],
    field_resolutions: &[FieldResolution],
) -> (Vec<PlannedAction>, Vec<PlanDiagnostic>) {
    let mut actions = Vec::new();
    let mut diagnostics = Vec::new();

    for target_provider in providers {
        let Some(target) = entries
            .iter()
            .find(|entry| entry.provider == *target_provider)
        else {
            continue;
        };

        for source_provider in providers {
            if source_provider == target_provider {
                continue;
            }
            let Some(source) = entries
                .iter()
                .find(|entry| entry.provider == *source_provider)
            else {
                continue;
            };
            let field_updates = field_resolutions
                .iter()
                .filter(|resolution| resolution.source_provider == *source_provider)
                .map(|resolution| resolution.field)
                .filter(|field| !target_matches_source(target, source, *field))
                .collect::<Vec<_>>();
            diagnostics.extend(field_updates.iter().filter_map(|field| {
                unsupported_write_diagnostic(
                    work_id,
                    source.provider,
                    *target_provider,
                    target.provider_entry_id.clone(),
                    target.media_kind,
                    *field,
                )
            }));
            let field_updates =
                target_writable_fields(*target_provider, target.media_kind, field_updates);

            if field_updates.is_empty() {
                continue;
            }

            let evidence = field_resolutions
                .iter()
                .filter(|resolution| {
                    resolution.source_provider == *source_provider
                        && field_updates.contains(&resolution.field)
                })
                .map(|resolution| resolution.evidence.as_str())
                .collect::<HashSet<_>>();
            let mut evidence = evidence.into_iter().collect::<Vec<_>>();
            evidence.sort_unstable();

            actions.push(PlannedAction {
                kind: PlannedActionKind::UpdateEntry,
                work_id,
                source_provider: source.provider,
                target_provider: *target_provider,
                target_provider_entry_id: target.provider_entry_id.clone(),
                media_kind: target.media_kind,
                status: source.status,
                score_hundred: source.score_hundred,
                progress_episodes: source.progress_episodes,
                progress_chapters: source.progress_chapters,
                progress_volumes: source.progress_volumes,
                field_changes: planned_field_changes(Some(target), source, &field_updates),
                field_updates,
                reason: format!(
                    "field-level {} resolution; update target default-writable fields",
                    evidence.join("+")
                ),
            });
        }
    }

    (actions, diagnostics)
}

fn unsupported_write_diagnostics_for_existing_target(
    work_id: i64,
    entries: &[PlanningCollectionEntry],
    providers: &[Provider],
    target: &PlanningCollectionEntry,
    fields: &[SyncField],
    authoritative: Option<&PlanningCollectionEntry>,
) -> Vec<PlanDiagnostic> {
    fields
        .iter()
        .filter_map(|field| {
            let source_provider = authoritative.map(|entry| entry.provider).or_else(|| {
                source_provider_for_fields(entries, providers, target.provider, &[*field])
            })?;
            unsupported_write_diagnostic(
                work_id,
                source_provider,
                target.provider,
                target.provider_entry_id.clone(),
                target.media_kind,
                *field,
            )
        })
        .collect()
}

fn unsupported_write_diagnostics_for_missing_target(
    work_id: i64,
    source_provider: Provider,
    target_provider: Provider,
    target_provider_entry_id: &str,
    media_kind: MediaKind,
    fields: &[SyncField],
) -> Vec<PlanDiagnostic> {
    fields
        .iter()
        .filter_map(|field| {
            unsupported_write_diagnostic(
                work_id,
                source_provider,
                target_provider,
                target_provider_entry_id.to_owned(),
                media_kind,
                *field,
            )
        })
        .collect()
}

fn unsupported_write_diagnostic(
    work_id: i64,
    source_provider: Provider,
    target_provider: Provider,
    target_provider_entry_id: String,
    media_kind: MediaKind,
    field: SyncField,
) -> Option<PlanDiagnostic> {
    if source_provider == Provider::Bangumi {
        return None;
    }
    if target_supports_write_field(target_provider, media_kind, field) {
        return None;
    }
    if target_provider != Provider::Bangumi
        || media_kind != MediaKind::Anime
        || field != SyncField::ProgressEpisodes
    {
        return None;
    }

    Some(PlanDiagnostic {
        work_id,
        source_provider,
        target_provider,
        target_provider_entry_id,
        media_kind,
        field,
        reason:
            "Bangumi anime episode progress writes require resolved episode IDs; aggregate progress is unsupported"
                .to_owned(),
    })
}

fn target_writable_fields(
    target_provider: Provider,
    media_kind: MediaKind,
    fields: Vec<SyncField>,
) -> Vec<SyncField> {
    fields
        .into_iter()
        .filter(|field| target_supports_write_field(target_provider, media_kind, *field))
        .collect()
}

fn target_supports_write_field(
    target_provider: Provider,
    media_kind: MediaKind,
    field: SyncField,
) -> bool {
    match field {
        SyncField::Status | SyncField::Score => true,
        SyncField::ProgressEpisodes => {
            media_kind == MediaKind::Anime && target_provider != Provider::Bangumi
        }
        SyncField::ProgressChapters | SyncField::ProgressVolumes => media_kind == MediaKind::Manga,
        SyncField::RepeatCount
        | SyncField::StartedAt
        | SyncField::CompletedAt
        | SyncField::Notes
        | SyncField::Tags => false,
    }
}

fn newest_reliable_entry(entries: &[PlanningCollectionEntry]) -> Option<&PlanningCollectionEntry> {
    let latest_timestamp = entries
        .iter()
        .filter_map(|entry| entry.provider_updated_at_epoch_secs)
        .max()?;
    let mut latest_entries = entries
        .iter()
        .filter(|entry| entry.provider_updated_at_epoch_secs == Some(latest_timestamp))
        .collect::<Vec<_>>();

    if latest_entries.len() != 1 {
        return None;
    }

    let latest = latest_entries.pop().expect("len checked above");
    if is_high_reliability_timestamp_provider(latest.provider) {
        Some(latest)
    } else {
        None
    }
}

fn is_high_reliability_timestamp_provider(provider: Provider) -> bool {
    matches!(provider, Provider::AniList | Provider::MyAnimeList)
}

fn conflict_for_field(
    work_id: i64,
    entries: &[PlanningCollectionEntry],
    field: SyncField,
) -> Option<PlanConflict> {
    let external_clear = entries.iter().any(|entry| {
        entry
            .field_observations
            .get(&field)
            .is_some_and(|observation| observation.externally_cleared)
    });
    let mut distinct = HashSet::new();
    let values = entries
        .iter()
        .map(|entry| {
            let value = field_value(entry, field);
            if value != UNSET_FIELD_VALUE || external_clear {
                distinct.insert(value.clone());
            }
            FieldProviderValue {
                provider: entry.provider,
                value,
            }
        })
        .collect::<Vec<_>>();

    if distinct.len() <= 1 {
        return None;
    }

    Some(PlanConflict {
        work_id,
        field,
        values,
        reason: "multiple source values for default-writable field; dry-run blocks writes"
            .to_owned(),
    })
}

fn target_matches_source(
    target: &PlanningCollectionEntry,
    source: &PlanningCollectionEntry,
    field: SyncField,
) -> bool {
    canonical_state(target).value(field)
        == canonical_state(source).target_value(target.provider, field)
}

fn planned_field_changes(
    target: Option<&PlanningCollectionEntry>,
    source: &PlanningCollectionEntry,
    fields: &[SyncField],
) -> Vec<PlannedFieldChange> {
    fields
        .iter()
        .map(|field| PlannedFieldChange {
            field: *field,
            old_value: target.and_then(|entry| field_value_opt(entry, *field)),
            new_value: field_value_opt(source, *field)
                .unwrap_or_else(|| UNSET_FIELD_VALUE.to_owned()),
        })
        .collect()
}

fn field_value(entry: &PlanningCollectionEntry, field: SyncField) -> String {
    field_value_opt(entry, field).unwrap_or_else(|| UNSET_FIELD_VALUE.to_owned())
}

fn field_value_opt(entry: &PlanningCollectionEntry, field: SyncField) -> Option<String> {
    match field {
        SyncField::Status => canonical_state(entry).value(field),
        SyncField::Score => entry.score_hundred.map(|value| value.to_string()),
        SyncField::ProgressEpisodes => entry.progress_episodes.map(|value| value.to_string()),
        SyncField::ProgressChapters => entry.progress_chapters.map(|value| value.to_string()),
        SyncField::ProgressVolumes => entry.progress_volumes.map(|value| value.to_string()),
        SyncField::RepeatCount
        | SyncField::StartedAt
        | SyncField::CompletedAt
        | SyncField::Notes
        | SyncField::Tags => Some("<protected>".to_owned()),
    }
}

fn canonical_state(entry: &PlanningCollectionEntry) -> CanonicalFieldState {
    CanonicalFieldState::new(
        entry.status,
        entry.score_hundred,
        entry.progress_episodes,
        entry.progress_chapters,
        entry.progress_volumes,
    )
}

fn consensus_score(entries: &[PlanningCollectionEntry]) -> Option<u8> {
    entries.iter().find_map(|entry| entry.score_hundred)
}

fn consensus_progress(entries: &[PlanningCollectionEntry], field: SyncField) -> Option<u32> {
    entries.iter().find_map(|entry| match field {
        SyncField::ProgressEpisodes => entry.progress_episodes,
        SyncField::ProgressChapters => entry.progress_chapters,
        SyncField::ProgressVolumes => entry.progress_volumes,
        SyncField::Status
        | SyncField::Score
        | SyncField::RepeatCount
        | SyncField::StartedAt
        | SyncField::CompletedAt
        | SyncField::Notes
        | SyncField::Tags => None,
    })
}

fn missing_writable_fields(
    target: &PlanningCollectionEntry,
    entries: &[PlanningCollectionEntry],
) -> Vec<SyncField> {
    let mut fields = Vec::new();

    if target.score_hundred.is_none() && consensus_score(entries).is_some() {
        fields.push(SyncField::Score);
    }
    if target.progress_episodes.is_none()
        && consensus_progress(entries, SyncField::ProgressEpisodes).is_some()
    {
        fields.push(SyncField::ProgressEpisodes);
    }
    if target.progress_chapters.is_none()
        && consensus_progress(entries, SyncField::ProgressChapters).is_some()
    {
        fields.push(SyncField::ProgressChapters);
    }
    if target.progress_volumes.is_none()
        && consensus_progress(entries, SyncField::ProgressVolumes).is_some()
    {
        fields.push(SyncField::ProgressVolumes);
    }

    fields
}

fn fields_different_from_authoritative(
    target: &PlanningCollectionEntry,
    authoritative: &PlanningCollectionEntry,
) -> Vec<SyncField> {
    let mut fields = Vec::new();

    if target.status != authoritative.status {
        fields.push(SyncField::Status);
    }
    if authoritative.score_hundred.is_some() && target.score_hundred != authoritative.score_hundred
    {
        fields.push(SyncField::Score);
    }
    if authoritative.progress_episodes.is_some()
        && target.progress_episodes != authoritative.progress_episodes
    {
        fields.push(SyncField::ProgressEpisodes);
    }
    if authoritative.progress_chapters.is_some()
        && target.progress_chapters != authoritative.progress_chapters
    {
        fields.push(SyncField::ProgressChapters);
    }
    if authoritative.progress_volumes.is_some()
        && target.progress_volumes != authoritative.progress_volumes
    {
        fields.push(SyncField::ProgressVolumes);
    }

    fields
}

fn has_field_value(entry: &PlanningCollectionEntry, field: SyncField) -> bool {
    match field {
        SyncField::Status => true,
        SyncField::Score => entry.score_hundred.is_some(),
        SyncField::ProgressEpisodes => entry.progress_episodes.is_some(),
        SyncField::ProgressChapters => entry.progress_chapters.is_some(),
        SyncField::ProgressVolumes => entry.progress_volumes.is_some(),
        SyncField::RepeatCount
        | SyncField::StartedAt
        | SyncField::CompletedAt
        | SyncField::Notes
        | SyncField::Tags => false,
    }
}

fn writable_fields_present(entries: &[PlanningCollectionEntry]) -> Vec<SyncField> {
    let mut fields = vec![SyncField::Status];

    if consensus_score(entries).is_some() {
        fields.push(SyncField::Score);
    }
    if consensus_progress(entries, SyncField::ProgressEpisodes).is_some() {
        fields.push(SyncField::ProgressEpisodes);
    }
    if consensus_progress(entries, SyncField::ProgressChapters).is_some() {
        fields.push(SyncField::ProgressChapters);
    }
    if consensus_progress(entries, SyncField::ProgressVolumes).is_some() {
        fields.push(SyncField::ProgressVolumes);
    }

    fields
}

fn writable_fields_present_for_entry(entry: &PlanningCollectionEntry) -> Vec<SyncField> {
    let mut fields = vec![SyncField::Status];

    if entry.score_hundred.is_some() {
        fields.push(SyncField::Score);
    }
    if entry.progress_episodes.is_some() {
        fields.push(SyncField::ProgressEpisodes);
    }
    if entry.progress_chapters.is_some() {
        fields.push(SyncField::ProgressChapters);
    }
    if entry.progress_volumes.is_some() {
        fields.push(SyncField::ProgressVolumes);
    }

    fields
}
