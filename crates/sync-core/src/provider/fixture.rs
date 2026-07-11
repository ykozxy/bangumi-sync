use crate::model::{
    CollectionEntry, CollectionEntryError, CollectionStatus, MediaKind, Progress, Provider, Score,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCollectionSnapshot {
    provider: Provider,
    media_kind: MediaKind,
    entries: Vec<CollectionEntry>,
    identity_links: Vec<ProviderSnapshotIdentityLink>,
    raw_payload_hash: String,
    audit: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSnapshotIdentityLink {
    source_provider: Provider,
    source_media_kind: MediaKind,
    source_external_id: String,
    target_provider: Provider,
    target_media_kind: MediaKind,
    target_external_id: String,
    title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFixtureError {
    InvalidJson {
        provider: Provider,
        message: String,
    },
    MediaKindMismatch {
        provider: Provider,
        expected: MediaKind,
        actual: MediaKind,
        provider_entry_id: String,
    },
    UnsupportedMediaKind {
        provider: Provider,
        provider_entry_id: String,
        media_kind: String,
    },
    UnsupportedStatus {
        provider: Provider,
        provider_entry_id: String,
        status: String,
    },
    InvalidScore {
        provider: Provider,
        provider_entry_id: String,
        value: u16,
    },
    InvalidEntry {
        provider: Provider,
        provider_entry_id: String,
        reason: String,
    },
    InvalidTimestamp {
        provider: Provider,
        provider_entry_id: String,
        value: String,
    },
}

impl ProviderCollectionSnapshot {
    pub(crate) fn new(
        provider: Provider,
        media_kind: MediaKind,
        raw_payload: &str,
        entries: Vec<CollectionEntry>,
    ) -> Self {
        let audit = entries
            .iter()
            .map(|entry| {
                format!(
                    "{}:{} status={:?} progress={:?}",
                    provider.as_str(),
                    entry.provider_entry_id(),
                    entry.status(),
                    entry.progress()
                )
            })
            .collect();

        Self {
            provider,
            media_kind,
            entries,
            identity_links: Vec::new(),
            raw_payload_hash: raw_payload_hash(raw_payload),
            audit,
        }
    }

    pub(crate) fn with_identity_links(
        mut self,
        identity_links: Vec<ProviderSnapshotIdentityLink>,
    ) -> Self {
        self.identity_links = identity_links;
        self
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn media_kind(&self) -> MediaKind {
        self.media_kind
    }

    pub fn entries(&self) -> &[CollectionEntry] {
        &self.entries
    }

    pub fn identity_links(&self) -> &[ProviderSnapshotIdentityLink] {
        &self.identity_links
    }

    pub fn raw_payload_hash(&self) -> &str {
        &self.raw_payload_hash
    }

    pub fn audit(&self) -> &[String] {
        &self.audit
    }
}

impl ProviderSnapshotIdentityLink {
    pub(crate) fn new(
        source_provider: Provider,
        source_media_kind: MediaKind,
        source_external_id: impl Into<String>,
        target_provider: Provider,
        target_media_kind: MediaKind,
        target_external_id: impl Into<String>,
        title: Option<String>,
    ) -> Self {
        Self {
            source_provider,
            source_media_kind,
            source_external_id: source_external_id.into(),
            target_provider,
            target_media_kind,
            target_external_id: target_external_id.into(),
            title,
        }
    }

    pub fn source_provider(&self) -> Provider {
        self.source_provider
    }

    pub fn source_media_kind(&self) -> MediaKind {
        self.source_media_kind
    }

    pub fn source_external_id(&self) -> &str {
        &self.source_external_id
    }

    pub fn target_provider(&self) -> Provider {
        self.target_provider
    }

    pub fn target_media_kind(&self) -> MediaKind {
        self.target_media_kind
    }

    pub fn target_external_id(&self) -> &str {
        &self.target_external_id
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
}

pub(crate) fn invalid_json(provider: Provider, error: serde_json::Error) -> ProviderFixtureError {
    ProviderFixtureError::InvalidJson {
        provider,
        message: error.to_string(),
    }
}

pub(crate) fn status_from_provider_value(
    provider: Provider,
    provider_entry_id: &str,
    status: &str,
) -> Result<CollectionStatus, ProviderFixtureError> {
    match status {
        "1" | "wish" | "PLANNING" | "planning" | "plan_to_watch" | "plan_to_read"
        | "plan_to_watch/read" => Ok(CollectionStatus::Planned),
        "2" | "collect" | "COMPLETED" | "completed" => Ok(CollectionStatus::Completed),
        "3" | "do" | "doing" | "CURRENT" | "current" | "watching" | "reading" => {
            Ok(CollectionStatus::InProgress)
        }
        "4" | "on_hold" | "PAUSED" | "paused" => Ok(CollectionStatus::Paused),
        "5" | "dropped" | "DROPPED" => Ok(CollectionStatus::Dropped),
        "REPEATING" | "repeating" => Ok(CollectionStatus::Completed),
        other => Err(ProviderFixtureError::UnsupportedStatus {
            provider,
            provider_entry_id: provider_entry_id.to_string(),
            status: other.to_string(),
        }),
    }
}

pub(crate) fn score_from_hundred_point(
    provider: Provider,
    provider_entry_id: &str,
    value: u16,
) -> Result<Option<Score>, ProviderFixtureError> {
    if value == 0 {
        return Ok(None);
    }

    Score::from_hundred_point(value)
        .map(Some)
        .map_err(|_| ProviderFixtureError::InvalidScore {
            provider,
            provider_entry_id: provider_entry_id.to_string(),
            value,
        })
}

pub(crate) fn score_from_ten_point(
    provider: Provider,
    provider_entry_id: &str,
    value: u16,
) -> Result<Option<Score>, ProviderFixtureError> {
    let Some(hundred_point_value) = value.checked_mul(10) else {
        return Err(ProviderFixtureError::InvalidScore {
            provider,
            provider_entry_id: provider_entry_id.to_string(),
            value: u16::MAX,
        });
    };

    score_from_hundred_point(provider, provider_entry_id, hundred_point_value)
}

pub(crate) fn score_from_fractional_hundred_point(
    provider: Provider,
    provider_entry_id: &str,
    value: f64,
) -> Result<Option<Score>, ProviderFixtureError> {
    let rounded = value.round();
    if !(0.0..=100.0).contains(&rounded) {
        return Err(ProviderFixtureError::InvalidScore {
            provider,
            provider_entry_id: provider_entry_id.to_string(),
            value: rounded.clamp(0.0, f64::from(u16::MAX)) as u16,
        });
    }

    score_from_hundred_point(provider, provider_entry_id, rounded as u16)
}

pub(crate) fn progress_for_kind(
    media_kind: MediaKind,
    primary: u32,
    secondary: Option<u32>,
) -> Progress {
    match media_kind {
        MediaKind::Anime => Progress::anime(primary),
        MediaKind::Manga => Progress::manga(Some(primary), secondary),
    }
}

pub(crate) fn collection_entry(
    provider: Provider,
    provider_entry_id: String,
    media_kind: MediaKind,
    status: CollectionStatus,
    score: Option<Score>,
    progress: Progress,
) -> Result<CollectionEntry, ProviderFixtureError> {
    CollectionEntry::new(
        provider,
        provider_entry_id.clone(),
        media_kind,
        status,
        score,
        progress,
    )
    .map_err(|error| ProviderFixtureError::InvalidEntry {
        provider,
        provider_entry_id,
        reason: collection_entry_error_message(error),
    })
}

pub(crate) fn rfc3339_utc_epoch_seconds(
    provider: Provider,
    provider_entry_id: &str,
    value: Option<&str>,
) -> Result<Option<i64>, ProviderFixtureError> {
    let Some(value) = value else {
        return Ok(None);
    };

    parse_rfc3339_utc_epoch_seconds(value)
        .map(Some)
        .ok_or_else(|| ProviderFixtureError::InvalidTimestamp {
            provider,
            provider_entry_id: provider_entry_id.to_string(),
            value: value.to_string(),
        })
}

fn parse_rfc3339_utc_epoch_seconds(value: &str) -> Option<i64> {
    if value.len() != 20 || !value.ends_with('Z') {
        return None;
    }

    let year = parse_i64(value.get(0..4)?)?;
    let month = parse_i64(value.get(5..7)?)?;
    let day = parse_i64(value.get(8..10)?)?;
    let hour = parse_i64(value.get(11..13)?)?;
    let minute = parse_i64(value.get(14..16)?)?;
    let second = parse_i64(value.get(17..19)?)?;

    if value.get(4..5)? != "-"
        || value.get(7..8)? != "-"
        || value.get(10..11)? != "T"
        || value.get(13..14)? != ":"
        || value.get(16..17)? != ":"
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }

    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn parse_i64(value: &str) -> Option<i64> {
    value.parse::<i64>().ok()
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

fn collection_entry_error_message(error: CollectionEntryError) -> String {
    match error {
        CollectionEntryError::MissingProviderEntryId => "missing provider entry id".to_string(),
        CollectionEntryError::IncompatibleProgress { media_kind } => {
            format!("incompatible progress for {:?}", media_kind)
        }
    }
}

fn raw_payload_hash(payload: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;

    for byte in payload.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    format!("fnv1a64:{hash:016x}")
}
