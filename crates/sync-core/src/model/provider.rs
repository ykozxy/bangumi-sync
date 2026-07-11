#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    Bangumi,
    AniList,
    MyAnimeList,
}

impl Provider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bangumi => "bangumi",
            Self::AniList => "anilist",
            Self::MyAnimeList => "myanimelist",
        }
    }
}
