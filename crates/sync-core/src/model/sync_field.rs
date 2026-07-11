#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyncField {
    Status,
    Score,
    ProgressEpisodes,
    ProgressChapters,
    ProgressVolumes,
    RepeatCount,
    StartedAt,
    CompletedAt,
    Notes,
    Tags,
}

impl SyncField {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Score => "score",
            Self::ProgressEpisodes => "progress_episodes",
            Self::ProgressChapters => "progress_chapters",
            Self::ProgressVolumes => "progress_volumes",
            Self::RepeatCount => "repeat_count",
            Self::StartedAt => "started_at",
            Self::CompletedAt => "completed_at",
            Self::Notes => "notes",
            Self::Tags => "tags",
        }
    }

    pub const fn is_default_writable(self) -> bool {
        matches!(
            self,
            Self::Status
                | Self::Score
                | Self::ProgressEpisodes
                | Self::ProgressChapters
                | Self::ProgressVolumes
        )
    }
}
