mod auto_link;
mod crosswalk_import;
mod dataset_import;
mod manual;
mod matcher;

pub use auto_link::{
    auto_link_provider_item_match, preview_auto_link_provider_item_match, AutoLinkReport,
    AutoLinkedEdge,
};
pub use crosswalk_import::{
    import_anilist_id_mal_crosswalk, AnilistMalCrosswalk, CrosswalkImportError,
};
pub use dataset_import::{
    import_anime_offline_database, import_bangumi_dataset, validate_anime_offline_database_json,
    validate_bangumi_dataset_json, DatasetImportError,
};
pub use manual::{
    import_legacy_ignore_entries, import_legacy_manual_relations,
    validate_legacy_ignore_entries_json, validate_legacy_manual_relations_json, LegacyImportError,
};
pub use matcher::{
    match_provider_items, match_provider_items_with_metadata, select_auto_match, AutoMatchDecision,
    MatchCandidate,
};
