use super::{CollectionStatus, Provider, SyncField};

pub(crate) const UNSET_FIELD_VALUE: &str = "<unset>";
pub(crate) const MISSING_ENTRY_VALUE: &str = "<missing-entry>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CanonicalFieldState {
    status: CollectionStatus,
    score_hundred: Option<u8>,
    progress_episodes: Option<u32>,
    progress_chapters: Option<u32>,
    progress_volumes: Option<u32>,
}

impl CanonicalFieldState {
    pub(crate) const fn new(
        status: CollectionStatus,
        score_hundred: Option<u8>,
        progress_episodes: Option<u32>,
        progress_chapters: Option<u32>,
        progress_volumes: Option<u32>,
    ) -> Self {
        Self {
            status,
            score_hundred,
            progress_episodes,
            progress_chapters,
            progress_volumes,
        }
    }

    pub(crate) const fn status(self) -> CollectionStatus {
        self.status
    }

    pub(crate) const fn score_hundred(self) -> Option<u8> {
        self.score_hundred
    }

    pub(crate) const fn progress_episodes(self) -> Option<u32> {
        self.progress_episodes
    }

    pub(crate) const fn progress_chapters(self) -> Option<u32> {
        self.progress_chapters
    }

    pub(crate) const fn progress_volumes(self) -> Option<u32> {
        self.progress_volumes
    }

    pub(crate) fn value(self, field: SyncField) -> Option<String> {
        match field {
            SyncField::Status => Some(collection_status_value(self.status()).to_owned()),
            SyncField::Score => Some(optional_value(self.score_hundred())),
            SyncField::ProgressEpisodes => Some(optional_value(self.progress_episodes())),
            SyncField::ProgressChapters => Some(optional_value(self.progress_chapters())),
            SyncField::ProgressVolumes => Some(optional_value(self.progress_volumes())),
            SyncField::RepeatCount
            | SyncField::StartedAt
            | SyncField::CompletedAt
            | SyncField::Notes
            | SyncField::Tags => None,
        }
    }

    pub(crate) fn hash(self, field: SyncField) -> Option<String> {
        self.value(field).map(|value| stable_hash(&value))
    }

    pub(crate) fn target_value(
        self,
        target_provider: Provider,
        field: SyncField,
    ) -> Option<String> {
        if field == SyncField::Score
            && matches!(target_provider, Provider::Bangumi | Provider::MyAnimeList)
        {
            return Some(
                self.score_hundred()
                    .map(score_hundred_after_ten_point_round_trip)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| UNSET_FIELD_VALUE.to_owned()),
            );
        }

        self.value(field)
    }
}

pub(crate) fn stable_hash(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;

    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    format!("fnv1a64:{hash:016x}")
}

fn collection_status_value(status: CollectionStatus) -> &'static str {
    match status {
        CollectionStatus::InProgress => "in_progress",
        CollectionStatus::Completed => "completed",
        CollectionStatus::Paused => "paused",
        CollectionStatus::Dropped => "dropped",
        CollectionStatus::Planned => "planned",
    }
}

fn optional_value<T: ToString>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| UNSET_FIELD_VALUE.to_owned())
}

fn score_hundred_after_ten_point_round_trip(score_hundred: u8) -> u8 {
    (((u16::from(score_hundred) + 5) / 10).min(10) * 10) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_preserve_typed_field_values() {
        let state = CanonicalFieldState::new(
            CollectionStatus::Completed,
            Some(85),
            Some(12),
            Some(34),
            Some(5),
        );

        assert_eq!(state.status(), CollectionStatus::Completed);
        assert_eq!(state.score_hundred(), Some(85));
        assert_eq!(state.progress_episodes(), Some(12));
        assert_eq!(state.progress_chapters(), Some(34));
        assert_eq!(state.progress_volumes(), Some(5));
    }

    #[test]
    fn status_values_use_wire_snake_case() {
        let cases = [
            (CollectionStatus::InProgress, "in_progress"),
            (CollectionStatus::Completed, "completed"),
            (CollectionStatus::Paused, "paused"),
            (CollectionStatus::Dropped, "dropped"),
            (CollectionStatus::Planned, "planned"),
        ];

        for (status, expected) in cases {
            let state = CanonicalFieldState::new(status, None, None, None, None);

            assert_eq!(state.value(SyncField::Status).as_deref(), Some(expected));
        }
    }

    #[test]
    fn values_canonicalize_numbers_and_unset_fields() {
        let state =
            CanonicalFieldState::new(CollectionStatus::InProgress, Some(70), Some(12), None, None);

        assert_eq!(state.value(SyncField::Score).as_deref(), Some("70"));
        assert_eq!(
            state.value(SyncField::ProgressEpisodes).as_deref(),
            Some("12")
        );
        assert_eq!(
            state.value(SyncField::ProgressChapters).as_deref(),
            Some(UNSET_FIELD_VALUE)
        );
        assert_eq!(
            state.value(SyncField::ProgressVolumes).as_deref(),
            Some(UNSET_FIELD_VALUE)
        );
    }

    #[test]
    fn protected_fields_have_no_canonical_value_or_hash() {
        let state = CanonicalFieldState::new(CollectionStatus::InProgress, None, None, None, None);

        for field in [
            SyncField::RepeatCount,
            SyncField::StartedAt,
            SyncField::CompletedAt,
            SyncField::Notes,
            SyncField::Tags,
        ] {
            assert_eq!(state.value(field), None);
            assert_eq!(state.hash(field), None);
        }
    }

    #[test]
    fn hashes_are_fnv1a64_hashes_of_canonical_values() {
        let state =
            CanonicalFieldState::new(CollectionStatus::InProgress, Some(70), None, None, None);

        for field in [
            SyncField::Status,
            SyncField::Score,
            SyncField::ProgressEpisodes,
            SyncField::ProgressChapters,
            SyncField::ProgressVolumes,
        ] {
            let value = state.value(field).expect("field should be writable");
            assert_eq!(state.hash(field), Some(stable_hash(&value)));
        }

        assert_eq!(stable_hash("hello"), "fnv1a64:a430d84680aabd0b");
    }

    #[test]
    fn missing_entry_marker_is_distinct_from_unset_field_marker() {
        assert_eq!(UNSET_FIELD_VALUE, "<unset>");
        assert_eq!(MISSING_ENTRY_VALUE, "<missing-entry>");
        assert_ne!(UNSET_FIELD_VALUE, MISSING_ENTRY_VALUE);
    }

    #[test]
    fn target_values_model_ten_point_provider_round_trip() {
        let state =
            CanonicalFieldState::new(CollectionStatus::Completed, Some(85), None, None, None);

        assert_eq!(
            state.target_value(Provider::AniList, SyncField::Score),
            Some("85".to_owned())
        );
        assert_eq!(
            state.target_value(Provider::Bangumi, SyncField::Score),
            Some("90".to_owned())
        );
        assert_eq!(
            state.target_value(Provider::MyAnimeList, SyncField::Score),
            Some("90".to_owned())
        );
        assert_eq!(
            state.target_value(Provider::Bangumi, SyncField::Status),
            Some("completed".to_owned())
        );
    }
}
