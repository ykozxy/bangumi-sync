# Multiplatform Sync Rust v2 Design

Status: implementation in progress (pre-alpha). Live provider writes remain gated on explicit user approval and throwaway-account verification.
Date: 2026-06-11
Last reviewed: 2026-07-11
Branch: `codex/rust-sync-v2`

## Purpose

The current project synchronizes Bangumi anime collection data into AniList. The v2 project expands that into a safe, high-accuracy, high-performance synchronization engine across Bangumi, AniList, and MyAnimeList for both anime and manga.

The central design choice is to keep the current TypeScript implementation as a stable baseline while building a parallel Rust v2 core. The Rust core owns identity resolution, local storage, diff planning, conflict handling, and provider adapters. Real account writes stay disabled until dry-run plans and throwaway-account tests are proven.

## Goals

- Synchronize collection entries across Bangumi, AniList, and MyAnimeList.
- Support anime and manga/book collection data.
- Support bidirectional and eventually three-way sync.
- Improve matching accuracy with a persistent identity graph instead of repeated fuzzy scans.
- Improve performance by replacing per-run database rebuilds with SQLite-backed indexes and incremental refresh.
- Preserve existing user data by default: dry-run first, no deletes by default, and no writes to real accounts without explicit confirmation.
- Make implementation testable with fixture data, mocked provider APIs, and throwaway live accounts.
- Make credential handling operationally viable for NAS/headless deployments by separating browser-based authorization from the sync daemon.

## Non-Goals

- Do not replace the existing TypeScript flow in one step.
- Do not auto-delete remote collection entries in the default mode.
- Do not overwrite private notes, comments, tags, or rewatch/reread metadata by default.
- Do not treat title similarity alone as a safe match.
- Do not depend on the user's real browser sessions for automated write tests.
- Do not extract browser cookies, browser profile data, private web tokens, or CSRF/session material to bypass official OAuth flows.

## Current System Summary

The current code is a one-way TypeScript/JavaScript flow:

- `src/main.ts` drives Bangumi to AniList sync in single-run or server mode.
- `src/utils/bangumi_client.ts` reads Bangumi anime collection data and searches anime subjects. Subject type is hard-coded to anime.
- `src/utils/anilist_client.ts` reads AniList anime data and writes AniList MediaList entries. GraphQL media type is hard-coded to anime.
- `src/utils/sync_util.ts` maps Bangumi and AniList records into `AnimeCollection`, fills missing ids, and generates a change log.
- `src/utils/data_util.ts` loads `bangumi-data` and `anime-offline-database`, builds in-memory maps, and performs fuzzy matching.

The main limitations are:

- Anime-only data model.
- Bangumi-to-AniList only in the main path.
- AniList is the only write target.
- Matching is mostly local-dataset and fuzzy-title driven.
- Local databases are rebuilt each run.
- Matching scans can become O(n) per entry.
- There is no durable write journal, conflict store, or field-level provenance.

## Provider Capability Matrix

| Provider | Auth | Anime read | Anime write | Manga read | Manga write | Reliable per-entry updated time | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Bangumi | OAuth authorization code, bearer access token, refresh token | Yes, collection subject type `2` | Yes, collection create/update endpoints | Yes, book subject type `1` | Yes, book collection progress fields | Limited | Access tokens are short-lived enough to require refresh automation. `updated_at` is not reliable for all collection fields. Book progress uses episode/volume-style fields. |
| AniList | OAuth authorization code, bearer access token, no refresh token in official docs | Yes, `MediaListCollection(type: ANIME)` | Yes, `SaveMediaListEntry` and bulk update mutation | Yes, `MediaListCollection(type: MANGA)` | Yes, `SaveMediaListEntry` | Yes, `updatedAt` Unix seconds | Access tokens are long-lived but require reauthorization when expired. Exposes `idMal`, which is a strong crosswalk to MAL. Rate limits need header-aware throttling. |
| MyAnimeList | OAuth2 authorization code with PKCE, bearer access token, refresh token | Yes, `/users/{name}/animelist` | Yes, update anime list status | Yes, `/users/{name}/mangalist` | Yes, update manga list status | Yes, list status `updated_at` | Access tokens are short-lived; refresh tokens need proactive refresh. Delete endpoints exist but should remain disabled by default. |

## Credential Strategy

The sync daemon should not own interactive login. V2 should use a credential boundary with two roles:

- `auth-broker`: a desktop or one-shot CLI component that opens official OAuth authorization URLs in the user's browser, captures only the redirect code or official pin, validates `state`, exchanges the code through official token endpoints, and stores token metadata.
- `sync daemon`: a headless component that asks a credential provider for access tokens, refreshes tokens only through official refresh endpoints, and reports `needs_reauth` when a provider cannot be refreshed.

The browser may be used to carry the user's existing login session during OAuth authorization. It must not be used as a credential source. The implementation must not read provider cookies, browser cookies, local storage, remote debugging state, CSRF tokens, private web APIs, or page internals to construct API credentials.

Provider-specific auth behavior:

- Bangumi: use official authorization-code OAuth and refresh tokens. The daemon can refresh without user interaction while refresh remains valid.
- MyAnimeList: use official authorization-code OAuth with PKCE. The daemon must refresh before the refresh token expires.
- AniList: use official authorization-code OAuth or auth pin. Since official docs state refresh tokens are not supported, the daemon must warn before expiry and require auth-broker reauthorization.

Endpoint provenance:

- AniList authorization-code endpoints and one-year access-token behavior are from the official AniList API docs.
- MyAnimeList authorization, token exchange, and refresh endpoints are from the official MAL API authorization docs. MAL currently supports PKCE with `code_challenge_method=plain`.
- Bangumi's current OpenAPI confirms bearer-protected API routes, but not OAuth authorize/token/refresh endpoint details. The v2 request-shape model marks Bangumi OAuth endpoints as legacy repo-derived from the existing TypeScript implementation until a current official OAuth reference is confirmed.

NAS/headless deployment should use a mounted persistent token store. For trusted personal deployments, the auth-broker may install an encrypted token bundle into the NAS store. For stricter deployments, the auth-broker can keep refresh tokens locally and issue short-lived access tokens to the NAS over an authenticated local channel. The first implementation should model these capabilities and preflight decisions before implementing token storage or browser integration.

## Target Architecture

The Rust v2 implementation should be added beside the current TypeScript implementation. The first usable milestone is a dry-run CLI that can ingest snapshots, resolve identities, and emit a proposed sync plan without writing to any provider.

Recommended module layout:

```text
crates/
  sync-core/
    src/
      model/
      identity/
      sync/
      store/
      provider/
  sync-cli/
    src/main.rs
```

Recommended Rust modules:

- `provider::bangumi`: Bangumi API client, auth, collection read, collection write planner/apply methods.
- `provider::anilist`: AniList GraphQL client, auth, collection read, collection write planner/apply methods.
- `provider::myanimelist`: MyAnimeList API client, PKCE auth support, collection read, collection write planner/apply methods.
- `model`: normalized media, collection entry, status, progress, score, and provider-specific metadata.
- `identity::graph`: canonical work ids and external id edges.
- `identity::crosswalk_import`: imports `anime-offline-database`, `bangumi-data`, AniList `idMal`, and manual mappings.
- `identity::matcher`: candidate generation, scoring, confidence thresholds, and ambiguity handling.
- `sync::planner`: source-target field diffing and action planning.
- `sync::conflict`: per-field conflict detection and resolution.
- `sync::apply`: write execution with dry-run default.
- `sync::journal`: write-ahead records, provider response capture, and replay protection.
- `store::sqlite`: schema migrations, indexed queries, FTS indexes, snapshots, and cache metadata.

## Storage Model

SQLite should be the default local store. It gives durable cache, indexed identity lookup, FTS candidate search, and an auditable write journal without requiring a server.

Core tables:

- `platform_account`: provider, account id, username, token reference, last sync time.
- `provider_credential`: provider, account id, auth flow, bootstrap mode, access token expiry, refresh token state, credential store reference, last refresh time, last reauth request time. This table stores metadata and secret-store references only; it must not store access tokens, refresh tokens, browser cookies, or raw OAuth codes.
- `provider_item`: provider, media kind, provider id, titles, format, release dates, creators/studios, source payload hash.
- `identity_work`: canonical local work id, media kind, display title, confidence summary.
- `external_id_edge`: work id, provider, media kind, external id, source, confidence, match method, dataset version, verified time, stale flag.
- `collection_entry`: account id, work id, provider id, media kind, normalized status, progress, score, repeat count, private fields hash, provider payload hash.
- `collection_snapshot_state`: account id, provider, media kind, local snapshot generation, payload hash, observed time.
- `field_provenance`: collection entry id, field name, provider, current and previous value hashes, observation version, last changed version, change origin, pending external-change and external-clear markers, optional consumed journal-field attribution, observed time, source reliability.
- `sync_state`: account pair/group, field policy, last planned time, last applied time.
- `write_journal`: provider, account id, operation id, request hash, response hash, started time, completed time, result status.
- `write_journal_field`: journal id, work/source/target identity, field name, basis observation version and snapshot generation, before/source/expected hashes, attribution version, and irreversible consumption marker.
- `conflict`: work id, field name, provider values, selected resolution, reason, resolved time.
- `manual_mapping`: manually verified external id relationships and ignored false-positive relationships.

## Normalized Media Model

Use media kind as a first-class dimension:

```text
MediaKind = Anime | Manga
Provider = Bangumi | AniList | MyAnimeList
SyncField = Status | Score | ProgressEpisodes | ProgressChapters | ProgressVolumes | RepeatCount | StartedAt | CompletedAt | Notes | Tags
```

Do not force manga into anime-style episode fields. Preserve provider-specific fields in raw payload storage and only normalize fields with a known safe mapping.

## Matching Strategy

The matcher should be identity-graph-first:

1. Direct id reuse from existing manual mappings or previously verified graph edges.
2. Strong crosswalk import where available.
   - AniList `idMal` is strong for AniList-to-MAL anime and manga.
   - `anime-offline-database` is useful for anime cross-provider ids.
   - Bangumi-to-global mappings are weaker and need confidence scoring.
3. Local indexed candidate generation with SQLite FTS over normalized titles, aliases, original titles, romanized titles, and translated titles.
4. Provider search only for unmapped records or low-confidence local candidates.
5. Metadata filters for media kind, format, release year, episode/chapter/volume counts, season, creators, studios, and relation type.
6. Confidence scoring with explicit thresholds:
   - High confidence: auto-link and record edge.
   - Medium confidence: include in review report but do not write based on it.
   - Low confidence: leave unmatched.
7. Manual review file for confirmations, ignores, and overrides.

Important matching safeguards:

- Never auto-match anime and manga across media kinds.
- Never merge sequels, recaps, side stories, specials, or adaptations by title alone.
- Prefer exact external ids over title similarity.
- Treat duplicate MAL ids, duplicate titles, and missing dates as ambiguity signals.
- Store negative matches so known false positives are not re-suggested every run.

## Sync Planning

The planner should run in phases:

1. Ingest provider snapshots into SQLite.
2. Normalize collection fields by provider and media kind.
3. Resolve each provider entry to an `identity_work`.
4. Build a field-level comparison table across the selected providers.
5. Apply field policies to produce a sync plan.
6. Emit a dry-run report by default.
7. Apply writes only when explicitly enabled.
8. Record every write attempt in `write_journal`.
9. Re-read affected entries after writes and verify the expected remote state.

Default field policies:

- Status and progress can sync bidirectionally when the source value is newer or the target is empty.
- Score can sync when configured score scales are compatible.
- Notes, comments, and tags are read-only in the first write-enabled release unless the user opts in.
- Deletes are disabled.
- Bangumi `updated_at` is not trusted as a sole conflict-resolution signal.
- AniList and MAL `updatedAt`/`updated_at` can be used, but write journal data takes precedence for changes made by this tool.

## Conflict Resolution

The conflict engine should be conservative:

- If only one provider has a value, propose filling missing targets.
- If values differ and exactly one provider changed since the last observed snapshot, propose propagating that change.
- If multiple providers changed since the last observed snapshot, create a conflict and do not write.
- If a provider timestamp is unreliable, use local snapshot hashes and write journal records before considering latest-write-wins.
- If a field is private or lossy to map, do not overwrite it automatically.

## Performance Plan

The Rust rewrite should address current bottlenecks by design:

- Persist imported datasets in SQLite instead of rebuilding maps every run.
- Use indexed lookup for external ids.
- Use FTS candidate search instead of scanning all records per match.
- Cache normalized titles and derived metadata.
- Store dataset versions and payload hashes to skip unchanged imports.
- Rate-limit provider calls centrally and batch where provider APIs support it.
- Separate ingest, match, plan, and apply stages so unchanged stages can be skipped.

## Safety and Testing

Required gates before live write support:

- Unit tests for status, progress, score, and date mapping.
- Fixture tests for anime and manga snapshots from all three providers.
- Golden-file tests for sync plans.
- Matcher tests for exact match, false positive, ambiguous title, sequel/special, and cross-media rejection cases.
- Mock HTTP tests for each provider adapter.
- Dry-run CLI output review on a real read-only snapshot.
- Throwaway-account live write tests.
- Explicit user confirmation before any write test against personal accounts.

Live testing rules:

- Real user accounts are read-only unless the user explicitly approves a specific write test.
- No automated delete tests against real accounts.
- Throwaway Bangumi, AniList, and MAL accounts should be used for end-to-end write verification.
- Browser sessions may be used for login setup only; they should not be used to mutate personal collection data.

## Migration Strategy

Migration should be incremental:

1. Keep current TypeScript sync available and untouched.
2. Add Rust v2 as a separate CLI with dry-run only.
3. Import existing `config/manual_relations.json` and `config/ignore_entries.json` into the v2 identity graph.
4. Validate v2 matching against current TypeScript output.
5. Add read-only AniList, Bangumi, and MAL snapshots for anime.
6. Add manga snapshots.
7. Enable write planning.
8. Enable writes to throwaway accounts.
9. Enable opt-in writes for real accounts only after dry-run review.

## Open Decisions for User Review

- Whether the first implementation milestone should target anime-only dry-run across all three providers, or anime plus manga read-only for Bangumi and AniList before MAL.
- Whether notes/comments/tags should remain read-only in v2.0 or become opt-in sync fields.
- Whether score synchronization should use raw provider scales or a normalized 100-point internal scale.
- Whether the existing Docker deployment should continue to run the TypeScript baseline while Rust v2 matures, or whether Docker should gain a separate v2 command early.
