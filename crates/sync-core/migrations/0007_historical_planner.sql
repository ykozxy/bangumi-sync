ALTER TABLE field_provenance
    ADD COLUMN previous_observed_value_hash TEXT;

ALTER TABLE field_provenance
    ADD COLUMN observation_version INTEGER NOT NULL DEFAULT 1
        CHECK (observation_version >= 1);

ALTER TABLE field_provenance
    ADD COLUMN last_changed_observation_version INTEGER NOT NULL DEFAULT 1
        CHECK (last_changed_observation_version >= 1);

ALTER TABLE field_provenance
    ADD COLUMN change_origin TEXT NOT NULL DEFAULT 'initial'
        CHECK (change_origin IN ('initial', 'unchanged', 'external', 'tool_write'));

ALTER TABLE field_provenance
    ADD COLUMN attributed_write_journal_field_id INTEGER;

ALTER TABLE field_provenance
    ADD COLUMN externally_cleared INTEGER NOT NULL DEFAULT 0
        CHECK (externally_cleared IN (0, 1));

ALTER TABLE field_provenance
    ADD COLUMN pending_external_change INTEGER NOT NULL DEFAULT 0
        CHECK (pending_external_change IN (0, 1));

UPDATE field_provenance
SET field_name = 'score'
WHERE field_name = 'score_hundred';

CREATE TABLE write_journal_field (
    id INTEGER PRIMARY KEY,
    write_journal_id INTEGER NOT NULL REFERENCES write_journal(id) ON DELETE CASCADE,
    work_id INTEGER NOT NULL REFERENCES identity_work(id) ON DELETE CASCADE,
    media_kind TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    target_provider_entry_id TEXT NOT NULL,
    field_name TEXT NOT NULL,
    basis_observation_version INTEGER,
    basis_snapshot_generation INTEGER NOT NULL,
    before_value_hash TEXT NOT NULL,
    source_value_hash TEXT NOT NULL,
    expected_value_hash TEXT NOT NULL,
    attributed_collection_entry_id INTEGER REFERENCES collection_entry(id) ON DELETE SET NULL,
    attributed_observation_version INTEGER,
    consumed_at TEXT,
    UNIQUE (write_journal_id, field_name)
);

CREATE TABLE collection_snapshot_state (
    account_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    generation INTEGER NOT NULL DEFAULT 0 CHECK (generation >= 0),
    provider_payload_hash TEXT NOT NULL,
    observed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (account_id, provider, media_kind)
);

CREATE INDEX idx_write_journal_field_journal
    ON write_journal_field(write_journal_id);

CREATE INDEX idx_write_journal_field_target
    ON write_journal_field(
        work_id,
        media_kind,
        target_provider_entry_id,
        field_name
    );
