CREATE TABLE provider_item_alias (
    provider_item_id INTEGER NOT NULL REFERENCES provider_item(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    PRIMARY KEY (provider_item_id, alias)
);

CREATE VIRTUAL TABLE provider_item_fts USING fts5(
    provider_item_id UNINDEXED,
    media_kind UNINDEXED,
    canonical_title,
    aliases
);
