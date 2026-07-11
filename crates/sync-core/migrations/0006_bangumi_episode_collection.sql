CREATE TABLE bangumi_episode_collection (
    id INTEGER PRIMARY KEY,
    account_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    episode_id INTEGER NOT NULL,
    episode_sort REAL NOT NULL,
    collection_type INTEGER NOT NULL,
    updated_at_epoch_secs INTEGER,
    observed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (account_id, subject_id, episode_id)
);

CREATE INDEX idx_bangumi_episode_collection_subject_sort
    ON bangumi_episode_collection(account_id, subject_id, episode_sort, episode_id);
