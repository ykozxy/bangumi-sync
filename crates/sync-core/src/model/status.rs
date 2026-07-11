#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollectionStatus {
    InProgress,
    Completed,
    Paused,
    Dropped,
    Planned,
}

impl CollectionStatus {
    pub const fn is_active(self) -> bool {
        matches!(self, Self::InProgress)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Dropped)
    }
}
