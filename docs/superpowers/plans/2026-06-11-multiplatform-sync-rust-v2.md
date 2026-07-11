# Multiplatform Sync Rust v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to execute this plan. Use sub-agents for independent research, API adapter tests, matcher validation, and migration checks. Do not write to real user accounts. Do not merge this branch into `master` or `main`.

Status: execution in progress. Phases 1-5A and parts of Phases 6-7 exist in the pre-alpha implementation; live writes remain disabled.
Date: 2026-06-11
Last reviewed: 2026-07-11
Branch: `codex/rust-sync-v2`
Design: `docs/superpowers/specs/2026-06-11-multiplatform-sync-rust-v2-design.md`

## Goal

Build a Rust v2 sync engine that supports Bangumi, AniList, and MyAnimeList collection synchronization for anime and manga, with durable identity matching, dry-run planning, conflict handling, and safe opt-in writes.

## Architecture

Keep the current TypeScript project as the stable baseline. Add a separate Rust workspace for v2. The v2 engine is stage-based:

```text
provider snapshots -> normalization -> identity graph -> sync planner -> dry-run report -> optional apply -> verify -> journal
```

Default behavior is read-only and dry-run. Writes are implemented only after the planner, matcher, and provider adapters are covered by fixture and mock HTTP tests.

## Tech Stack

- Rust workspace for v2 implementation.
- SQLite with migrations for cache, identity graph, snapshots, and write journal.
- Credential lifecycle model for auth-broker bootstrap, official refresh, and reauthorization preflight.
- HTTP client with retry and provider-aware rate limiting.
- Mock HTTP tests for provider adapters.
- Golden-file tests for sync plan output.
- Existing TypeScript code remains available for comparison and rollback.

## Phase 0: Baseline and Guardrails

**Purpose:** Make the existing project safe to evolve and establish current behavior.

Steps:

1. Confirm the branch is `codex/rust-sync-v2`.
2. Record that existing working tree modifications are pre-existing and must not be reverted.
3. Repair local JavaScript dependencies only if approved or needed for baseline tests.
4. Run the existing TypeScript build after dependency repair.
5. Capture a baseline dry-run or fixture output from the current Bangumi-to-AniList flow without writing to real accounts.
6. Add a top-level safety note to developer docs stating that live writes and deletes require explicit approval.

Verification:

```bash
npm run build
```

Exit criteria:

- Existing TypeScript build status is known.
- Current one-way behavior is documented.
- No real account writes are performed.

## Phase 1: Rust Workspace Skeleton

**Purpose:** Add Rust v2 without changing the existing TypeScript runtime.

Files to add:

- `Cargo.toml`
- `crates/sync-core/Cargo.toml`
- `crates/sync-core/src/lib.rs`
- `crates/sync-core/src/model/mod.rs`
- `crates/sync-core/src/provider/mod.rs`
- `crates/sync-core/src/identity/mod.rs`
- `crates/sync-core/src/sync/mod.rs`
- `crates/sync-core/src/store/mod.rs`
- `crates/sync-cli/Cargo.toml`
- `crates/sync-cli/src/main.rs`

Implementation tasks:

1. Define workspace crates.
2. Add CLI command surface with read-only commands:
   - `sync-v2 import-fixtures`
   - `sync-v2 plan`
   - `sync-v2 inspect-match`
3. Add model enums for provider, media kind, status, and sync fields.
4. Add fixture loading support before live providers.

Verification:

```bash
cargo fmt --all
cargo test --workspace
cargo run -p sync-cli -- --help
```

Exit criteria:

- Rust workspace builds.
- CLI exposes read-only commands.
- No provider credentials are required.

## Phase 2: Normalized Data Model

**Purpose:** Represent anime and manga without forcing provider-specific fields into the old anime-only shape.

Files to modify or add:

- `crates/sync-core/src/model/media.rs`
- `crates/sync-core/src/model/collection.rs`
- `crates/sync-core/src/model/status.rs`
- `crates/sync-core/src/model/provider.rs`
- `crates/sync-core/src/model/score.rs`
- `crates/sync-core/src/model/tests.rs`

Implementation tasks:

1. Define `MediaKind = Anime | Manga`.
2. Define provider-neutral collection fields:
   - status
   - score
   - progress episodes
   - progress chapters
   - progress volumes
   - repeat count
   - started date
   - completed date
   - notes/tags as protected fields
3. Implement provider status mappings.
4. Implement score conversion as explicit policy, not implicit overwrite.
5. Add tests for lossy mappings and unsupported fields.

Verification:

```bash
cargo test -p sync-core model
```

Exit criteria:

- Anime and manga have distinct progress fields.
- Lossy provider mappings are represented explicitly.
- Protected fields cannot be planned for writes by default.

## Phase 3: SQLite Store and Migrations

**Purpose:** Replace per-run in-memory rebuilds with durable indexed storage.

Files to add:

- `crates/sync-core/src/store/sqlite.rs`
- `crates/sync-core/src/store/migrations.rs`
- `crates/sync-core/migrations/0001_initial.sql`
- `crates/sync-core/migrations/0002_fts_indexes.sql`

Implementation tasks:

1. Create tables for provider items, identity works, external id edges, collection entries, field provenance, sync state, write journal, conflicts, and manual mappings.
2. Add FTS tables for title and alias search.
3. Add source payload hash columns for incremental imports.
4. Add repository methods for direct external id lookup and candidate search.
5. Add in-memory SQLite tests.

Verification:

```bash
cargo test -p sync-core store
```

Exit criteria:

- Migrations run on an empty database.
- External id lookup is indexed.
- FTS candidate search returns deterministic ordered candidates.

## Phase 4: Identity Graph and Matching

**Purpose:** Improve matching accuracy and performance.

Files to add or modify:

- `crates/sync-core/src/identity/graph.rs`
- `crates/sync-core/src/identity/crosswalk_import.rs`
- `crates/sync-core/src/identity/matcher.rs`
- `crates/sync-core/src/identity/confidence.rs`
- `crates/sync-core/src/identity/manual.rs`
- `crates/sync-core/tests/matching_golden.rs`

Sub-agent assignments:

- Agent A: analyze existing `config/manual_relations.json` and `config/ignore_entries.json` and define migration rules.
- Agent B: build fixture cases for exact matches, ambiguous titles, sequels, specials, and cross-media false positives.
- Agent C: validate AniList `idMal` and anime-offline-database import behavior against small fixture subsets.

Implementation tasks:

1. Import existing manual relations as high-confidence edges.
2. Import ignore entries as negative edges.
3. Import AniList `idMal` as strong AniList-to-MAL edges.
4. Import anime-offline-database edges for anime.
5. Implement FTS candidate search for missing edges.
6. Implement confidence scoring and threshold decisions.
7. Emit review reports for ambiguous or medium-confidence matches.

Verification:

```bash
cargo test -p sync-core identity
cargo test --workspace matching_golden
```

Exit criteria:

- Direct id matches avoid fuzzy search.
- Ambiguous matches do not auto-link.
- Cross-media candidates are rejected.
- Known negative matches are not suggested again.

## Phase 5: Provider Adapters in Read-Only Mode

**Purpose:** Read collection snapshots for all providers and media kinds.

Files to add:

- `crates/sync-core/src/provider/bangumi.rs`
- `crates/sync-core/src/provider/anilist.rs`
- `crates/sync-core/src/provider/myanimelist.rs`
- `crates/sync-core/src/provider/rate_limit.rs`
- `crates/sync-core/src/provider/auth.rs`
- `crates/sync-core/tests/provider_fixtures.rs`

Sub-agent assignments:

- Agent A: Bangumi anime and book collection read adapter with fixture tests.
- Agent B: AniList anime and manga GraphQL read adapter with fixture tests.
- Agent C: MAL anime and manga read adapter with fixture tests and PKCE auth notes.

Implementation tasks:

1. Define a provider trait for snapshot reads.
2. Implement Bangumi collection reads for subject types anime and book.
3. Implement AniList collection reads for `ANIME` and `MANGA`.
4. Implement MAL anime and manga list reads.
5. Normalize provider payloads into `collection_entry`.
6. Capture raw payload hashes.
7. Respect provider rate limits.

Verification:

```bash
cargo test -p sync-core provider
cargo run -p sync-cli -- import-fixtures --provider bangumi --kind anime
cargo run -p sync-cli -- import-fixtures --provider anilist --kind manga
cargo run -p sync-cli -- import-fixtures --provider myanimelist --kind anime
```

Exit criteria:

- Fixture snapshots import without network.
- Live read commands can be tested manually without writes.
- Provider adapters preserve raw payloads for audit.

## Phase 5A: Credential Lifecycle and Auth Broker Boundary

**Purpose:** Make provider auth behavior explicit before adding live provider reads or operational Docker/NAS mode.

Files to modify:

- `crates/sync-core/src/provider/auth.rs`
- `crates/sync-core/src/provider/mod.rs`
- `crates/sync-core/tests/provider_auth_rate_limit.rs`
- `docs/superpowers/specs/2026-06-11-multiplatform-sync-rust-v2-design.md`

Implementation tasks:

1. Model provider credential capabilities:
   - Bangumi: authorization code, refresh token supported.
   - AniList: authorization code, refresh token not supported, auth-broker reauthorization required before expiry.
   - MyAnimeList: authorization code with PKCE, refresh token supported.
2. Model supported bootstrap modes:
   - auth-broker browser authorization.
   - localhost callback for headless/NAS via SSH tunnel.
   - AniList manual pin fallback.
3. Add credential preflight actions:
   - use stored access token.
   - refresh with provider.
   - reauthorize with auth-broker.
4. Add SQLite credential metadata storage:
   - store provider, account id, auth flow, bootstrap mode, credential store reference, access token expiry, refresh token state, last refresh, and last reauth request.
   - do not store access token values, refresh token values, browser cookies, raw OAuth codes, or web-session material.
5. Add read-only CLI preflight:
   - `sync-v2 auth status --db <path> --provider <provider> --account <id>`
   - read local credential metadata and print whether the daemon should use, refresh, or reauthorize.
   - reject `--refresh`, `--apply`, `--write`, and `--delete`.
6. Keep token values outside debug output and request URLs.
7. Do not implement cookie extraction, browser profile access, remote debugging, or web-session scraping.
8. Add pure OAuth request-shape builders:
   - AniList authorization-code and token exchange from official docs; refresh must be rejected.
   - MyAnimeList authorization-code with PKCE plain, token exchange, and refresh from official docs.
   - Bangumi authorization/token/refresh marked as legacy repo-derived until current official OAuth endpoint docs are confirmed.
   - no browser open, callback server, token exchange, or live network side effects.
9. Add mockable token exchange boundary:
   - token exchange executor accepts injected transport and injected secret store traits only.
   - fake transport tests normalize token JSON into metadata and store secret material only through the secret store trait.
   - SQLite helper persists exchange metadata and `credential_store_ref` without storing token values.
   - no default HTTP client, keychain implementation, browser opener, callback listener, or environment token reader in `sync-core`.

Verification:

```bash
cargo test -p sync-core --test provider_auth_rate_limit
cargo test -p sync-cli --test cli_contract auth_status
```

Exit criteria:

- Tests prove Bangumi and MAL can be planned for provider refresh.
- Tests prove AniList is planned for auth-broker reauthorization instead of refresh.
- The sync core exposes credential decisions and metadata storage without opening a browser or touching live accounts.
- The CLI can report credential preflight status from local SQLite without refreshing tokens or starting browser auth.
- OAuth request builders are pure data constructors, redact secrets in debug output, and encode source provenance for legacy-derived Bangumi endpoints.
- Token exchange tests prove fake transport/secret-store boundaries, failure paths, redaction, and metadata-only SQLite persistence.

## Phase 6: Sync Planner Dry-Run

**Purpose:** Produce safe sync plans without modifying remote accounts.

Files to add:

- `crates/sync-core/src/sync/planner.rs`
- `crates/sync-core/src/sync/policy.rs`
- `crates/sync-core/src/sync/conflict.rs`
- `crates/sync-core/src/sync/report.rs`
- `crates/sync-core/tests/planner_golden.rs`

Implementation tasks:

1. Compare provider collection entries field by field.
2. Use write journal and provider timestamps to infer changed fields.
3. Treat Bangumi timestamps as low reliability.
4. Produce actions only for safe fields under default policy.
5. Emit conflicts when multiple providers changed a field.
6. Generate human-readable and machine-readable plan reports.

Verification:

```bash
cargo test -p sync-core sync
cargo test --workspace planner_golden
cargo run -p sync-cli -- plan --dry-run --fixture fixtures/three-provider-anime.json
```

Exit criteria:

- Dry-run is the default.
- Planner never emits deletes under default policy.
- Conflicts block writes for affected fields.
- Reports explain source, target, field, old value, new value, and reason.

## Phase 7: Write Adapters Behind Explicit Flags

**Purpose:** Add write capability without enabling unsafe behavior.

Files to add or modify:

- `crates/sync-core/src/provider/bangumi_write.rs`
- `crates/sync-core/src/provider/anilist_write.rs`
- `crates/sync-core/src/provider/myanimelist_write.rs`
- `crates/sync-core/src/sync/apply.rs`
- `crates/sync-core/src/sync/journal.rs`

Implementation tasks:

1. Implement provider write methods for safe fields only.
2. Require `--apply` plus provider selection to write.
3. Require an additional confirmation flag for comments, notes, tags, and deletes.
4. Record write intent before network call.
5. Record provider response after network call.
6. Re-read modified entries and verify expected state.

Verification:

```bash
cargo test -p sync-core apply
cargo run -p sync-cli -- plan --dry-run --fixture fixtures/write-plan.json
```

Exit criteria:

- Write code is covered by mock HTTP tests.
- Dry-run remains default.
- No delete path is reachable without explicit destructive confirmation.
- Write journal records planned, attempted, succeeded, and failed states.

## Phase 8: Throwaway Account End-to-End Tests

**Purpose:** Prove live provider writes without touching personal data.

Required setup:

- Throwaway Bangumi account.
- Throwaway AniList account.
- Throwaway MAL account.
- Test OAuth credentials scoped to throwaway accounts.

Sub-agent assignments:

- Agent A: Bangumi throwaway write smoke test.
- Agent B: AniList throwaway write smoke test.
- Agent C: MAL throwaway write smoke test.
- Agent D: three-provider conflict and dedupe scenario report.

Implementation tasks:

1. Create minimal collection entries on throwaway accounts.
2. Run dry-run plan.
3. Apply safe status/progress/score writes.
4. Re-read and verify.
5. Record provider-specific caveats.
6. Confirm no delete operations ran.

Verification:

```bash
cargo run -p sync-cli -- plan --dry-run --profile throwaway
cargo run -p sync-cli -- plan --apply --profile throwaway --providers bangumi,anilist,myanimelist
cargo run -p sync-cli -- verify --profile throwaway
```

Exit criteria:

- Three-provider anime sync works on throwaway accounts.
- Three-provider manga sync works on throwaway accounts.
- Journal and verification output prove the expected writes.
- No real user account was modified.

## Phase 9: Docker and Operational Mode

**Purpose:** Make v2 runnable in the same deployment style as the current project.

Files to modify:

- `Dockerfile`
- `.dockerignore`
- `README.md`
- `README_EN.md`
- `package.json` only if a TypeScript wrapper command is needed

Implementation tasks:

1. Add a Rust build stage or separate v2 image target.
2. Preserve the current TypeScript command.
3. Add v2 dry-run command examples.
4. Add config examples for provider credentials and field policies.
5. Add scheduler mode only after one-shot dry-run and apply modes are stable.

Verification:

```bash
docker build .
cargo test --workspace
npm run build
```

Exit criteria:

- Existing deployment remains available.
- v2 dry-run can run in Docker.
- Documentation clearly separates baseline and v2 behavior.

## Phase 10: Real Account Opt-In Rollout

**Purpose:** Safely use v2 with the user's real accounts after review.

Rollout steps:

1. Run read-only snapshot import for real accounts.
2. Run dry-run plan.
3. Review all unmatched, ambiguous, and conflict entries.
4. Import manual confirmations.
5. Re-run dry-run.
6. Apply writes for a small allowlisted subset.
7. Verify remote state.
8. Expand allowlist only after successful verification.

Exit criteria:

- User explicitly approves each live write batch.
- Reports show no unexpected destructive operations.
- Rollback information exists in the write journal.

## First Execution Slice After Approval

The first coding slice should be Phase 0 and Phase 1 only:

1. Repair or document current dependency/build state.
2. Add the Rust workspace skeleton.
3. Add core model enums and fixture-only CLI commands.
4. Run `cargo fmt --all` and `cargo test --workspace`.
5. Do not add live provider writes.
6. Do not change existing TypeScript sync behavior.

This slice creates the foundation needed for sub-agents to work independently in later phases.
