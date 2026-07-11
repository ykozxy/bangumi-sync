CREATE TABLE provider_credential (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    account_id TEXT NOT NULL,
    auth_flow TEXT NOT NULL CHECK (auth_flow IN ('authorization_code', 'authorization_code_pkce_plain')),
    bootstrap_mode TEXT NOT NULL CHECK (bootstrap_mode IN ('auth_broker', 'local_callback', 'manual_pin')),
    credential_store_ref TEXT NOT NULL,
    access_token_expires_at_epoch_secs INTEGER NOT NULL,
    refresh_token_state TEXT NOT NULL CHECK (refresh_token_state IN ('absent', 'present_unknown', 'present_expires_at')),
    refresh_token_expires_at_epoch_secs INTEGER,
    last_refresh_at_epoch_secs INTEGER,
    last_reauth_request_at_epoch_secs INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (provider, account_id),
    CHECK (
        (refresh_token_state = 'present_expires_at' AND refresh_token_expires_at_epoch_secs IS NOT NULL)
        OR (refresh_token_state != 'present_expires_at' AND refresh_token_expires_at_epoch_secs IS NULL)
    )
);

CREATE INDEX idx_provider_credential_provider_account
    ON provider_credential(provider, account_id);
