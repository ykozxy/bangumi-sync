use std::collections::HashSet;

use crate::model::{MediaKind, Provider};
use crate::store::{ManualMappingDecision, SqliteStore, StoreError};
use crate::title::normalize_title;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchCandidate {
    pub provider: Provider,
    pub media_kind: MediaKind,
    pub external_id: String,
    pub canonical_title: String,
    pub release_year: Option<u16>,
    pub confidence: u16,
    pub match_method: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoMatchDecision {
    Accepted {
        provider: Provider,
        media_kind: MediaKind,
        external_id: String,
        confidence: u16,
        match_method: String,
    },
    NeedsReview {
        reason: String,
    },
}

pub fn match_provider_items(
    store: &SqliteStore,
    media_kind: MediaKind,
    query: &str,
    limit: u32,
) -> Result<Vec<MatchCandidate>, StoreError> {
    match_provider_items_with_metadata(store, media_kind, query, None, limit)
}

pub fn match_provider_items_with_metadata(
    store: &SqliteStore,
    media_kind: MediaKind,
    query: &str,
    query_release_year: Option<u16>,
    limit: u32,
) -> Result<Vec<MatchCandidate>, StoreError> {
    let normalized_query = normalize_title(query);
    let fts_queries = fts_queries_for_title(query, &normalized_query);
    if fts_queries.is_empty() {
        return Ok(Vec::new());
    };
    let mut candidates = Vec::new();
    let mut seen_items = HashSet::new();

    for fts_query in fts_queries {
        for item in store.search_all_provider_items(media_kind, &fts_query)? {
            if !seen_items.insert((item.provider, item.media_kind, item.external_id.clone())) {
                continue;
            }
            let decision =
                store.manual_mapping_decision(item.provider, item.media_kind, &item.external_id)?;
            if decision == Some(ManualMappingDecision::Ignore) {
                continue;
            }

            let normalized_title = normalize_title(&item.canonical_title);
            let alias_exact = item
                .aliases
                .iter()
                .any(|alias| normalize_title(alias) == normalized_query);
            let (confidence, match_method) = if normalized_title == normalized_query
                && release_year_mismatch(query_release_year, item.release_year)
            {
                (800, "fts-title-exact-year-mismatch")
            } else if normalized_title == normalized_query
                && release_year_unknown(query_release_year, item.release_year)
            {
                (800, "fts-title-exact-unknown-year")
            } else if normalized_title == normalized_query {
                (900, "fts-title-exact")
            } else if alias_exact && is_risky_alias_only_format(item.format.as_deref()) {
                (800, "fts-alias-exact-risky-format")
            } else if alias_exact
                && !is_safe_alias_only_format(item.media_kind, item.format.as_deref())
            {
                (800, "fts-alias-exact-unknown-format")
            } else if alias_exact && release_year_mismatch(query_release_year, item.release_year) {
                (800, "fts-alias-exact-year-mismatch")
            } else if alias_exact && release_year_unknown(query_release_year, item.release_year) {
                (800, "fts-alias-exact-unknown-year")
            } else if alias_exact {
                (900, "fts-alias-exact")
            } else {
                (650, "fts-title")
            };

            candidates.push(MatchCandidate {
                provider: item.provider,
                media_kind: item.media_kind,
                external_id: item.external_id,
                canonical_title: item.canonical_title,
                release_year: item.release_year,
                confidence,
                match_method: match_method.to_owned(),
            });
        }
    }

    candidates.sort_by(|left, right| {
        right
            .confidence
            .cmp(&left.confidence)
            .then_with(|| left.canonical_title.cmp(&right.canonical_title))
            .then_with(|| left.external_id.cmp(&right.external_id))
    });

    candidates.truncate(limit as usize);
    Ok(candidates)
}

pub fn select_auto_match(candidates: &[MatchCandidate]) -> AutoMatchDecision {
    let Some(top) = candidates.first() else {
        return AutoMatchDecision::NeedsReview {
            reason: "no-candidates".to_owned(),
        };
    };

    if top.confidence < 900 {
        return AutoMatchDecision::NeedsReview {
            reason: "low-confidence".to_owned(),
        };
    }

    if candidates
        .iter()
        .skip(1)
        .any(|candidate| candidate.confidence == top.confidence)
    {
        return AutoMatchDecision::NeedsReview {
            reason: "ambiguous-top-confidence".to_owned(),
        };
    }

    AutoMatchDecision::Accepted {
        provider: top.provider,
        media_kind: top.media_kind,
        external_id: top.external_id.clone(),
        confidence: top.confidence,
        match_method: top.match_method.clone(),
    }
}

fn fts_queries_for_title(query: &str, normalized_query: &str) -> Vec<String> {
    let mut queries = Vec::new();
    let normalized_tokens = normalized_query.split_whitespace().collect::<Vec<_>>();

    push_fts_query(&mut queries, &normalized_tokens);
    push_fts_query(&mut queries, &split_trailing_s_tokens(&normalized_tokens));
    push_fts_query(&mut queries, &drop_standalone_x_tokens(&normalized_tokens));

    let raw_separator_normalized = normalize_title_with_apostrophes_as_separators(query);
    let raw_separator_tokens = raw_separator_normalized
        .split_whitespace()
        .collect::<Vec<_>>();
    push_fts_query(&mut queries, &raw_separator_tokens);
    push_fts_query(
        &mut queries,
        &drop_standalone_x_tokens(&raw_separator_tokens),
    );

    queries
}

fn push_fts_query(queries: &mut Vec<String>, tokens: &[&str]) {
    if let Some(query) = fts_query_for_tokens(tokens) {
        if !queries.contains(&query) {
            queries.push(query);
        }
    }
}

fn fts_query_for_tokens(tokens: &[&str]) -> Option<String> {
    if tokens.is_empty() {
        return None;
    }
    Some(
        tokens
            .iter()
            .map(|token| format!("\"{token}\""))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn split_trailing_s_tokens<'a>(tokens: &[&'a str]) -> Vec<&'a str> {
    let mut split_tokens = Vec::new();

    for token in tokens {
        if token.len() > 2 && token.ends_with('s') {
            split_tokens.push(&token[..token.len() - 1]);
            split_tokens.push("s");
        } else {
            split_tokens.push(token);
        }
    }

    split_tokens
}

fn drop_standalone_x_tokens<'a>(tokens: &[&'a str]) -> Vec<&'a str> {
    if tokens.len() <= 1 {
        return tokens.to_vec();
    }

    tokens
        .iter()
        .copied()
        .filter(|token| *token != "x")
        .collect()
}

fn normalize_title_with_apostrophes_as_separators(value: &str) -> String {
    normalize_title(&value.replace(['\'', '’', 'ʼ', '＇'], " "))
}

fn is_risky_alias_only_format(format: Option<&str>) -> bool {
    let Some(format) = format else {
        return false;
    };

    matches!(
        format.trim().to_ascii_lowercase().as_str(),
        "special" | "ova" | "ona" | "movie"
    )
}

fn is_safe_alias_only_format(media_kind: MediaKind, format: Option<&str>) -> bool {
    let Some(format) = format else {
        return false;
    };
    let normalized = format.trim().to_ascii_lowercase().replace(['_', '-'], " ");

    match media_kind {
        MediaKind::Anime => matches!(normalized.as_str(), "tv" | "tv series" | "series"),
        MediaKind::Manga => matches!(normalized.as_str(), "manga" | "manhua" | "manhwa"),
    }
}

fn release_year_mismatch(
    query_release_year: Option<u16>,
    candidate_release_year: Option<u16>,
) -> bool {
    matches!(
        (query_release_year, candidate_release_year),
        (Some(query_year), Some(candidate_year)) if query_year != candidate_year
    )
}

fn release_year_unknown(
    query_release_year: Option<u16>,
    candidate_release_year: Option<u16>,
) -> bool {
    query_release_year.is_some() && candidate_release_year.is_none()
}
