CREATE TABLE provider_item (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    canonical_title TEXT NOT NULL,
    format TEXT,
    source_payload_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (provider, media_kind, external_id)
);

CREATE TABLE identity_work (
    id INTEGER PRIMARY KEY,
    media_kind TEXT NOT NULL,
    display_title TEXT NOT NULL,
    confidence_summary TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE external_id_edge (
    id INTEGER PRIMARY KEY,
    work_id INTEGER NOT NULL REFERENCES identity_work(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 1000),
    match_method TEXT NOT NULL,
    dataset_version TEXT,
    verified_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    stale INTEGER NOT NULL DEFAULT 0,
    UNIQUE (provider, media_kind, external_id)
);

CREATE TABLE collection_entry (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL,
    work_id INTEGER REFERENCES identity_work(id) ON DELETE SET NULL,
    provider TEXT NOT NULL,
    provider_entry_id TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    score_hundred INTEGER CHECK (score_hundred BETWEEN 0 AND 100),
    progress_episodes INTEGER,
    progress_chapters INTEGER,
    progress_volumes INTEGER,
    repeat_count INTEGER,
    private_fields_hash TEXT,
    provider_payload_hash TEXT NOT NULL,
    provider_updated_at_epoch_secs INTEGER,
    observed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (account_id, provider, media_kind, provider_entry_id)
);

CREATE TABLE field_provenance (
    id INTEGER PRIMARY KEY,
    collection_entry_id INTEGER NOT NULL REFERENCES collection_entry(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    provider TEXT NOT NULL,
    observed_value_hash TEXT NOT NULL,
    observed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_reliability TEXT NOT NULL,
    UNIQUE (collection_entry_id, field_name, provider)
);

CREATE TABLE sync_state (
    id INTEGER PRIMARY KEY,
    account_group TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    field_policy TEXT NOT NULL,
    last_planned_at TEXT,
    last_applied_at TEXT,
    UNIQUE (account_group, media_kind)
);

CREATE TABLE write_journal (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    account_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response_hash TEXT,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    result_status TEXT NOT NULL,
    UNIQUE (provider, account_id, operation_id)
);

CREATE TABLE conflict (
    id INTEGER PRIMARY KEY,
    work_id INTEGER REFERENCES identity_work(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    provider_values_json TEXT NOT NULL,
    selected_resolution TEXT,
    reason TEXT,
    resolved_at TEXT
);

CREATE TABLE manual_mapping (
    id INTEGER PRIMARY KEY,
    work_id INTEGER REFERENCES identity_work(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    decision TEXT NOT NULL,
    note TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (provider, media_kind, external_id, decision)
);

CREATE INDEX idx_external_id_edge_lookup
    ON external_id_edge(provider, media_kind, external_id);

CREATE INDEX idx_external_id_edge_work
    ON external_id_edge(work_id, provider, media_kind);

CREATE INDEX idx_provider_item_lookup
    ON provider_item(provider, media_kind, external_id);

CREATE INDEX idx_provider_item_media_kind
    ON provider_item(media_kind);

CREATE INDEX idx_collection_entry_work_kind
    ON collection_entry(work_id, media_kind);

CREATE INDEX idx_collection_entry_account_work
    ON collection_entry(account_id, work_id);

CREATE INDEX idx_field_provenance_lookup
    ON field_provenance(collection_entry_id, field_name);

CREATE INDEX idx_write_journal_provider_account_started
    ON write_journal(provider, account_id, started_at);

CREATE INDEX idx_conflict_work_field_resolved
    ON conflict(work_id, field_name, resolved_at);
