pub const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_initial.sql"),
    include_str!("../../migrations/0002_fts_indexes.sql"),
    include_str!("../../migrations/0003_provider_credentials.sql"),
    include_str!("../../migrations/0004_provider_item_release_year.sql"),
    include_str!("../../migrations/0005_provider_credential_refresh_attempt.sql"),
    include_str!("../../migrations/0006_bangumi_episode_collection.sql"),
];
