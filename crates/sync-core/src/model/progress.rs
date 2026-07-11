use super::MediaKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    episodes: Option<u32>,
    chapters: Option<u32>,
    volumes: Option<u32>,
}

impl Progress {
    pub const fn none() -> Self {
        Self {
            episodes: None,
            chapters: None,
            volumes: None,
        }
    }

    pub const fn anime(episodes: u32) -> Self {
        Self {
            episodes: Some(episodes),
            chapters: None,
            volumes: None,
        }
    }

    pub const fn manga(chapters: Option<u32>, volumes: Option<u32>) -> Self {
        Self {
            episodes: None,
            chapters,
            volumes,
        }
    }

    pub const fn episodes(self) -> Option<u32> {
        self.episodes
    }

    pub const fn chapters(self) -> Option<u32> {
        self.chapters
    }

    pub const fn volumes(self) -> Option<u32> {
        self.volumes
    }

    pub const fn is_compatible_with(self, media_kind: MediaKind) -> bool {
        match media_kind {
            MediaKind::Anime => self.chapters.is_none() && self.volumes.is_none(),
            MediaKind::Manga => self.episodes.is_none(),
        }
    }
}
