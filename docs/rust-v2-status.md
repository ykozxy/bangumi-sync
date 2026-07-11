# Rust v2 Status

Status: pre-alpha
Last reviewed: 2026-07-11

Rust v2 is a parallel implementation. The TypeScript Bangumi-to-AniList flow
remains the operational baseline while Rust v2 proves safe multiplatform sync.
The repository and CI use the Rust version pinned in `rust-toolchain.toml`.

## Implemented

- Normalized anime and manga models for Bangumi, AniList, and MyAnimeList.
- SQLite migrations, indexed identity lookup, FTS title search, snapshots,
  credential metadata, write journal records, and Bangumi episode IDs.
- Manual relation, ignore entry, AniList `idMal`, anime-offline-database, and
  bangumi-data imports.
- Fixture parsing plus provider-shaped read and write request builders.
- Encrypted local credential bundles and OAuth authorization-session checks.
- Real HTTP transport for read-only collection refreshes.
- Dry-run planning, conflict blocking, mock apply, and fixture read-back
  verification.

## Safety boundary

- Live provider writes, deletes, notes, and tags are disabled.
- `apply` and `sync` require the local `--test-write-transport` path.
- Network OAuth token exchange and refresh are not exposed as production CLI
  operations yet.
- Personal accounts must remain read-only until throwaway-account verification
  is complete and a specific live write is approved.

Store local SQLite databases, auth sessions, fixture token responses, and
credential bundles under `.sync-v2/`. The directory is ignored by Git and
Docker. Never commit these artifacts.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm run build
npm audit --omit=dev --audit-level=high
```

## Known gaps

- The planner does not yet use historical field observations and completed
  write-journal evidence to identify which provider changed each field.
- Rate-limit headers are parsed but backoff and retry are not connected to the
  live read transport.
- Production OAuth token exchange and refresh are not wired through the CLI.
- Bangumi anime episode progress has request-shape support but is still blocked
  by the normal planner/apply path until resolved episode IDs are carried end to
  end.
- There are no throwaway-account live-write results, Rust Docker target,
  scheduler, or real-account rollout workflow.
- The TypeScript baseline still carries two moderate `npm audit` findings through
  the deprecated `node-notifier` dependency and its `uuid` child. CI blocks new
  high-severity production findings; notifier removal belongs to the separate
  TypeScript modernization slice.

## Next milestone

Deliver a repeatable read-only three-provider dry run:

1. Complete production OAuth bootstrap without browser credential extraction.
2. Add provider-aware retry and backoff.
3. Fetch real read-only snapshots into `.sync-v2/`.
4. Import local identity datasets and auto-link high-confidence entries.
5. Review JSON actions, conflicts, unmatched entries, and unsupported fields.

Historical field observations and journal-aware planning are the next core
correctness slice before any live write transport is enabled.
