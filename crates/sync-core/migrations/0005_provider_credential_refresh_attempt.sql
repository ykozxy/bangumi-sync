CREATE TABLE provider_credential_refresh_attempt (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    account_id TEXT NOT NULL,
    previous_credential_store_ref TEXT NOT NULL,
    attempted_credential_store_ref TEXT,
    access_token_expires_at_epoch_secs INTEGER,
    refresh_token_state TEXT CHECK (refresh_token_state IN ('absent', 'present_unknown', 'present_expires_at')),
    refresh_token_expires_at_epoch_secs INTEGER,
    last_refresh_at_epoch_secs INTEGER,
    status TEXT NOT NULL CHECK (status IN ('pending', 'committed', 'failed')),
    failure_kind TEXT,
    started_at_epoch_secs INTEGER NOT NULL,
    completed_at_epoch_secs INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (
        (status = 'pending' AND attempted_credential_store_ref IS NULL AND completed_at_epoch_secs IS NULL)
        OR (status = 'committed' AND attempted_credential_store_ref IS NOT NULL AND completed_at_epoch_secs IS NOT NULL)
        OR (status = 'failed' AND completed_at_epoch_secs IS NOT NULL)
    ),
    CHECK (
        refresh_token_state IS NULL
        OR (refresh_token_state = 'present_expires_at' AND refresh_token_expires_at_epoch_secs IS NOT NULL)
        OR (refresh_token_state != 'present_expires_at' AND refresh_token_expires_at_epoch_secs IS NULL)
    )
);

CREATE INDEX idx_provider_credential_refresh_attempt_provider_account
    ON provider_credential_refresh_attempt(provider, account_id, id);
