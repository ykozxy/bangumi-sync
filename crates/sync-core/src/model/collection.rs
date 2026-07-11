use super::{CollectionStatus, MediaKind, Progress, Provider, Score};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionEntry {
    provider: Provider,
    provider_entry_id: String,
    media_kind: MediaKind,
    status: CollectionStatus,
    score: Option<Score>,
    progress: Progress,
    provider_updated_at_epoch_secs: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionEntryError {
    MissingProviderEntryId,
    IncompatibleProgress { media_kind: MediaKind },
}

impl CollectionEntry {
    pub fn new(
        provider: Provider,
        provider_entry_id: impl Into<String>,
        media_kind: MediaKind,
        status: CollectionStatus,
        score: Option<Score>,
        progress: Progress,
    ) -> Result<Self, CollectionEntryError> {
        let provider_entry_id = provider_entry_id.into();

        if provider_entry_id.trim().is_empty() {
            return Err(CollectionEntryError::MissingProviderEntryId);
        }

        if !progress.is_compatible_with(media_kind) {
            return Err(CollectionEntryError::IncompatibleProgress { media_kind });
        }

        Ok(Self {
            provider,
            provider_entry_id,
            media_kind,
            status,
            score,
            progress,
            provider_updated_at_epoch_secs: None,
        })
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn provider_entry_id(&self) -> &str {
        &self.provider_entry_id
    }

    pub fn media_kind(&self) -> MediaKind {
        self.media_kind
    }

    pub fn status(&self) -> CollectionStatus {
        self.status
    }

    pub fn score(&self) -> Option<Score> {
        self.score
    }

    pub fn progress(&self) -> Progress {
        self.progress
    }

    pub fn provider_updated_at_epoch_secs(&self) -> Option<i64> {
        self.provider_updated_at_epoch_secs
    }

    pub fn with_provider_updated_at_epoch_secs(mut self, value: Option<i64>) -> Self {
        self.provider_updated_at_epoch_secs = value;
        self
    }
}
