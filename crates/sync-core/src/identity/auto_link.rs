use std::collections::{HashMap, HashSet};

use crate::model::{MediaKind, Provider};
use crate::store::{ExternalIdEdgeUpsertInput, SqliteStore, StoreError};

use super::matcher::{match_provider_items_with_metadata, MatchCandidate};

const AUTO_LINK_MIN_CONFIDENCE: u16 = 900;
const AUTO_LINK_CANDIDATE_LIMIT: u32 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoLinkReport {
    pub work_id: Option<i64>,
    pub linked_edges: Vec<AutoLinkedEdge>,
    pub review_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoLinkedEdge {
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub external_id: String,
    pub confidence: u16,
    pub match_method: String,
}

struct AutoLinkPlan {
    display_title: Option<String>,
    report: AutoLinkReport,
}

pub fn preview_auto_link_provider_item_match(
    store: &SqliteStore,
    provider: Provider,
    media_kind: MediaKind,
    external_id: &str,
) -> Result<AutoLinkReport, StoreError> {
    Ok(build_auto_link_plan(store, provider, media_kind, external_id)?.report)
}

pub fn auto_link_provider_item_match(
    store: &SqliteStore,
    provider: Provider,
    media_kind: MediaKind,
    external_id: &str,
) -> Result<AutoLinkReport, StoreError> {
    let plan = build_auto_link_plan(store, provider, media_kind, external_id)?;
    let mut report = plan.report;
    if report.review_reason.is_some() || report.linked_edges.is_empty() {
        return Ok(report);
    }

    let edge_inputs = report
        .linked_edges
        .iter()
        .map(|edge| ExternalIdEdgeUpsertInput {
            provider: edge.provider,
            media_kind: edge.media_kind,
            external_id: edge.external_id.clone(),
            source: "auto-match".to_owned(),
            confidence: edge.confidence,
            match_method: edge.match_method.clone(),
            dataset_version: None,
        })
        .collect::<Vec<_>>();
    let work_id = store.upsert_external_id_edges_atomically(
        report.work_id,
        media_kind,
        plan.display_title
            .as_deref()
            .expect("non-review auto-link plan has source title"),
        &edge_inputs,
    )?;
    report.work_id = Some(work_id);

    Ok(report)
}

fn build_auto_link_plan(
    store: &SqliteStore,
    provider: Provider,
    media_kind: MediaKind,
    external_id: &str,
) -> Result<AutoLinkPlan, StoreError> {
    let Some(source_item) = store.provider_item(provider, media_kind, external_id)? else {
        return Ok(review_plan("source-provider-item-not-found"));
    };

    let candidates = match_provider_items_with_metadata(
        store,
        media_kind,
        &source_item.canonical_title,
        source_item.release_year,
        AUTO_LINK_CANDIDATE_LIMIT,
    )?;
    let candidates = candidates
        .into_iter()
        .filter(|candidate| candidate.confidence >= AUTO_LINK_MIN_CONFIDENCE)
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Ok(review_plan("no-high-confidence-candidates"));
    }
    if !candidates.iter().any(|candidate| {
        candidate.provider == provider
            && candidate.media_kind == media_kind
            && candidate.external_id == external_id
    }) {
        return Ok(review_plan("source-provider-item-not-in-candidates"));
    }
    if has_conflicting_candidate_release_years(&candidates) {
        return Ok(review_plan("conflicting-candidate-release-years"));
    }
    if source_item.release_year.is_none()
        && has_incomplete_target_candidate_release_years(&candidates, provider, external_id)
    {
        return Ok(review_plan("incomplete-candidate-release-years"));
    }
    if has_duplicate_provider_candidates(&candidates) {
        return Ok(review_plan("ambiguous-provider-candidates"));
    }

    let mut existing_work_ids = HashSet::new();
    let mut candidate_existing_work_ids = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let existing_work_id = store.find_work_by_external_id(
            candidate.provider,
            candidate.media_kind,
            &candidate.external_id,
        )?;
        if let Some(work_id) = existing_work_id {
            existing_work_ids.insert(work_id);
        }
        candidate_existing_work_ids.push((candidate, existing_work_id));
    }
    if existing_work_ids.len() > 1 {
        return Ok(review_plan("existing-edge-conflict"));
    }

    let existing_work_id = existing_work_ids.iter().next().copied();
    let linked_edges = candidate_existing_work_ids
        .iter()
        .filter(|(_, existing_work_id)| existing_work_id.is_none())
        .map(|(candidate, _)| AutoLinkedEdge {
            provider: candidate.provider,
            media_kind: candidate.media_kind,
            external_id: candidate.external_id.clone(),
            confidence: candidate.confidence,
            match_method: candidate.match_method.clone(),
        })
        .collect::<Vec<_>>();

    Ok(AutoLinkPlan {
        display_title: Some(source_item.canonical_title),
        report: AutoLinkReport {
            work_id: existing_work_id,
            linked_edges,
            review_reason: None,
        },
    })
}

fn has_duplicate_provider_candidates(candidates: &[MatchCandidate]) -> bool {
    let mut counts = HashMap::new();
    for candidate in candidates {
        *counts.entry(candidate.provider).or_insert(0usize) += 1;
    }

    counts.values().any(|count| *count > 1)
}

fn has_conflicting_candidate_release_years(candidates: &[MatchCandidate]) -> bool {
    let mut known_years = HashSet::new();
    for candidate in candidates {
        if let Some(release_year) = candidate.release_year {
            known_years.insert(release_year);
        }
    }

    known_years.len() > 1
}

fn has_incomplete_target_candidate_release_years(
    candidates: &[MatchCandidate],
    source_provider: Provider,
    source_external_id: &str,
) -> bool {
    let mut has_known_target_year = false;
    let mut has_unknown_target_year = false;

    for candidate in candidates {
        if candidate.provider == source_provider && candidate.external_id == source_external_id {
            continue;
        }
        if candidate.release_year.is_some() {
            has_known_target_year = true;
        } else {
            has_unknown_target_year = true;
        }
    }

    has_known_target_year && has_unknown_target_year
}

fn review_plan(reason: &str) -> AutoLinkPlan {
    AutoLinkPlan {
        display_title: None,
        report: AutoLinkReport {
            work_id: None,
            linked_edges: Vec::new(),
            review_reason: Some(reason.to_owned()),
        },
    }
}
