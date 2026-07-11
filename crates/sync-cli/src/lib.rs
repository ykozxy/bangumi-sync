use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use sync_core::identity::{
    auto_link_provider_item_match, import_anime_offline_database, import_bangumi_dataset,
    import_legacy_ignore_entries, import_legacy_manual_relations,
    match_provider_items_with_metadata, preview_auto_link_provider_item_match, select_auto_match,
    validate_anime_offline_database_json, validate_bangumi_dataset_json,
    validate_legacy_ignore_entries_json, validate_legacy_manual_relations_json, AutoLinkReport,
    AutoMatchDecision,
};
use sync_core::model::{MediaKind, Provider, SyncField};
use sync_core::provider::{
    build_provider_authorization_request, build_provider_token_exchange_request,
    exchange_provider_oauth_token, parse_anilist_collection_fixture,
    parse_bangumi_collection_fixture, parse_bangumi_episode_collection_fixture,
    parse_myanimelist_collection_fixture, plan_provider_credential_action,
    AuthorizedProviderReadRequest, AuthorizedProviderReadTransport, AuthorizedProviderWriteRequest,
    AuthorizedProviderWriteTransport, CredentialBundleEnvelope, CredentialBundleError,
    CredentialBundleSealCodec, FileCredentialBundleSecretStore, PassphraseCredentialBundleCodec,
    ProviderAccessToken, ProviderAuthBootstrapMode, ProviderAuthError, ProviderAuthFlow,
    ProviderCollectionSnapshot, ProviderCredentialAccessTokenStore, ProviderCredentialAction,
    ProviderCredentialRefreshOutcome, ProviderCredentialSecretLookup,
    ProviderCredentialSecretStore, ProviderCredentialSecretStoreInput, ProviderCredentialState,
    ProviderHttpClient, ProviderHttpReadTransport, ProviderHttpRequest, ProviderHttpResponse,
    ProviderOAuthAuthorizationInput, ProviderOAuthEndpointSource, ProviderOAuthTokenExchangeInput,
    ProviderOAuthTokenHttpResponse, ProviderOAuthTokenRequest, ProviderOAuthTokenTransport,
    ProviderReadRequest, ProviderReadRequestMethod, ProviderRefreshTokenState,
    ProviderWriteRequestMethod,
};
use sync_core::store::SqliteStore;
use sync_core::sync::{
    apply_plan_with_writer, apply_plan_with_writer_and_deferred_verifier,
    auto_link_local_collection_entries, import_collection_snapshot_with_identity_links,
    plan_dry_run, plan_dry_run_for_media_kinds, preflight_stored_provider_credential_secret,
    refresh_snapshots_and_plan_dry_run_with_auto_refresh,
    refresh_snapshots_and_plan_dry_run_with_stored_access_tokens,
    refresh_stored_provider_credential_if_needed, ApplyError, PlannedAction, PlannedActionKind,
    ProviderSyncCredentialRefreshConfig, StoredAccessTokenProviderPostWriteVerifier,
    StoredAccessTokenProviderWriter, StoredCredentialRefreshError,
    StoredProviderCollectionImportOutcome, StoredProviderCredentialSecretPreflightOutcome,
    StoredProviderSyncCycleError, StoredProviderSyncCycleOutcome, SyncPlan,
};

const USAGE: &str = "\
sync-v2

Usage:
  sync-v2 --help
  sync-v2 auth status --db <path> --provider <provider> --account <id> [(--test-credential-bundle <path>|--credential-bundle <path> --credential-bundle-passphrase-env <env>)] [--now <unix-seconds>] [--format text|json]
  sync-v2 auth begin --provider <provider> --client-id <id> --redirect-uri <uri> --session-file <path> [--show-sensitive-authorization-url] [--now <unix-seconds>] [--format text|json]
  sync-v2 auth callback-plan --session-file <path> --callback-url <url-or-query> [--now <unix-seconds>] [--format text|json]
  sync-v2 auth complete --test-token-transport --session-file <path> --callback-url <url-or-query> --db <path> --account <id> (--test-credential-bundle <path>|--credential-bundle <path> --credential-bundle-passphrase-env <env>) --token-response <path> [--client-secret <secret> --now <unix-seconds>] [--format text|json]
  sync-v2 auth authorize-url --provider <provider> --client-id <id> --redirect-uri <uri> --state <state> [--pkce-code-challenge <value>] [--show-sensitive-authorization-url] [--format text|json]
  sync-v2 auth exchange-code --test-token-transport --db <path> --provider <provider> --account <id> --client-id <id> --redirect-uri <uri> --authorization-code <code> --test-credential-bundle <path> --token-response <path> [--client-secret <secret> --pkce-code-verifier <value> --now <unix-seconds>] [--format text|json]
  sync-v2 auth refresh --test-token-transport --db <path> --provider <provider> --account <id> --client-id <id> (--test-credential-bundle <path>|--credential-bundle <path> --credential-bundle-passphrase-env <env>) [--token-response <path>] [--client-secret <secret> --now <unix-seconds>] [--format text|json]
  sync-v2 import-fixtures --provider <provider> --kind <anime|manga> --fixture <path> --db <path> --account <id>
  sync-v2 import-legacy-config --db <path> --kind <anime|manga> [--manual-relations <path>] [--ignore-entries <path>]
  sync-v2 import-dataset --db <path> --dataset <anime-offline|bangumi-data> --file <path> [--kind <anime|manga>]
  sync-v2 plan --dry-run --db <path> --account <id> --kind <anime|manga|all> --providers <list> [--refresh-snapshots ((--fixture-response <provider>:<kind>:<path>|bangumi:anime:episodes:<subject-id>:<path>...)|--live-read) [--auto-link-local] [(--test-credential-bundle <path>|--credential-bundle <path> --credential-bundle-passphrase-env <env>)] --test-token-transport --token-response <provider>:<path>... --refresh-client-id <provider>:<id>... --refresh-client-secret <provider>:<secret>... --limit <n> --now <unix-seconds>] [--format text|json]
  sync-v2 apply --test-write-transport [--verify-post-write --fixture-response <provider>:<kind>:<path>... --limit <n>] [--auto-link-local] --db <path> --account <id> --kind <anime|manga|all> --providers <list> [(--test-credential-bundle <path>|--credential-bundle <path> --credential-bundle-passphrase-env <env>)] [--now <unix-seconds>] [--format text|json]
  sync-v2 sync --test-write-transport [--verify-post-write] [--auto-link-local] --db <path> --account <id> --kind <anime|manga|all> --providers <list> --fixture-response <provider>:<kind>:<path>|bangumi:anime:episodes:<subject-id>:<path>... [(--test-credential-bundle <path>|--credential-bundle <path> --credential-bundle-passphrase-env <env>)] [--test-token-transport --token-response <provider>:<path>... --refresh-client-id <provider>:<id>... --refresh-client-secret <provider>:<secret>... --limit <n> --now <unix-seconds>] [--format text|json]
  sync-v2 inspect-match --dry-run --db <path> --kind <anime|manga> --query <title> [--release-year <yyyy>] [--limit <n>] [--format text|json]
  sync-v2 auto-link (--dry-run|--apply-local) --db <path> --provider <provider> --kind <anime|manga> --id <provider-id> [--format text|json]

Commands:
  auth status      Inspect local credential metadata and report required auth action.
  auth begin       Create a local OAuth authorization session file without browser or network actions.
  auth callback-plan
                  Validate a pasted OAuth callback against a local auth session without token exchange.
  auth complete    Validate a local auth session callback, then install credentials through an injected test token transport.
  auth authorize-url
                  Build local-only OAuth authorization metadata without browser or network actions.
  auth exchange-code
                  Install local credential metadata through an injected test token transport.
  auth refresh     Refresh local credential metadata through an injected test token transport.
  import-fixtures  Import local fixture snapshots into the v2 model.
  import-legacy-config
                  Import legacy manual/ignore config files into the local identity graph.
  import-dataset   Import local catalog/crosswalk datasets into the SQLite match cache.
  plan             Build a dry-run sync plan. Optionally refresh local snapshots from fixture responses first.
  apply            Apply a local plan through stored credentials and a test write transport only.
  sync             Refresh snapshots with fixture read transport plus test write transport apply.
  inspect-match    Explain identity candidates for a fixture entry.
  auto-link        Resolve one local provider item into local identity edges.

Safety:
  Live provider writes, deletes, notes, tags, and network auth are disabled.
  auth begin writes a local temporary auth session file; it does not open a browser, exchange tokens, read a DB, or write credential bundles. It suppresses the sensitive authorization URL unless --show-sensitive-authorization-url is passed.
  auth callback-plan validates only local callback/session data; it does not exchange tokens, read a DB, or write credentials.
  auth complete --test-token-transport exchanges only against local fixture token responses after local session/callback validation; encrypted credential bundles require a passphrase from an environment variable.
  auth authorize-url builds local authorization metadata only; it does not open a browser, exchange tokens, or write a DB. It suppresses the sensitive authorization URL unless --show-sensitive-authorization-url is passed.
  auth exchange-code --test-token-transport exchanges only against local fixture token responses and installs only local credential metadata plus the test credential bundle.
  auth refresh --test-token-transport uses local fixture token responses when refresh is required and may update only local credential metadata plus the selected local credential bundle.
  auth status verifies only local credential bundle readability when a bundle is provided; it does not refresh tokens, start a browser, or send network requests.
  plan --refresh-snapshots uses local credential metadata plus fixture responses and may update only the local SQLite snapshot cache; --auto-link-local may update only the local SQLite identity graph and collection work links before planning; --test-token-transport may refresh local bundle credentials from fixture token responses first.
  apply --test-write-transport records provider-shaped write attempts only in the local SQLite journal; --auto-link-local may update only the local SQLite identity graph and collection work links before planning/apply; --verify-post-write consumes fixture reads for read-back verification.
  sync --test-write-transport refreshes local SQLite snapshots from fixtures, then records provider-shaped write attempts only in the local SQLite journal; --auto-link-local may update only the local SQLite identity graph and collection work links before planning/apply; --test-token-transport may refresh local bundle credentials from fixture token responses first; --verify-post-write consumes additional fixture reads for read-back verification.
  --test-credential-bundle is a local test-only credential source, not production encryption.
  auto-link --apply-local writes only the local SQLite identity graph.
";

const READ_ONLY_COMMANDS: &[&str] = &["plan", "inspect-match"];
const AUTH_SESSION_TTL_SECONDS: i64 = 600;

pub fn run<I, S, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> i32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    W: Write,
    E: Write,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect::<Vec<_>>();

    let command = args.get(1).map(String::as_str).unwrap_or("--help");

    match command {
        "--help" | "-h" => {
            let _ = stdout.write_all(USAGE.as_bytes());
            0
        }
        "auth" => run_auth(&args[2..], stdout, stderr),
        "import-fixtures" => run_import_fixtures(&args[2..], stdout, stderr),
        "import-legacy-config" => run_import_legacy_config(&args[2..], stdout, stderr),
        "import-dataset" => run_import_dataset(&args[2..], stdout, stderr),
        "plan" => run_plan(&args[2..], stdout, stderr),
        "apply" => run_apply(&args[2..], stdout, stderr),
        "sync" => run_sync(&args[2..], stdout, stderr),
        "inspect-match" => run_inspect_match(&args[2..], stdout, stderr),
        "auto-link" => run_auto_link(&args[2..], stdout, stderr),
        known if READ_ONLY_COMMANDS.contains(&known) => {
            if let Some(unexpected) = args.get(2) {
                let _ = writeln!(stderr, "unexpected argument for {known}: {unexpected}");
                let _ = stderr.write_all(USAGE.as_bytes());
                return 2;
            }

            let _ = writeln!(
                stdout,
                "sync-v2 {known}: read-only skeleton command; dry-run behavior only"
            );
            0
        }
        unknown => {
            let _ = writeln!(stderr, "unknown command: {unknown}");
            let _ = stderr.write_all(USAGE.as_bytes());
            2
        }
    }
}

#[derive(Debug, Default)]
struct ImportFixtureOptions {
    provider: Option<Provider>,
    media_kind: Option<MediaKind>,
    fixture_path: Option<PathBuf>,
    db_path: Option<PathBuf>,
    account_id: Option<String>,
}

#[derive(Debug, Default)]
struct ImportLegacyConfigOptions {
    db_path: Option<PathBuf>,
    media_kind: Option<MediaKind>,
    manual_relations_path: Option<PathBuf>,
    ignore_entries_path: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct ImportDatasetOptions {
    db_path: Option<PathBuf>,
    dataset: Option<DatasetKind>,
    file_path: Option<PathBuf>,
    media_kind: Option<MediaKind>,
}

#[derive(Debug)]
struct PlanOptions {
    dry_run: bool,
    refresh_snapshots: bool,
    live_read: bool,
    auto_link_local: bool,
    test_token_transport: bool,
    db_path: Option<PathBuf>,
    account_id: Option<String>,
    media_scope: Option<PlanMediaScope>,
    providers: Option<Vec<Provider>>,
    fixture_responses: Vec<FixtureResponseInput>,
    test_credential_bundle_path: Option<PathBuf>,
    credential_bundle_path: Option<PathBuf>,
    credential_bundle_passphrase_env: Option<String>,
    token_responses: Vec<ProviderTokenResponseInput>,
    refresh_client_ids: Vec<ProviderScopedValue>,
    refresh_client_secrets: Vec<ProviderScopedValue>,
    limit: u16,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            refresh_snapshots: false,
            live_read: false,
            auto_link_local: false,
            test_token_transport: false,
            db_path: None,
            account_id: None,
            media_scope: None,
            providers: None,
            fixture_responses: Vec::new(),
            test_credential_bundle_path: None,
            credential_bundle_path: None,
            credential_bundle_passphrase_env: None,
            token_responses: Vec::new(),
            refresh_client_ids: Vec::new(),
            refresh_client_secrets: Vec::new(),
            limit: 100,
            now_unix_seconds: None,
            output_format: PlanOutputFormat::Text,
        }
    }
}

#[derive(Debug)]
struct ApplyOptions {
    test_write_transport: bool,
    verify_post_write: bool,
    auto_link_local: bool,
    db_path: Option<PathBuf>,
    account_id: Option<String>,
    media_scope: Option<PlanMediaScope>,
    providers: Option<Vec<Provider>>,
    fixture_responses: Vec<FixtureResponseInput>,
    test_credential_bundle_path: Option<PathBuf>,
    credential_bundle_path: Option<PathBuf>,
    credential_bundle_passphrase_env: Option<String>,
    limit: u16,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            test_write_transport: false,
            verify_post_write: false,
            auto_link_local: false,
            db_path: None,
            account_id: None,
            media_scope: None,
            providers: None,
            fixture_responses: Vec::new(),
            test_credential_bundle_path: None,
            credential_bundle_path: None,
            credential_bundle_passphrase_env: None,
            limit: 100,
            now_unix_seconds: None,
            output_format: PlanOutputFormat::Text,
        }
    }
}

#[derive(Debug)]
struct SyncOptions {
    test_write_transport: bool,
    verify_post_write: bool,
    auto_link_local: bool,
    test_token_transport: bool,
    db_path: Option<PathBuf>,
    account_id: Option<String>,
    media_scope: Option<PlanMediaScope>,
    providers: Option<Vec<Provider>>,
    fixture_responses: Vec<FixtureResponseInput>,
    test_credential_bundle_path: Option<PathBuf>,
    credential_bundle_path: Option<PathBuf>,
    credential_bundle_passphrase_env: Option<String>,
    token_responses: Vec<ProviderTokenResponseInput>,
    refresh_client_ids: Vec<ProviderScopedValue>,
    refresh_client_secrets: Vec<ProviderScopedValue>,
    limit: u16,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            test_write_transport: false,
            verify_post_write: false,
            auto_link_local: false,
            test_token_transport: false,
            db_path: None,
            account_id: None,
            media_scope: None,
            providers: None,
            fixture_responses: Vec::new(),
            test_credential_bundle_path: None,
            credential_bundle_path: None,
            credential_bundle_passphrase_env: None,
            token_responses: Vec::new(),
            refresh_client_ids: Vec::new(),
            refresh_client_secrets: Vec::new(),
            limit: 100,
            now_unix_seconds: None,
            output_format: PlanOutputFormat::Text,
        }
    }
}

#[derive(Debug)]
struct FixtureResponseInput {
    provider: Provider,
    media_kind: MediaKind,
    endpoint: FixtureResponseEndpoint,
    path: PathBuf,
}

#[derive(Debug)]
enum FixtureResponseEndpoint {
    Collection,
    BangumiEpisodeCollection { subject_id: String },
}

#[derive(Debug, Default)]
struct FixtureResponseMap {
    collections: HashMap<(Provider, MediaKind), Vec<String>>,
    bangumi_episode_collections: HashMap<String, Vec<String>>,
}

#[derive(Debug)]
struct ProviderTokenResponseInput {
    provider: Provider,
    path: PathBuf,
}

#[derive(Debug)]
struct ProviderScopedValue {
    provider: Provider,
    value: String,
}

#[derive(Debug)]
struct InspectMatchOptions {
    dry_run: bool,
    db_path: Option<PathBuf>,
    media_kind: Option<MediaKind>,
    query: Option<String>,
    release_year: Option<u16>,
    limit: u32,
    output_format: PlanOutputFormat,
}

impl Default for InspectMatchOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            db_path: None,
            media_kind: None,
            query: None,
            release_year: None,
            limit: 10,
            output_format: PlanOutputFormat::Text,
        }
    }
}

#[derive(Debug, Default)]
struct AutoLinkOptions {
    dry_run: bool,
    apply_local: bool,
    db_path: Option<PathBuf>,
    provider: Option<Provider>,
    media_kind: Option<MediaKind>,
    external_id: Option<String>,
    output_format: PlanOutputFormat,
}

#[derive(Debug, Default)]
struct AuthStatusOptions {
    db_path: Option<PathBuf>,
    provider: Option<Provider>,
    account_id: Option<String>,
    test_credential_bundle_path: Option<PathBuf>,
    credential_bundle_path: Option<PathBuf>,
    credential_bundle_passphrase_env: Option<String>,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

#[derive(Default)]
struct AuthAuthorizeUrlOptions {
    provider: Option<Provider>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    pkce_code_challenge: Option<String>,
    show_sensitive_authorization_url: bool,
    output_format: PlanOutputFormat,
}

#[derive(Debug, Default)]
struct AuthBeginOptions {
    provider: Option<Provider>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    session_file_path: Option<PathBuf>,
    show_sensitive_authorization_url: bool,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

#[derive(Default)]
struct AuthCallbackPlanOptions {
    session_file_path: Option<PathBuf>,
    callback_url: Option<String>,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

#[derive(Default)]
struct AuthCompleteOptions {
    test_token_transport: bool,
    db_path: Option<PathBuf>,
    account_id: Option<String>,
    session_file_path: Option<PathBuf>,
    callback_url: Option<String>,
    client_secret: Option<String>,
    test_credential_bundle_path: Option<PathBuf>,
    credential_bundle_path: Option<PathBuf>,
    credential_bundle_passphrase_env: Option<String>,
    token_response_path: Option<PathBuf>,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

struct AuthSession {
    provider: Provider,
    client_id: String,
    redirect_uri: String,
    state: String,
    pkce_code_verifier: Option<String>,
    endpoint_source: String,
    bootstrap_mode: String,
    expires_at_epoch_secs: i64,
}

struct AuthExchangeCodeOptions {
    test_token_transport: bool,
    db_path: Option<PathBuf>,
    provider: Option<Provider>,
    account_id: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    redirect_uri: Option<String>,
    authorization_code: Option<String>,
    pkce_code_verifier: Option<String>,
    test_credential_bundle_path: Option<PathBuf>,
    token_response_path: Option<PathBuf>,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

impl Default for AuthExchangeCodeOptions {
    fn default() -> Self {
        Self {
            test_token_transport: false,
            db_path: None,
            provider: None,
            account_id: None,
            client_id: None,
            client_secret: None,
            redirect_uri: None,
            authorization_code: None,
            pkce_code_verifier: None,
            test_credential_bundle_path: None,
            token_response_path: None,
            now_unix_seconds: None,
            output_format: PlanOutputFormat::Text,
        }
    }
}

struct AuthRefreshOptions {
    test_token_transport: bool,
    db_path: Option<PathBuf>,
    provider: Option<Provider>,
    account_id: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    test_credential_bundle_path: Option<PathBuf>,
    credential_bundle_path: Option<PathBuf>,
    credential_bundle_passphrase_env: Option<String>,
    token_response_path: Option<PathBuf>,
    now_unix_seconds: Option<i64>,
    output_format: PlanOutputFormat,
}

struct AuthCredentialInstallInput {
    db_path: PathBuf,
    provider: Provider,
    account_id: String,
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: String,
    authorization_code: String,
    pkce_code_verifier: Option<String>,
    credential_store: AuthCredentialStoreConfig,
    token_response_path: PathBuf,
    now_unix_seconds: i64,
}

struct AuthCredentialInstallOutcome {
    exchange: sync_core::provider::ProviderCredentialExchangeResult,
    requests: Vec<ProviderOAuthTokenRequest>,
}

enum AuthCredentialInstallError {
    BuildRequest,
    OpenDatabase,
    Exchange,
    PersistMetadata,
}

#[derive(Clone)]
enum AuthCredentialStoreConfig {
    TestCredentialBundle(PathBuf),
    EncryptedCredentialBundle { path: PathBuf, passphrase: String },
}

enum CliCredentialSecretStore {
    TestCredentialBundle(FileCredentialBundleSecretStore<FixtureCredentialBundleCodec>),
    EncryptedCredentialBundle(FileCredentialBundleSecretStore<PassphraseCredentialBundleCodec>),
}

impl AuthCredentialStoreConfig {
    fn source_label(&self) -> &'static str {
        match self {
            Self::TestCredentialBundle(_) => "test_credential_bundle",
            Self::EncryptedCredentialBundle { .. } => "file_credential_bundle",
        }
    }

    fn codec_label(&self) -> &'static str {
        match self {
            Self::TestCredentialBundle(_) => "test-reversing",
            Self::EncryptedCredentialBundle { .. } => PassphraseCredentialBundleCodec::CODEC_LABEL,
        }
    }

    fn into_secret_store(self) -> CliCredentialSecretStore {
        match self {
            Self::TestCredentialBundle(path) => CliCredentialSecretStore::TestCredentialBundle(
                FileCredentialBundleSecretStore::new(path, FixtureCredentialBundleCodec),
            ),
            Self::EncryptedCredentialBundle { path, passphrase } => {
                CliCredentialSecretStore::EncryptedCredentialBundle(
                    FileCredentialBundleSecretStore::new(
                        path,
                        PassphraseCredentialBundleCodec::new(passphrase),
                    ),
                )
            }
        }
    }
}

impl Default for AuthRefreshOptions {
    fn default() -> Self {
        Self {
            test_token_transport: false,
            db_path: None,
            provider: None,
            account_id: None,
            client_id: None,
            client_secret: None,
            test_credential_bundle_path: None,
            credential_bundle_path: None,
            credential_bundle_passphrase_env: None,
            token_response_path: None,
            now_unix_seconds: None,
            output_format: PlanOutputFormat::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanMediaScope {
    One(MediaKind),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatasetKind {
    AnimeOffline,
    BangumiData,
}

impl DatasetKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AnimeOffline => "anime-offline",
            Self::BangumiData => "bangumi-data",
        }
    }
}

impl Default for PlanOutputFormat {
    fn default() -> Self {
        Self::Text
    }
}

fn run_inspect_match<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    if args.is_empty() {
        let _ = writeln!(
            stdout,
            "sync-v2 inspect-match: read-only skeleton command; dry-run behavior only"
        );
        return 0;
    }

    let options = match parse_inspect_match_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let db_path = options.db_path.expect("validated db path");
    let media_kind = options.media_kind.expect("validated media kind");
    let query = options.query.expect("validated query");
    let store = match SqliteStore::open_read_only(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };
    let candidates = match match_provider_items_with_metadata(
        &store,
        media_kind,
        &query,
        options.release_year,
        options.limit,
    ) {
        Ok(candidates) => candidates,
        Err(error) => {
            let _ = writeln!(stderr, "failed to inspect match candidates: {error:?}");
            return 1;
        }
    };
    let decision = select_auto_match(&candidates);

    match options.output_format {
        PlanOutputFormat::Text => {
            write_text_inspect_match(stdout, options.release_year, &candidates, &decision)
        }
        PlanOutputFormat::Json => write_json_inspect_match(
            stdout,
            media_kind,
            &query,
            options.release_year,
            options.limit,
            &candidates,
            &decision,
        ),
    }

    0
}

fn write_text_inspect_match<W>(
    stdout: &mut W,
    query_release_year: Option<u16>,
    candidates: &[sync_core::identity::MatchCandidate],
    decision: &AutoMatchDecision,
) where
    W: Write,
{
    let _ = writeln!(
        stdout,
        "sync-v2 inspect-match: dry-run candidates={} decision={} read-only",
        candidates.len(),
        auto_match_decision_name(decision)
    );
    if let Some(year) = query_release_year {
        let _ = writeln!(stdout, "query_release_year={year}");
    }
    if let AutoMatchDecision::NeedsReview { reason } = decision {
        let _ = writeln!(stdout, "review_reason={reason}");
    }
    for candidate in candidates {
        let _ = writeln!(
            stdout,
            "- {} {} {} confidence={} method={} title={}",
            candidate.provider.as_str(),
            candidate.media_kind.as_str(),
            candidate.external_id,
            candidate.confidence,
            candidate.match_method,
            candidate.canonical_title
        );
        if let Some(year) = candidate.release_year {
            let _ = writeln!(stdout, "  release_year={year}");
        }
    }
}

fn write_json_inspect_match<W>(
    stdout: &mut W,
    media_kind: MediaKind,
    query: &str,
    query_release_year: Option<u16>,
    limit: u32,
    candidates: &[sync_core::identity::MatchCandidate],
    decision: &AutoMatchDecision,
) where
    W: Write,
{
    let candidates = candidates
        .iter()
        .map(|candidate| {
            json!({
                "provider": candidate.provider.as_str(),
                "media_kind": candidate.media_kind.as_str(),
                "external_id": candidate.external_id.as_str(),
                "canonical_title": candidate.canonical_title.as_str(),
                "release_year": candidate.release_year,
                "confidence": candidate.confidence,
                "match_method": candidate.match_method.as_str(),
            })
        })
        .collect::<Vec<_>>();
    let report = json!({
        "dry_run": true,
        "read_only": true,
        "query": query,
        "query_release_year": query_release_year,
        "media_kind": media_kind.as_str(),
        "limit": limit,
        "decision": auto_match_decision_json(decision),
        "candidates": candidates,
    });

    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn run_auto_link<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_auto_link_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let db_path = options.db_path.expect("validated db path");
    let provider = options.provider.expect("validated provider");
    let media_kind = options.media_kind.expect("validated media kind");
    let external_id = options.external_id.expect("validated external id");
    let report = if options.apply_local {
        let store = match SqliteStore::open(&db_path) {
            Ok(store) => store,
            Err(error) => {
                let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
                return 1;
            }
        };
        match auto_link_provider_item_match(&store, provider, media_kind, &external_id) {
            Ok(report) => report,
            Err(error) => {
                let _ = writeln!(stderr, "failed to auto-link provider item: {error:?}");
                return 1;
            }
        }
    } else {
        let store = match SqliteStore::open_read_only(&db_path) {
            Ok(store) => store,
            Err(error) => {
                let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
                return 1;
            }
        };
        match preview_auto_link_provider_item_match(&store, provider, media_kind, &external_id) {
            Ok(report) => report,
            Err(error) => {
                let _ = writeln!(
                    stderr,
                    "failed to preview auto-link provider item: {error:?}"
                );
                return 1;
            }
        }
    };

    match options.output_format {
        PlanOutputFormat::Text => write_text_auto_link(
            stdout,
            options.dry_run,
            options.apply_local,
            provider,
            media_kind,
            &external_id,
            &report,
        ),
        PlanOutputFormat::Json => write_json_auto_link(
            stdout,
            options.dry_run,
            options.apply_local,
            provider,
            media_kind,
            &external_id,
            &report,
        ),
    }

    0
}

fn write_text_auto_link<W>(
    stdout: &mut W,
    dry_run: bool,
    apply_local: bool,
    provider: Provider,
    media_kind: MediaKind,
    external_id: &str,
    report: &AutoLinkReport,
) where
    W: Write,
{
    let _ = writeln!(
        stdout,
        "sync-v2 auto-link: provider={} kind={} id={} dry_run={} apply_local={} local-only linked_edges={} work_id={}",
        provider.as_str(),
        media_kind.as_str(),
        external_id,
        dry_run,
        apply_local,
        report.linked_edges.len(),
        report
            .work_id
            .map(|work_id| work_id.to_string())
            .unwrap_or_else(|| "<new>".to_owned())
    );
    if let Some(reason) = &report.review_reason {
        let _ = writeln!(stdout, "review_reason={reason}");
    }
    for edge in &report.linked_edges {
        let _ = writeln!(
            stdout,
            "- {} {} {} confidence={} method={}",
            edge.provider.as_str(),
            edge.media_kind.as_str(),
            edge.external_id,
            edge.confidence,
            edge.match_method
        );
    }
}

fn write_json_auto_link<W>(
    stdout: &mut W,
    dry_run: bool,
    apply_local: bool,
    provider: Provider,
    media_kind: MediaKind,
    external_id: &str,
    report: &AutoLinkReport,
) where
    W: Write,
{
    let edges = report
        .linked_edges
        .iter()
        .map(|edge| {
            json!({
                "provider": edge.provider.as_str(),
                "media_kind": edge.media_kind.as_str(),
                "external_id": edge.external_id.as_str(),
                "confidence": edge.confidence,
                "match_method": edge.match_method.as_str(),
            })
        })
        .collect::<Vec<_>>();
    let body = json!({
        "dry_run": dry_run,
        "read_only": dry_run,
        "local_only": true,
        "apply_local": apply_local,
        "provider": provider.as_str(),
        "media_kind": media_kind.as_str(),
        "external_id": external_id,
        "work_id": report.work_id,
        "review_reason": report.review_reason.as_deref(),
        "linked_edges": edges,
    });

    let _ = serde_json::to_writer_pretty(&mut *stdout, &body);
    let _ = writeln!(stdout);
}

fn run_import_dataset<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_import_dataset_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let dataset = options.dataset.expect("validated dataset");
    let file_path = options.file_path.expect("validated file path");
    let dataset_json = match read_local_file(&file_path, stderr) {
        Ok(json) => json,
        Err(()) => return 1,
    };
    let validation = match dataset {
        DatasetKind::AnimeOffline => validate_anime_offline_database_json(&dataset_json),
        DatasetKind::BangumiData => validate_bangumi_dataset_json(&dataset_json),
    };
    if let Err(error) = validation {
        let _ = writeln!(stderr, "invalid dataset: {error:?}");
        return 1;
    }

    let db_path = options.db_path.expect("validated db path");
    let store = match SqliteStore::open(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };

    let imported = match dataset {
        DatasetKind::AnimeOffline => import_anime_offline_database(&store, &dataset_json),
        DatasetKind::BangumiData => import_bangumi_dataset(
            &store,
            options.media_kind.expect("validated media kind"),
            &dataset_json,
        ),
    };
    let imported = match imported {
        Ok(imported) => imported,
        Err(error) => {
            let _ = writeln!(stderr, "failed to import dataset: {error:?}");
            return 1;
        }
    };

    let media_kind_suffix = match (dataset, options.media_kind) {
        (DatasetKind::BangumiData, Some(media_kind)) => {
            format!(" media_kind={}", media_kind.as_str())
        }
        _ => String::new(),
    };
    let _ = writeln!(
        stdout,
        "sync-v2 import-dataset: dataset={}{} imported={} into {} (local-only match cache import)",
        dataset.as_str(),
        media_kind_suffix,
        imported,
        db_path.display()
    );
    0
}

fn run_import_legacy_config<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_import_legacy_config_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let manual_relations_json = if let Some(manual_relations_path) = options.manual_relations_path {
        let manual_json = match read_local_file(&manual_relations_path, stderr) {
            Ok(json) => json,
            Err(()) => return 1,
        };
        if let Err(error) = validate_legacy_manual_relations_json(&manual_json) {
            let _ = writeln!(stderr, "invalid manual relations: {error:?}");
            return 1;
        }
        Some(manual_json)
    } else {
        None
    };
    let ignore_entries_json = if let Some(ignore_entries_path) = options.ignore_entries_path {
        let ignore_json = match read_local_file(&ignore_entries_path, stderr) {
            Ok(json) => json,
            Err(()) => return 1,
        };
        if let Err(error) = validate_legacy_ignore_entries_json(&ignore_json) {
            let _ = writeln!(stderr, "invalid ignore entries: {error:?}");
            return 1;
        }
        Some(ignore_json)
    } else {
        None
    };

    let db_path = options.db_path.expect("validated db path");
    let media_kind = options.media_kind.expect("validated media kind");
    let store = match SqliteStore::open(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };

    let mut manual_relations_count = 0;
    if let Some(manual_json) = manual_relations_json {
        manual_relations_count =
            match import_legacy_manual_relations(&store, media_kind, &manual_json) {
                Ok(imported) => imported,
                Err(error) => {
                    let _ = writeln!(stderr, "failed to import manual relations: {error:?}");
                    return 1;
                }
            };
    }

    let mut ignore_entries_count = 0;
    if let Some(ignore_json) = ignore_entries_json {
        ignore_entries_count = match import_legacy_ignore_entries(&store, media_kind, &ignore_json)
        {
            Ok(imported) => imported,
            Err(error) => {
                let _ = writeln!(stderr, "failed to import ignore entries: {error:?}");
                return 1;
            }
        };
    }

    let _ = writeln!(
        stdout,
        "sync-v2 import-legacy-config: manual_relations={manual_relations_count} ignore_entries={ignore_entries_count} into {} (local-only identity import)",
        db_path.display()
    );
    0
}

fn read_local_file<E>(path: &PathBuf, stderr: &mut E) -> Result<String, ()>
where
    E: Write,
{
    match std::fs::read_to_string(path) {
        Ok(json) => Ok(json),
        Err(error) => {
            let _ = writeln!(stderr, "failed to read {}: {error}", path.display());
            Err(())
        }
    }
}

fn run_import_fixtures<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_import_fixture_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let provider = options.provider.expect("validated provider");
    let media_kind = options.media_kind.expect("validated media kind");
    let fixture_path = options.fixture_path.expect("validated fixture path");
    let db_path = options.db_path.expect("validated db path");
    let account_id = options.account_id.expect("validated account id");

    let fixture_json = match std::fs::read_to_string(&fixture_path) {
        Ok(json) => json,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to read fixture {}: {error}",
                fixture_path.display()
            );
            return 1;
        }
    };
    let snapshot = match parse_fixture(provider, media_kind, &fixture_json) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = writeln!(stderr, "failed to parse fixture: {error:?}");
            return 1;
        }
    };
    let store = match SqliteStore::open(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };
    let imported =
        match import_collection_snapshot_with_identity_links(&store, &account_id, &snapshot) {
            Ok(imported) => imported,
            Err(error) => {
                let _ = writeln!(stderr, "failed to import snapshot: {error:?}");
                return 1;
            }
        };

    let _ = writeln!(
        stdout,
        "sync-v2 import-fixtures: imported {imported} collection entries into {} (read-only provider fixture import)",
        db_path.display()
    );
    0
}

fn run_auth<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    match args.first().map(String::as_str) {
        Some("status") => run_auth_status(&args[1..], stdout, stderr),
        Some("begin") => run_auth_begin(&args[1..], stdout, stderr),
        Some("callback-plan") => run_auth_callback_plan(&args[1..], stdout, stderr),
        Some("complete") => run_auth_complete(&args[1..], stdout, stderr),
        Some("authorize-url") => run_auth_authorize_url(&args[1..], stdout, stderr),
        Some("exchange-code") => run_auth_exchange_code(&args[1..], stdout, stderr),
        Some("refresh") => run_auth_refresh(&args[1..], stdout, stderr),
        Some(other) => {
            let _ = writeln!(stderr, "unknown auth subcommand: {other}");
            let _ = stderr.write_all(USAGE.as_bytes());
            2
        }
        None => {
            let _ = writeln!(stderr, "missing auth subcommand");
            let _ = stderr.write_all(USAGE.as_bytes());
            2
        }
    }
}

fn run_auth_status<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_auth_status_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };
    let credential_store_config = match auth_optional_credential_store_config(
        options.test_credential_bundle_path.clone(),
        options.credential_bundle_path.clone(),
        options.credential_bundle_passphrase_env.as_deref(),
    ) {
        Ok(config) => config,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };
    let credential_source_label = credential_store_config
        .as_ref()
        .map(AuthCredentialStoreConfig::source_label);
    let credential_bundle_codec_label = credential_store_config
        .as_ref()
        .map(AuthCredentialStoreConfig::codec_label);

    let db_path = options.db_path.expect("validated db path");
    let provider = options.provider.expect("validated provider");
    let account_id = options.account_id.expect("validated account id");
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);

    let store = match SqliteStore::open_read_only(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };
    let (credential_state, credential_presence) =
        match store.provider_credential_state(provider, &account_id) {
            Ok(Some(credential_state)) => (credential_state, "present"),
            Ok(None) => (ProviderCredentialState::missing(provider), "missing"),
            Err(error) => {
                let _ = writeln!(stderr, "failed to inspect credential metadata: {error:?}");
                return 1;
            }
        };
    let action = plan_provider_credential_action(credential_state, now_unix_seconds);
    let secret_preflight = if let Some(config) = credential_store_config {
        let mut secret_store = config.into_secret_store();
        match preflight_stored_provider_credential_secret(
            &store,
            provider,
            &account_id,
            now_unix_seconds,
            &mut secret_store,
        ) {
            Ok(outcome) => Some(outcome),
            Err(error) => {
                let _ = writeln!(
                    stderr,
                    "failed to preflight credential secret: {}",
                    credential_refresh_error_kind(&error)
                );
                return 1;
            }
        }
    } else {
        None
    };

    match options.output_format {
        PlanOutputFormat::Text => write_text_auth_status(
            stdout,
            provider,
            &account_id,
            action,
            credential_presence,
            credential_source_label,
            credential_bundle_codec_label,
            secret_preflight.as_ref(),
        ),
        PlanOutputFormat::Json => write_json_auth_status(
            stdout,
            provider,
            &account_id,
            action,
            credential_presence,
            credential_source_label,
            credential_bundle_codec_label,
            secret_preflight.as_ref(),
        ),
    }
    0
}

fn run_auth_begin<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_auth_begin_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let provider = options.provider.expect("validated provider");
    let client_id = options.client_id.expect("validated client id");
    let redirect_uri = options.redirect_uri.expect("validated redirect uri");
    let session_file_path = options.session_file_path.expect("validated session file");
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);
    let expires_at_epoch_secs = now_unix_seconds + AUTH_SESSION_TTL_SECONDS;
    let state = match generate_oauth_urlsafe_token(24) {
        Ok(state) => state,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            return 1;
        }
    };
    let pkce_code_verifier = if provider == Provider::MyAnimeList {
        match generate_oauth_urlsafe_token(48) {
            Ok(verifier) => Some(verifier),
            Err(message) => {
                let _ = writeln!(stderr, "{message}");
                return 1;
            }
        }
    } else {
        None
    };

    let request = match build_provider_authorization_request(ProviderOAuthAuthorizationInput {
        provider,
        client_id: client_id.clone(),
        redirect_uri: redirect_uri.clone(),
        state: state.clone(),
        pkce_code_challenge: pkce_code_verifier.clone(),
    }) {
        Ok(request) => request,
        Err(error) => {
            let _ = writeln!(stderr, "failed to build auth session: {error:?}");
            return 2;
        }
    };

    let session = auth_begin_session_json(
        provider,
        &client_id,
        &redirect_uri,
        &state,
        pkce_code_verifier.as_deref(),
        request.endpoint_source,
        now_unix_seconds,
        expires_at_epoch_secs,
    );
    if write_auth_session_file(&session_file_path, &session).is_err() {
        let _ = writeln!(stderr, "failed to write auth session");
        return 1;
    }

    match options.output_format {
        PlanOutputFormat::Text => write_text_auth_begin(
            stdout,
            provider,
            options
                .show_sensitive_authorization_url
                .then_some(request.url.as_str()),
            request.endpoint_source,
            now_unix_seconds,
            expires_at_epoch_secs,
        ),
        PlanOutputFormat::Json => write_json_auth_begin(
            stdout,
            provider,
            options
                .show_sensitive_authorization_url
                .then_some(request.url.as_str()),
            request.endpoint_source,
            now_unix_seconds,
            expires_at_epoch_secs,
        ),
    }
    0
}

fn run_auth_callback_plan<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_auth_callback_plan_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let session_file_path = options.session_file_path.expect("validated session file");
    let callback_url = options.callback_url.expect("validated callback url");
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);
    let session = match read_auth_session_file(&session_file_path) {
        Ok(session) => session,
        Err(reason) => {
            let _ = writeln!(stderr, "failed to validate auth callback: {reason}");
            return 1;
        }
    };
    let callback = match parse_callback_parameters(&callback_url) {
        Ok(callback) => callback,
        Err(reason) => {
            let _ = writeln!(stderr, "failed to validate auth callback: {reason}");
            return 1;
        }
    };
    let code_present = match validate_auth_callback(&session, &callback, now_unix_seconds) {
        Ok(code_present) => code_present,
        Err(reason) => {
            let _ = writeln!(stderr, "failed to validate auth callback: {reason}");
            return 1;
        }
    };

    match options.output_format {
        PlanOutputFormat::Text => write_text_auth_callback_plan(stdout, &session, code_present),
        PlanOutputFormat::Json => write_json_auth_callback_plan(stdout, &session, code_present),
    }
    0
}

fn run_auth_complete<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_auth_complete_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };
    let credential_store = match auth_required_credential_store_config(
        options.test_credential_bundle_path.clone(),
        options.credential_bundle_path.clone(),
        options.credential_bundle_passphrase_env.as_deref(),
    ) {
        Ok(config) => config,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };
    let credential_source_label = credential_store.source_label();
    let credential_bundle_codec_label = credential_store.codec_label();

    let session_file_path = options.session_file_path.expect("validated session file");
    let callback_url = options.callback_url.expect("validated callback url");
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);
    let session = match read_auth_session_file(&session_file_path) {
        Ok(session) => session,
        Err(reason) => {
            let _ = writeln!(stderr, "failed to validate auth callback: {reason}");
            return 1;
        }
    };
    let callback = match parse_callback_parameters(&callback_url) {
        Ok(callback) => callback,
        Err(reason) => {
            let _ = writeln!(stderr, "failed to validate auth callback: {reason}");
            return 1;
        }
    };
    if let Err(reason) = validate_auth_callback(&session, &callback, now_unix_seconds) {
        let _ = writeln!(stderr, "failed to validate auth callback: {reason}");
        return 1;
    }
    let authorization_code = callback
        .get("code")
        .expect("validated authorization code")
        .to_owned();

    let account_id = options.account_id.expect("validated account id");
    let install =
        match install_test_transport_authorization_code_credential(AuthCredentialInstallInput {
            db_path: options.db_path.expect("validated db path"),
            provider: session.provider,
            account_id: account_id.clone(),
            client_id: session.client_id,
            client_secret: options.client_secret,
            redirect_uri: session.redirect_uri,
            authorization_code,
            pkce_code_verifier: session.pkce_code_verifier,
            credential_store,
            token_response_path: options
                .token_response_path
                .expect("validated token response path"),
            now_unix_seconds,
        }) {
            Ok(install) => install,
            Err(AuthCredentialInstallError::BuildRequest) => {
                let _ = writeln!(stderr, "failed to build token exchange request: auth_error");
                return 2;
            }
            Err(AuthCredentialInstallError::OpenDatabase) => {
                let _ = writeln!(stderr, "failed to open db: store_error");
                return 1;
            }
            Err(AuthCredentialInstallError::Exchange) => {
                let _ = writeln!(stderr, "failed to exchange authorization code: auth_error");
                return 1;
            }
            Err(AuthCredentialInstallError::PersistMetadata) => {
                let _ = writeln!(stderr, "failed to persist credential metadata: store_error");
                return 1;
            }
        };

    match options.output_format {
        PlanOutputFormat::Text => write_text_auth_complete(
            stdout,
            session.provider,
            &account_id,
            &install.exchange,
            &install.requests,
            credential_source_label,
            credential_bundle_codec_label,
        ),
        PlanOutputFormat::Json => write_json_auth_complete(
            stdout,
            session.provider,
            &account_id,
            &install.exchange,
            &install.requests,
            credential_source_label,
            credential_bundle_codec_label,
        ),
    }

    0
}

fn write_text_auth_callback_plan<W>(
    stdout: &mut W,
    session: &AuthSession,
    authorization_code_present: bool,
) where
    W: Write,
{
    let _ = writeln!(
        stdout,
        "sync-v2 auth callback-plan: provider={} local-only opens_browser=false network=false exchanges_token=false writes_database=false state_valid=true authorization_code_present={} pkce_required={} pkce_available={} ready_for_exchange=true endpoint_source={} bootstrap_mode={}",
        session.provider.as_str(),
        authorization_code_present,
        session.provider == Provider::MyAnimeList,
        session.pkce_code_verifier.is_some(),
        session.endpoint_source,
        session.bootstrap_mode
    );
}

fn write_json_auth_callback_plan<W>(
    stdout: &mut W,
    session: &AuthSession,
    authorization_code_present: bool,
) where
    W: Write,
{
    let report = json!({
        "provider": session.provider.as_str(),
        "local_only": true,
        "opens_browser": false,
        "network": false,
        "exchanges_token": false,
        "writes_database": false,
        "state_valid": true,
        "authorization_code_present": authorization_code_present,
        "pkce_required": session.provider == Provider::MyAnimeList,
        "pkce_available": session.pkce_code_verifier.is_some(),
        "ready_for_exchange": true,
        "endpoint_source": session.endpoint_source,
        "bootstrap_mode": session.bootstrap_mode,
        "expires_at_epoch_secs": session.expires_at_epoch_secs,
    });
    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn write_text_auth_begin<W>(
    stdout: &mut W,
    provider: Provider,
    authorization_url: Option<&str>,
    endpoint_source: ProviderOAuthEndpointSource,
    created_at_epoch_secs: i64,
    expires_at_epoch_secs: i64,
) where
    W: Write,
{
    let shown_url = authorization_url
        .map(|url| format!(" authorization_url={url}"))
        .unwrap_or_default();
    let _ = writeln!(
        stdout,
        "sync-v2 auth begin: provider={} local-only opens_browser=false network=false exchanges_token=false writes_database=false session_file_written=true endpoint_source={} bootstrap_mode=auth_broker sensitive_url=true authorization_url_suppressed={} created_at={} expires_at={}{}",
        provider.as_str(),
        oauth_endpoint_source_name(endpoint_source),
        authorization_url.is_none(),
        created_at_epoch_secs,
        expires_at_epoch_secs,
        shown_url
    );
}

fn write_json_auth_begin<W>(
    stdout: &mut W,
    provider: Provider,
    authorization_url: Option<&str>,
    endpoint_source: ProviderOAuthEndpointSource,
    created_at_epoch_secs: i64,
    expires_at_epoch_secs: i64,
) where
    W: Write,
{
    let mut report = json!({
        "provider": provider.as_str(),
        "local_only": true,
        "opens_browser": false,
        "network": false,
        "exchanges_token": false,
        "writes_database": false,
        "session_file_written": true,
        "endpoint_source": oauth_endpoint_source_name(endpoint_source),
        "bootstrap_mode": "auth_broker",
        "sensitive_url": true,
        "authorization_url_suppressed": authorization_url.is_none(),
        "created_at_epoch_secs": created_at_epoch_secs,
        "expires_at_epoch_secs": expires_at_epoch_secs,
    });
    if let Some(authorization_url) = authorization_url {
        report["authorization_url"] = json!(authorization_url);
    }
    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

#[allow(clippy::too_many_arguments)]
fn write_text_auth_status<W>(
    stdout: &mut W,
    provider: Provider,
    account_id: &str,
    action: ProviderCredentialAction,
    credential_presence: &str,
    credential_source_label: Option<&str>,
    credential_bundle_codec_label: Option<&str>,
    secret_preflight: Option<&StoredProviderCredentialSecretPreflightOutcome>,
) where
    W: Write,
{
    let credential_source = credential_source_label
        .map(|label| format!(" credential_source={label}"))
        .unwrap_or_default();
    let credential_bundle_codec = credential_bundle_codec_label
        .map(|label| format!(" credential_bundle_codec={label}"))
        .unwrap_or_default();
    let secret_preflight = secret_preflight
        .map(|outcome| {
            format!(
                " secret_preflight={}",
                secret_preflight_outcome_name(outcome)
            )
        })
        .unwrap_or_default();
    let _ = writeln!(
        stdout,
        "sync-v2 auth status: provider={} account={} action={}{} credential={} read-only local-only opens_browser=false network=false metadata_updated=false token_request_count=0{}{}{}",
        provider.as_str(),
        account_id,
        credential_action_name(action),
        credential_action_mode_suffix(action),
        credential_presence,
        credential_source,
        credential_bundle_codec,
        secret_preflight
    );
}

#[allow(clippy::too_many_arguments)]
fn write_json_auth_status<W>(
    stdout: &mut W,
    provider: Provider,
    account_id: &str,
    action: ProviderCredentialAction,
    credential_presence: &str,
    credential_source_label: Option<&str>,
    credential_bundle_codec_label: Option<&str>,
    secret_preflight: Option<&StoredProviderCredentialSecretPreflightOutcome>,
) where
    W: Write,
{
    let mut report = json!({
        "provider": provider.as_str(),
        "account": account_id,
        "action": credential_action_name(action),
        "credential": credential_presence,
        "read_only": true,
        "local_only": true,
        "opens_browser": false,
        "network": false,
        "metadata_updated": false,
        "token_request_count": 0,
    });
    if let ProviderCredentialAction::Reauthorize { preferred_mode } = action {
        report["mode"] = json!(auth_bootstrap_mode_name(preferred_mode));
    }
    if let Some(credential_source_label) = credential_source_label {
        report["credential_source"] = json!(credential_source_label);
    }
    if let Some(credential_bundle_codec_label) = credential_bundle_codec_label {
        report["credential_bundle_codec"] = json!(credential_bundle_codec_label);
    }
    if let Some(secret_preflight) = secret_preflight {
        report["secret_preflight"] = json!(secret_preflight_outcome_name(secret_preflight));
    }

    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

#[allow(clippy::too_many_arguments)]
fn auth_begin_session_json(
    provider: Provider,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    pkce_code_verifier: Option<&str>,
    endpoint_source: ProviderOAuthEndpointSource,
    created_at_epoch_secs: i64,
    expires_at_epoch_secs: i64,
) -> serde_json::Value {
    let mut session = json!({
        "format_version": 1,
        "provider": provider.as_str(),
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "state": state,
        "endpoint_source": oauth_endpoint_source_name(endpoint_source),
        "bootstrap_mode": "auth_broker",
        "created_at_epoch_secs": created_at_epoch_secs,
        "expires_at_epoch_secs": expires_at_epoch_secs,
    });
    if let Some(pkce_code_verifier) = pkce_code_verifier {
        session["pkce_code_verifier"] = json!(pkce_code_verifier);
    }
    session
}

fn write_auth_session_file(
    session_file_path: &PathBuf,
    session: &serde_json::Value,
) -> Result<(), ()> {
    let session_json = serde_json::to_string_pretty(session).map_err(|_| ())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(session_file_path).map_err(|_| ())?;
    file.write_all(session_json.as_bytes()).map_err(|_| ())?;
    file.write_all(b"\n").map_err(|_| ())
}

fn read_auth_session_file(session_file_path: &PathBuf) -> Result<AuthSession, &'static str> {
    let session_json =
        std::fs::read_to_string(session_file_path).map_err(|_| "session_unreadable")?;
    let value =
        serde_json::from_str::<serde_json::Value>(&session_json).map_err(|_| "session_invalid")?;
    if value_i64(&value, "format_version")? != 1 {
        return Err("session_invalid");
    }
    let provider = parse_provider(value_str(&value, "provider")?).map_err(|_| "session_invalid")?;
    let client_id = value_str(&value, "client_id")?.to_owned();
    if !is_nonempty_without_control(&client_id) {
        return Err("session_invalid");
    }
    let redirect_uri = value_str(&value, "redirect_uri")?.to_owned();
    if !is_nonempty_without_control(&redirect_uri) {
        return Err("session_invalid");
    }
    let state = value_str(&value, "state")?.to_owned();
    if !is_nonempty_without_control(&state) {
        return Err("session_invalid");
    }
    let pkce_code_verifier = optional_value_str(&value, "pkce_code_verifier")?.map(str::to_owned);
    if provider == Provider::MyAnimeList {
        let Some(pkce_code_verifier) = pkce_code_verifier.as_deref() else {
            return Err("session_invalid");
        };
        if !is_valid_pkce_plain_value(pkce_code_verifier) {
            return Err("session_invalid");
        }
    } else if pkce_code_verifier.is_some() {
        return Err("session_invalid");
    }
    let endpoint_source =
        validate_auth_session_endpoint_source(value_str(&value, "endpoint_source")?)?;
    let bootstrap_mode =
        validate_auth_session_bootstrap_mode(value_str(&value, "bootstrap_mode")?)?;
    let expires_at_epoch_secs = value_i64(&value, "expires_at_epoch_secs")?;
    Ok(AuthSession {
        provider,
        client_id,
        redirect_uri,
        state,
        pkce_code_verifier,
        endpoint_source,
        bootstrap_mode,
        expires_at_epoch_secs,
    })
}

fn value_str<'a>(value: &'a serde_json::Value, field: &str) -> Result<&'a str, &'static str> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or("session_invalid")
}

fn optional_value_str<'a>(
    value: &'a serde_json::Value,
    field: &str,
) -> Result<Option<&'a str>, &'static str> {
    match value.get(field) {
        Some(value) => value.as_str().map(Some).ok_or("session_invalid"),
        None => Ok(None),
    }
}

fn value_i64(value: &serde_json::Value, field: &str) -> Result<i64, &'static str> {
    value
        .get(field)
        .and_then(serde_json::Value::as_i64)
        .ok_or("session_invalid")
}

fn validate_auth_session_endpoint_source(value: &str) -> Result<String, &'static str> {
    match value {
        "official_docs" | "legacy_repo_code" => Ok(value.to_owned()),
        _ => Err("session_invalid"),
    }
}

fn validate_auth_session_bootstrap_mode(value: &str) -> Result<String, &'static str> {
    match value {
        "auth_broker" | "local_callback" => Ok(value.to_owned()),
        _ => Err("session_invalid"),
    }
}

fn is_nonempty_without_control(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

fn is_valid_pkce_plain_value(value: &str) -> bool {
    is_nonempty_without_control(value)
        && (43..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
}

fn parse_callback_parameters(input: &str) -> Result<HashMap<String, String>, &'static str> {
    let query = callback_query(input);
    let mut parameters = HashMap::new();
    for part in query.split('&').filter(|part| !part.is_empty()) {
        let (name, value) = part.split_once('=').unwrap_or((part, ""));
        let name = percent_decode_form_component(name)?;
        let value = percent_decode_form_component(value)?;
        if parameters.insert(name, value).is_some() {
            return Err("callback_query_invalid");
        }
    }
    Ok(parameters)
}

fn callback_query(input: &str) -> &str {
    let without_fragment = input
        .split_once('#')
        .map(|(prefix, _)| prefix)
        .unwrap_or(input);
    let query = without_fragment
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or(without_fragment);
    query.strip_prefix('?').unwrap_or(query)
}

fn percent_decode_form_component(input: &str) -> Result<String, &'static str> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err("callback_query_invalid");
                }
                let high = hex_value(bytes[index + 1]).ok_or("callback_query_invalid")?;
                let low = hex_value(bytes[index + 2]).ok_or("callback_query_invalid")?;
                output.push((high << 4) | low);
                index += 3;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).map_err(|_| "callback_query_invalid")
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn validate_auth_callback(
    session: &AuthSession,
    callback: &HashMap<String, String>,
    now_unix_seconds: i64,
) -> Result<bool, &'static str> {
    if now_unix_seconds >= session.expires_at_epoch_secs {
        return Err("session_expired");
    }
    let callback_state = callback.get("state").ok_or("state_missing")?;
    if callback_state != &session.state {
        return Err("state_mismatch");
    }
    if callback.contains_key("error") {
        return Err("provider_error");
    }
    let code = callback.get("code").ok_or("authorization_code_missing")?;
    if code.trim().is_empty() || code.chars().any(char::is_control) {
        return Err("authorization_code_invalid");
    }
    if session.provider == Provider::MyAnimeList && session.pkce_code_verifier.is_none() {
        return Err("pkce_missing");
    }
    Ok(true)
}

fn run_auth_authorize_url<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_auth_authorize_url_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let provider = options.provider.expect("validated provider");
    let request = match build_provider_authorization_request(ProviderOAuthAuthorizationInput {
        provider,
        client_id: options.client_id.expect("validated client id"),
        redirect_uri: options.redirect_uri.expect("validated redirect uri"),
        state: options.state.expect("validated state"),
        pkce_code_challenge: options.pkce_code_challenge,
    }) {
        Ok(request) => request,
        Err(error) => {
            let _ = writeln!(stderr, "failed to build authorization url: {error:?}");
            return 2;
        }
    };

    match options.output_format {
        PlanOutputFormat::Text => {
            let shown_url = options
                .show_sensitive_authorization_url
                .then(|| format!(" authorization_url={}", request.url))
                .unwrap_or_default();
            let _ = writeln!(
                stdout,
                "sync-v2 auth authorize-url: provider={} endpoint_source={} local-only opens_browser=false exchanges_token=false sensitive_url={} authorization_url_suppressed={}{}",
                provider.as_str(),
                oauth_endpoint_source_name(request.endpoint_source),
                request.sensitive_url,
                !options.show_sensitive_authorization_url,
                shown_url
            );
        }
        PlanOutputFormat::Json => {
            let mut payload = json!({
                "provider": provider.as_str(),
                "local_only": true,
                "opens_browser": false,
                "exchanges_token": false,
                "endpoint_source": oauth_endpoint_source_name(request.endpoint_source),
                "sensitive_url": request.sensitive_url,
                "authorization_url_suppressed": !options.show_sensitive_authorization_url,
            });
            if options.show_sensitive_authorization_url {
                payload["authorization_url"] = json!(request.url);
            }
            let _ = serde_json::to_writer_pretty(&mut *stdout, &payload);
            let _ = writeln!(stdout);
        }
    }

    0
}

fn run_auth_exchange_code<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_auth_exchange_code_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let provider = options.provider.expect("validated provider");
    let account_id = options.account_id.expect("validated account id");
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);

    let install =
        match install_test_transport_authorization_code_credential(AuthCredentialInstallInput {
            db_path: options.db_path.expect("validated db path"),
            provider,
            account_id: account_id.clone(),
            client_id: options.client_id.expect("validated client id"),
            client_secret: options.client_secret,
            redirect_uri: options.redirect_uri.expect("validated redirect uri"),
            authorization_code: options
                .authorization_code
                .expect("validated authorization code"),
            pkce_code_verifier: options.pkce_code_verifier,
            credential_store: AuthCredentialStoreConfig::TestCredentialBundle(
                options
                    .test_credential_bundle_path
                    .expect("validated credential bundle path"),
            ),
            token_response_path: options
                .token_response_path
                .expect("validated token response path"),
            now_unix_seconds,
        }) {
            Ok(install) => install,
            Err(AuthCredentialInstallError::BuildRequest) => {
                let _ = writeln!(stderr, "failed to build token exchange request: auth_error");
                return 2;
            }
            Err(AuthCredentialInstallError::OpenDatabase) => {
                let _ = writeln!(stderr, "failed to open db: store_error");
                return 1;
            }
            Err(AuthCredentialInstallError::Exchange) => {
                let _ = writeln!(stderr, "failed to exchange authorization code: auth_error");
                return 1;
            }
            Err(AuthCredentialInstallError::PersistMetadata) => {
                let _ = writeln!(stderr, "failed to persist credential metadata: store_error");
                return 1;
            }
        };

    match options.output_format {
        PlanOutputFormat::Text => write_text_auth_exchange_code(
            stdout,
            provider,
            &account_id,
            &install.exchange,
            &install.requests,
        ),
        PlanOutputFormat::Json => write_json_auth_exchange_code(
            stdout,
            provider,
            &account_id,
            &install.exchange,
            &install.requests,
        ),
    }

    0
}

fn install_test_transport_authorization_code_credential(
    input: AuthCredentialInstallInput,
) -> Result<AuthCredentialInstallOutcome, AuthCredentialInstallError> {
    let request = build_provider_token_exchange_request(ProviderOAuthTokenExchangeInput {
        provider: input.provider,
        client_id: input.client_id,
        client_secret: input.client_secret,
        redirect_uri: input.redirect_uri,
        authorization_code: input.authorization_code,
        pkce_code_verifier: input.pkce_code_verifier,
    })
    .map_err(|_| AuthCredentialInstallError::BuildRequest)?;
    let store =
        SqliteStore::open(&input.db_path).map_err(|_| AuthCredentialInstallError::OpenDatabase)?;
    let mut transport = FixtureOAuthTokenTransport {
        response_path: Some(input.token_response_path),
        response_paths_by_provider: HashMap::new(),
        requests: Vec::new(),
    };
    let mut secret_store = input.credential_store.into_secret_store();
    let exchange = exchange_provider_oauth_token(
        &request,
        &input.account_id,
        ProviderAuthBootstrapMode::AuthBroker,
        input.now_unix_seconds,
        &mut transport,
        &mut secret_store,
    )
    .map_err(|_| AuthCredentialInstallError::Exchange)?;
    if store
        .upsert_provider_credential_exchange(exchange.clone())
        .is_err()
    {
        let _ = secret_store.delete_provider_tokens(ProviderCredentialSecretLookup {
            provider: exchange.provider,
            account_id: exchange.account_id.clone(),
            credential_store_ref: exchange.credential_store_ref.clone(),
        });
        return Err(AuthCredentialInstallError::PersistMetadata);
    }

    Ok(AuthCredentialInstallOutcome {
        exchange,
        requests: transport.requests,
    })
}

fn write_text_auth_exchange_code<W>(
    stdout: &mut W,
    provider: Provider,
    account_id: &str,
    exchange: &sync_core::provider::ProviderCredentialExchangeResult,
    requests: &[ProviderOAuthTokenRequest],
) where
    W: Write,
{
    let endpoint_source = requests
        .first()
        .map(|request| oauth_endpoint_source_name(request.endpoint_source))
        .unwrap_or("none");
    let _ = writeln!(
        stdout,
        "sync-v2 auth exchange-code: provider={} account={} outcome=installed local-only test_token_transport=true opens_browser=false exchanges_authorization_code=true credential_source=test_credential_bundle credential_bundle_codec=test-reversing metadata_updated=true token_request_count={} endpoint_source={} auth_flow={} bootstrap_mode={} expires_at={} refresh_state={}",
        provider.as_str(),
        account_id,
        requests.len(),
        endpoint_source,
        auth_flow_name(exchange.auth_flow),
        auth_bootstrap_mode_name(exchange.bootstrap_mode),
        exchange.access_token_expires_at_epoch_secs,
        refresh_token_state_name(exchange.refresh_token)
    );
}

fn write_json_auth_exchange_code<W>(
    stdout: &mut W,
    provider: Provider,
    account_id: &str,
    exchange: &sync_core::provider::ProviderCredentialExchangeResult,
    requests: &[ProviderOAuthTokenRequest],
) where
    W: Write,
{
    let report = json!({
        "provider": provider.as_str(),
        "account": account_id,
        "outcome": "installed",
        "local_only": true,
        "test_token_transport": true,
        "opens_browser": false,
        "exchanges_authorization_code": true,
        "credential_source": "test_credential_bundle",
        "credential_bundle_codec": "test-reversing",
        "metadata_updated": true,
        "token_request_count": requests.len(),
        "endpoint_source": requests
            .first()
            .map(|request| oauth_endpoint_source_name(request.endpoint_source))
            .unwrap_or("none"),
        "auth_flow": auth_flow_name(exchange.auth_flow),
        "bootstrap_mode": auth_bootstrap_mode_name(exchange.bootstrap_mode),
        "expires_at_epoch_secs": exchange.access_token_expires_at_epoch_secs,
        "refresh_state": refresh_token_state_name(exchange.refresh_token),
    });
    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn write_text_auth_complete<W>(
    stdout: &mut W,
    provider: Provider,
    account_id: &str,
    exchange: &sync_core::provider::ProviderCredentialExchangeResult,
    requests: &[ProviderOAuthTokenRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: &str,
) where
    W: Write,
{
    let endpoint_source = requests
        .first()
        .map(|request| oauth_endpoint_source_name(request.endpoint_source))
        .unwrap_or("none");
    let _ = writeln!(
        stdout,
        "sync-v2 auth complete: provider={} account={} outcome=installed local-only test_token_transport=true opens_browser=false uses_auth_session=true callback_valid=true exchanges_authorization_code=true credential_source={} credential_bundle_codec={} metadata_updated=true token_request_count={} endpoint_source={} auth_flow={} bootstrap_mode={} expires_at={} refresh_state={}",
        provider.as_str(),
        account_id,
        credential_source_label,
        credential_bundle_codec_label,
        requests.len(),
        endpoint_source,
        auth_flow_name(exchange.auth_flow),
        auth_bootstrap_mode_name(exchange.bootstrap_mode),
        exchange.access_token_expires_at_epoch_secs,
        refresh_token_state_name(exchange.refresh_token)
    );
}

fn write_json_auth_complete<W>(
    stdout: &mut W,
    provider: Provider,
    account_id: &str,
    exchange: &sync_core::provider::ProviderCredentialExchangeResult,
    requests: &[ProviderOAuthTokenRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: &str,
) where
    W: Write,
{
    let report = json!({
        "provider": provider.as_str(),
        "account": account_id,
        "outcome": "installed",
        "local_only": true,
        "test_token_transport": true,
        "opens_browser": false,
        "uses_auth_session": true,
        "callback_valid": true,
        "exchanges_authorization_code": true,
        "credential_source": credential_source_label,
        "credential_bundle_codec": credential_bundle_codec_label,
        "metadata_updated": true,
        "token_request_count": requests.len(),
        "endpoint_source": requests
            .first()
            .map(|request| oauth_endpoint_source_name(request.endpoint_source))
            .unwrap_or("none"),
        "auth_flow": auth_flow_name(exchange.auth_flow),
        "bootstrap_mode": auth_bootstrap_mode_name(exchange.bootstrap_mode),
        "expires_at_epoch_secs": exchange.access_token_expires_at_epoch_secs,
        "refresh_state": refresh_token_state_name(exchange.refresh_token),
    });
    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

struct FixtureOAuthTokenTransport {
    response_path: Option<PathBuf>,
    response_paths_by_provider: HashMap<Provider, PathBuf>,
    requests: Vec<ProviderOAuthTokenRequest>,
}

impl ProviderOAuthTokenTransport for FixtureOAuthTokenTransport {
    fn send_token_request(
        &mut self,
        request: &ProviderOAuthTokenRequest,
    ) -> Result<ProviderOAuthTokenHttpResponse, ProviderAuthError> {
        self.requests.push(request.clone());
        let response_path = self
            .response_paths_by_provider
            .get(&request.provider)
            .or(self.response_path.as_ref())
            .ok_or(ProviderAuthError::InvalidOAuthResponse {
                provider: request.provider,
                field: "body",
            })?;
        let body = std::fs::read_to_string(response_path).map_err(|_| {
            ProviderAuthError::InvalidOAuthResponse {
                provider: request.provider,
                field: "body",
            }
        })?;
        Ok(ProviderOAuthTokenHttpResponse { status: 200, body })
    }
}

fn run_auth_refresh<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_auth_refresh_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let db_path = options.db_path.expect("validated db path");
    let provider = options.provider.expect("validated provider");
    let account_id = options.account_id.expect("validated account id");
    let client_id = options.client_id.expect("validated client id");
    let credential_store = match auth_required_credential_store_config(
        options.test_credential_bundle_path.clone(),
        options.credential_bundle_path.clone(),
        options.credential_bundle_passphrase_env.as_deref(),
    ) {
        Ok(config) => config,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            return 2;
        }
    };
    let credential_source_label = credential_store.source_label();
    let credential_bundle_codec_label = credential_store.codec_label();
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);

    if !db_path.exists() {
        let _ = writeln!(
            stderr,
            "failed to open db {}: missing database",
            db_path.display()
        );
        return 1;
    }
    let store = match SqliteStore::open_existing_read_write(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };
    let mut transport = FixtureOAuthTokenTransport {
        response_path: options.token_response_path,
        response_paths_by_provider: HashMap::new(),
        requests: Vec::new(),
    };
    let mut secret_store = credential_store.into_secret_store();

    let outcome = match refresh_stored_provider_credential_if_needed(
        &store,
        provider,
        &account_id,
        client_id,
        options.client_secret,
        now_unix_seconds,
        &mut transport,
        &mut secret_store,
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to refresh credential: {}",
                credential_refresh_error_kind(&error)
            );
            return 1;
        }
    };

    let exit_code = match &outcome {
        ProviderCredentialRefreshOutcome::UseStoredAccessToken
        | ProviderCredentialRefreshOutcome::Refreshed(_) => 0,
        ProviderCredentialRefreshOutcome::Reauthorize { .. } => 1,
    };

    match options.output_format {
        PlanOutputFormat::Text => write_text_auth_refresh(
            stdout,
            provider,
            &account_id,
            &outcome,
            &transport.requests,
            credential_source_label,
            credential_bundle_codec_label,
        ),
        PlanOutputFormat::Json => write_json_auth_refresh(
            stdout,
            provider,
            &account_id,
            &outcome,
            &transport.requests,
            credential_source_label,
            credential_bundle_codec_label,
        ),
    }

    exit_code
}

fn write_text_auth_refresh<W>(
    stdout: &mut W,
    provider: Provider,
    account_id: &str,
    outcome: &ProviderCredentialRefreshOutcome,
    requests: &[ProviderOAuthTokenRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: &str,
) where
    W: Write,
{
    let (outcome_name, metadata_updated, suffix) = match outcome {
        ProviderCredentialRefreshOutcome::UseStoredAccessToken => {
            ("use_stored", false, String::new())
        }
        ProviderCredentialRefreshOutcome::Reauthorize { preferred_mode } => (
            "reauthorize",
            false,
            format!(" mode={}", auth_bootstrap_mode_name(*preferred_mode)),
        ),
        ProviderCredentialRefreshOutcome::Refreshed(result) => (
            "refreshed",
            true,
            format!(
                " expires_at={} refresh_state={}",
                result.access_token_expires_at_epoch_secs,
                refresh_token_state_name(result.refresh_token)
            ),
        ),
    };
    let endpoint_source = requests
        .first()
        .map(|request| oauth_endpoint_source_name(request.endpoint_source))
        .unwrap_or("none");
    let _ = writeln!(
        stdout,
        "sync-v2 auth refresh: provider={} account={} outcome={}{} local-only test_token_transport=true opens_browser=false exchanges_authorization_code=false credential_source={} credential_bundle_codec={} metadata_updated={} token_request_count={} endpoint_source={}",
        provider.as_str(),
        account_id,
        outcome_name,
        suffix,
        credential_source_label,
        credential_bundle_codec_label,
        metadata_updated,
        requests.len(),
        endpoint_source
    );
}

fn write_json_auth_refresh<W>(
    stdout: &mut W,
    provider: Provider,
    account_id: &str,
    outcome: &ProviderCredentialRefreshOutcome,
    requests: &[ProviderOAuthTokenRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: &str,
) where
    W: Write,
{
    let mut report = json!({
        "provider": provider.as_str(),
        "account": account_id,
        "local_only": true,
        "test_token_transport": true,
        "opens_browser": false,
        "exchanges_authorization_code": false,
        "credential_source": credential_source_label,
        "credential_bundle_codec": credential_bundle_codec_label,
        "token_request_count": requests.len(),
        "endpoint_source": requests
            .first()
            .map(|request| oauth_endpoint_source_name(request.endpoint_source))
            .unwrap_or("none"),
    });

    match outcome {
        ProviderCredentialRefreshOutcome::UseStoredAccessToken => {
            report["outcome"] = json!("use_stored");
            report["metadata_updated"] = json!(false);
        }
        ProviderCredentialRefreshOutcome::Reauthorize { preferred_mode } => {
            report["outcome"] = json!("reauthorize");
            report["metadata_updated"] = json!(false);
            report["mode"] = json!(auth_bootstrap_mode_name(*preferred_mode));
        }
        ProviderCredentialRefreshOutcome::Refreshed(result) => {
            report["outcome"] = json!("refreshed");
            report["metadata_updated"] = json!(true);
            report["expires_at_epoch_secs"] = json!(result.access_token_expires_at_epoch_secs);
            report["refresh_state"] = json!(refresh_token_state_name(result.refresh_token));
        }
    }

    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn parse_auth_exchange_code_options(args: &[String]) -> Result<AuthExchangeCodeOptions, String> {
    let mut options = AuthExchangeCodeOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();

        if flag == "--test-token-transport" {
            options.test_token_transport = true;
            index += 1;
            continue;
        }
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--exchange-token"
                | "--open-browser"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
                | "--credential-bundle"
        ) {
            return Err(format!(
                "live provider writes, browser sessions, and direct token input are disabled for auth exchange-code: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--provider" => options.provider = Some(parse_provider(value)?),
            "--account" | "--account-id" => options.account_id = Some(value.to_owned()),
            "--client-id" => options.client_id = Some(value.to_owned()),
            "--client-secret" => options.client_secret = Some(value.to_owned()),
            "--redirect-uri" => options.redirect_uri = Some(value.to_owned()),
            "--authorization-code" | "--code" => {
                options.authorization_code = Some(value.to_owned())
            }
            "--pkce-code-verifier" | "--code-verifier" => {
                options.pkce_code_verifier = Some(value.to_owned())
            }
            "--test-credential-bundle" => {
                options.test_credential_bundle_path = Some(PathBuf::from(value))
            }
            "--token-response" => options.token_response_path = Some(PathBuf::from(value)),
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("invalid --now unix seconds: {value}"))?,
                )
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => {
                return Err(format!(
                    "unexpected argument for auth exchange-code: {other}"
                ))
            }
        }

        index += 2;
    }

    if !options.test_token_transport {
        return Err("missing --test-token-transport".to_owned());
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options.provider.is_none() {
        return Err("missing --provider".to_owned());
    }
    if options
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --account".to_owned());
    }
    if options
        .client_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --client-id".to_owned());
    }
    if options
        .redirect_uri
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --redirect-uri".to_owned());
    }
    if options
        .authorization_code
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --authorization-code".to_owned());
    }
    if options.test_credential_bundle_path.is_none() {
        return Err("missing --test-credential-bundle".to_owned());
    }
    if options.token_response_path.is_none() {
        return Err("missing --token-response".to_owned());
    }

    Ok(options)
}

fn parse_auth_refresh_options(args: &[String]) -> Result<AuthRefreshOptions, String> {
    let mut options = AuthRefreshOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();

        if flag == "--test-token-transport" {
            options.test_token_transport = true;
            index += 1;
            continue;
        }
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--open-browser"
                | "--exchange-token"
                | "--authorization-code"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
        ) {
            return Err(format!(
                "live provider writes and network auth are disabled for auth refresh: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--provider" => options.provider = Some(parse_provider(value)?),
            "--account" | "--account-id" => options.account_id = Some(value.to_owned()),
            "--client-id" => options.client_id = Some(value.to_owned()),
            "--client-secret" => options.client_secret = Some(value.to_owned()),
            "--test-credential-bundle" => {
                options.test_credential_bundle_path = Some(PathBuf::from(value))
            }
            "--credential-bundle" => options.credential_bundle_path = Some(PathBuf::from(value)),
            "--credential-bundle-passphrase-env" => {
                options.credential_bundle_passphrase_env = Some(value.to_owned())
            }
            "--token-response" => options.token_response_path = Some(PathBuf::from(value)),
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("invalid --now unix seconds: {value}"))?,
                )
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => return Err(format!("unexpected argument for auth refresh: {other}")),
        }

        index += 2;
    }

    if !options.test_token_transport {
        return Err("missing --test-token-transport".to_owned());
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options.provider.is_none() {
        return Err("missing --provider".to_owned());
    }
    if options
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --account".to_owned());
    }
    if options
        .client_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --client-id".to_owned());
    }
    if options.test_credential_bundle_path.is_some() && options.credential_bundle_path.is_some() {
        return Err("choose only one credential bundle source for auth refresh".to_owned());
    }
    if options.test_credential_bundle_path.is_none() && options.credential_bundle_path.is_none() {
        return Err("missing --test-credential-bundle or --credential-bundle".to_owned());
    }
    Ok(options)
}

fn parse_auth_status_options(args: &[String]) -> Result<AuthStatusOptions, String> {
    let mut options = AuthStatusOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--open-browser"
                | "--exchange-token"
                | "--authorization-code"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
        ) {
            return Err(format!(
                "writes and network auth are disabled for auth status: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--provider" => options.provider = Some(parse_provider(value)?),
            "--account" | "--account-id" => options.account_id = Some(value.to_owned()),
            "--test-credential-bundle" => {
                options.test_credential_bundle_path = Some(PathBuf::from(value))
            }
            "--credential-bundle" => options.credential_bundle_path = Some(PathBuf::from(value)),
            "--credential-bundle-passphrase-env" => {
                options.credential_bundle_passphrase_env = Some(value.to_owned())
            }
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("invalid --now unix seconds: {value}"))?,
                )
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => return Err(format!("unexpected argument for auth status: {other}")),
        }

        index += 2;
    }

    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options.provider.is_none() {
        return Err("missing --provider".to_owned());
    }
    if options
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --account".to_owned());
    }

    Ok(options)
}

fn auth_optional_credential_store_config(
    test_credential_bundle_path: Option<PathBuf>,
    credential_bundle_path: Option<PathBuf>,
    credential_bundle_passphrase_env: Option<&str>,
) -> Result<Option<AuthCredentialStoreConfig>, String> {
    match (test_credential_bundle_path, credential_bundle_path) {
        (Some(_), Some(_)) => {
            Err("choose only one credential bundle source for auth command".to_owned())
        }
        (Some(path), None) => {
            if credential_bundle_passphrase_env.is_some() {
                return Err(
                    "--credential-bundle-passphrase-env requires --credential-bundle".to_owned(),
                );
            }
            Ok(Some(AuthCredentialStoreConfig::TestCredentialBundle(path)))
        }
        (None, Some(path)) => {
            let env_name = credential_bundle_passphrase_env
                .filter(|name| !name.trim().is_empty() && !name.chars().any(char::is_control))
                .ok_or_else(|| "missing credential bundle passphrase".to_owned())?;
            let passphrase = std::env::var(env_name)
                .ok()
                .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
                .ok_or_else(|| "missing credential bundle passphrase".to_owned())?;
            Ok(Some(AuthCredentialStoreConfig::EncryptedCredentialBundle {
                path,
                passphrase,
            }))
        }
        (None, None) => {
            if credential_bundle_passphrase_env.is_some() {
                return Err(
                    "--credential-bundle-passphrase-env requires --credential-bundle".to_owned(),
                );
            }
            Ok(None)
        }
    }
}

fn auth_required_credential_store_config(
    test_credential_bundle_path: Option<PathBuf>,
    credential_bundle_path: Option<PathBuf>,
    credential_bundle_passphrase_env: Option<&str>,
) -> Result<AuthCredentialStoreConfig, String> {
    auth_optional_credential_store_config(
        test_credential_bundle_path,
        credential_bundle_path,
        credential_bundle_passphrase_env,
    )?
    .ok_or_else(|| "missing --test-credential-bundle or --credential-bundle".to_owned())
}

fn parse_auth_begin_options(args: &[String]) -> Result<AuthBeginOptions, String> {
    let mut options = AuthBeginOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();
        if flag == "--show-sensitive-authorization-url" {
            options.show_sensitive_authorization_url = true;
            index += 1;
            continue;
        }
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--open-browser"
                | "--exchange-token"
                | "--client-secret"
                | "--authorization-code"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
                | "--db"
                | "--test-credential-bundle"
                | "--token-response"
        ) {
            return Err(format!(
                "browser, network auth, database, provider writes, and credential bundles are disabled for auth begin: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--provider" => options.provider = Some(parse_provider(value)?),
            "--client-id" => options.client_id = Some(value.to_owned()),
            "--redirect-uri" => options.redirect_uri = Some(value.to_owned()),
            "--session-file" => options.session_file_path = Some(PathBuf::from(value)),
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("invalid --now unix seconds: {value}"))?,
                )
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => return Err(format!("unexpected argument for auth begin: {other}")),
        }

        index += 2;
    }

    if options.provider.is_none() {
        return Err("missing --provider".to_owned());
    }
    if options
        .client_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --client-id".to_owned());
    }
    if options
        .redirect_uri
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --redirect-uri".to_owned());
    }
    if options.session_file_path.is_none() {
        return Err("missing --session-file".to_owned());
    }

    Ok(options)
}

fn parse_auth_callback_plan_options(args: &[String]) -> Result<AuthCallbackPlanOptions, String> {
    let mut options = AuthCallbackPlanOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--open-browser"
                | "--exchange-token"
                | "--client-secret"
                | "--authorization-code"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
                | "--db"
                | "--test-credential-bundle"
                | "--token-response"
        ) {
            return Err(format!(
                "browser, network auth, database, provider writes, direct tokens, and credential bundles are disabled for auth callback-plan: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err("missing value for auth callback-plan option".to_owned());
        };

        match flag {
            "--session-file" => options.session_file_path = Some(PathBuf::from(value)),
            "--callback-url" | "--callback" => options.callback_url = Some(value.to_owned()),
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| "invalid --now unix seconds".to_owned())?,
                )
            }
            "--format" => {
                options.output_format = match value.as_str() {
                    "text" => PlanOutputFormat::Text,
                    "json" => PlanOutputFormat::Json,
                    _ => return Err("unsupported output format for auth callback-plan".to_owned()),
                }
            }
            _ => return Err("unexpected argument for auth callback-plan".to_owned()),
        }

        index += 2;
    }

    if options.session_file_path.is_none() {
        return Err("missing --session-file".to_owned());
    }
    if options
        .callback_url
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --callback-url".to_owned());
    }

    Ok(options)
}

fn parse_auth_complete_options(args: &[String]) -> Result<AuthCompleteOptions, String> {
    let mut options = AuthCompleteOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();

        if flag == "--test-token-transport" {
            options.test_token_transport = true;
            index += 1;
            continue;
        }
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--exchange-token"
                | "--open-browser"
                | "--provider"
                | "--client-id"
                | "--redirect-uri"
                | "--authorization-code"
                | "--code"
                | "--pkce-code-verifier"
                | "--code-verifier"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
        ) {
            return Err(format!(
                "live provider writes, browser sessions, direct token input, and session overrides are disabled for auth complete: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err("missing value for auth complete option".to_owned());
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--account" | "--account-id" => options.account_id = Some(value.to_owned()),
            "--session-file" => options.session_file_path = Some(PathBuf::from(value)),
            "--callback-url" | "--callback" => options.callback_url = Some(value.to_owned()),
            "--client-secret" => options.client_secret = Some(value.to_owned()),
            "--test-credential-bundle" => {
                options.test_credential_bundle_path = Some(PathBuf::from(value))
            }
            "--credential-bundle" => options.credential_bundle_path = Some(PathBuf::from(value)),
            "--credential-bundle-passphrase-env" => {
                options.credential_bundle_passphrase_env = Some(value.to_owned())
            }
            "--token-response" => options.token_response_path = Some(PathBuf::from(value)),
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| "invalid --now unix seconds for auth complete".to_owned())?,
                )
            }
            "--format" => {
                options.output_format = match value.as_str() {
                    "text" => PlanOutputFormat::Text,
                    "json" => PlanOutputFormat::Json,
                    _ => return Err("unsupported output format for auth complete".to_owned()),
                }
            }
            _ => return Err("unexpected argument for auth complete".to_owned()),
        }

        index += 2;
    }

    if !options.test_token_transport {
        return Err("missing --test-token-transport".to_owned());
    }
    if options.session_file_path.is_none() {
        return Err("missing --session-file".to_owned());
    }
    if options
        .callback_url
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --callback-url".to_owned());
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --account".to_owned());
    }
    if options.test_credential_bundle_path.is_some() && options.credential_bundle_path.is_some() {
        return Err("choose only one credential bundle source for auth complete".to_owned());
    }
    if options.test_credential_bundle_path.is_none() && options.credential_bundle_path.is_none() {
        return Err("missing --test-credential-bundle or --credential-bundle".to_owned());
    }
    if options.token_response_path.is_none() {
        return Err("missing --token-response".to_owned());
    }

    Ok(options)
}

fn parse_auth_authorize_url_options(args: &[String]) -> Result<AuthAuthorizeUrlOptions, String> {
    let mut options = AuthAuthorizeUrlOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();
        if flag == "--show-sensitive-authorization-url" {
            options.show_sensitive_authorization_url = true;
            index += 1;
            continue;
        }
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--open-browser"
                | "--exchange-token"
                | "--client-secret"
                | "--authorization-code"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
                | "--db"
        ) {
            return Err(format!(
                "browser, network auth, database, and provider writes are disabled for auth authorize-url: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--provider" => options.provider = Some(parse_provider(value)?),
            "--client-id" => options.client_id = Some(value.to_owned()),
            "--redirect-uri" => options.redirect_uri = Some(value.to_owned()),
            "--state" => options.state = Some(value.to_owned()),
            "--pkce-code-challenge" | "--pkce" => {
                options.pkce_code_challenge = Some(value.to_owned())
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => {
                return Err(format!(
                    "unexpected argument for auth authorize-url: {other}"
                ))
            }
        }

        index += 2;
    }

    let Some(provider) = options.provider else {
        return Err("missing --provider".to_owned());
    };
    if options
        .client_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --client-id".to_owned());
    }
    if options
        .redirect_uri
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --redirect-uri".to_owned());
    }
    if options
        .state
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --state".to_owned());
    }
    if provider != Provider::MyAnimeList && options.pkce_code_challenge.is_some() {
        return Err("--pkce-code-challenge is only supported for myanimelist".to_owned());
    }

    Ok(options)
}

fn parse_import_fixture_options(args: &[String]) -> Result<ImportFixtureOptions, String> {
    let mut options = ImportFixtureOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();
        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--provider" => options.provider = Some(parse_provider(value)?),
            "--kind" => options.media_kind = Some(parse_media_kind(value)?),
            "--fixture" | "--file" => options.fixture_path = Some(PathBuf::from(value)),
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--account" | "--account-id" => options.account_id = Some(value.to_owned()),
            "--apply" | "--write" | "--delete" => {
                return Err(format!("writes are disabled for import-fixtures: {flag}"));
            }
            other => return Err(format!("unexpected argument for import-fixtures: {other}")),
        }

        index += 2;
    }

    if options.provider.is_none() {
        return Err("missing --provider".to_owned());
    }
    if options.media_kind.is_none() {
        return Err("missing --kind".to_owned());
    }
    if options.fixture_path.is_none() {
        return Err("missing --fixture".to_owned());
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --account".to_owned());
    }

    Ok(options)
}

fn parse_import_dataset_options(args: &[String]) -> Result<ImportDatasetOptions, String> {
    let mut options = ImportDatasetOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();
        if matches!(
            flag,
            "--apply" | "--write" | "--delete" | "--refresh" | "--reauthorize" | "--open-browser"
        ) {
            return Err(format!(
                "provider writes and network auth are disabled for import-dataset: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--dataset" => options.dataset = Some(parse_dataset_kind(value)?),
            "--file" | "--dataset-file" => options.file_path = Some(PathBuf::from(value)),
            "--kind" => options.media_kind = Some(parse_media_kind(value)?),
            other => return Err(format!("unexpected argument for import-dataset: {other}")),
        }

        index += 2;
    }

    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options.dataset.is_none() {
        return Err("missing --dataset".to_owned());
    }
    if options.file_path.is_none() {
        return Err("missing --file".to_owned());
    }

    match (
        options.dataset.expect("validated dataset"),
        options.media_kind,
    ) {
        (DatasetKind::AnimeOffline, Some(_)) => {
            return Err("anime-offline does not accept --kind".to_owned());
        }
        (DatasetKind::BangumiData, None) => {
            return Err("missing --kind for bangumi-data".to_owned());
        }
        _ => {}
    }

    Ok(options)
}

fn parse_import_legacy_config_options(
    args: &[String],
) -> Result<ImportLegacyConfigOptions, String> {
    let mut options = ImportLegacyConfigOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();
        if matches!(
            flag,
            "--apply" | "--write" | "--delete" | "--refresh" | "--reauthorize" | "--open-browser"
        ) {
            return Err(format!(
                "provider writes and network auth are disabled for import-legacy-config: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--kind" => options.media_kind = Some(parse_media_kind(value)?),
            "--manual-relations" | "--manual-relations-file" => {
                options.manual_relations_path = Some(PathBuf::from(value))
            }
            "--ignore-entries" | "--ignore-entries-file" => {
                options.ignore_entries_path = Some(PathBuf::from(value))
            }
            other => {
                return Err(format!(
                    "unexpected argument for import-legacy-config: {other}"
                ))
            }
        }

        index += 2;
    }

    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options.media_kind.is_none() {
        return Err("missing --kind".to_owned());
    }
    if options.manual_relations_path.is_none() && options.ignore_entries_path.is_none() {
        return Err("missing --manual-relations or --ignore-entries".to_owned());
    }

    Ok(options)
}

fn parse_inspect_match_options(args: &[String]) -> Result<InspectMatchOptions, String> {
    let mut options = InspectMatchOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();

        if flag == "--dry-run" {
            options.dry_run = true;
            index += 1;
            continue;
        }
        if matches!(
            flag,
            "--apply" | "--write" | "--delete" | "--refresh" | "--reauthorize" | "--open-browser"
        ) {
            return Err(format!(
                "writes and network calls are disabled for inspect-match: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--kind" => options.media_kind = Some(parse_media_kind(value)?),
            "--query" | "--title" => options.query = Some(value.to_owned()),
            "--release-year" | "--year" => options.release_year = Some(parse_release_year(value)?),
            "--format" => options.output_format = parse_plan_output_format(value)?,
            "--limit" => {
                options.limit = value
                    .parse::<u32>()
                    .map_err(|_| format!("invalid --limit: {value}"))?;
                if options.limit == 0 {
                    return Err("invalid --limit: must be greater than 0".to_owned());
                }
            }
            other => return Err(format!("unexpected argument for inspect-match: {other}")),
        }

        index += 2;
    }

    if !options.dry_run {
        return Err("missing --dry-run".to_owned());
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options.media_kind.is_none() {
        return Err("missing --kind".to_owned());
    }
    if options
        .query
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --query".to_owned());
    }

    Ok(options)
}

fn parse_auto_link_options(args: &[String]) -> Result<AutoLinkOptions, String> {
    let mut options = AutoLinkOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();

        if flag == "--dry-run" {
            options.dry_run = true;
            index += 1;
            continue;
        }
        if flag == "--apply-local" {
            options.apply_local = true;
            index += 1;
            continue;
        }
        if matches!(
            flag,
            "--apply" | "--write" | "--delete" | "--refresh" | "--reauthorize" | "--open-browser"
        ) {
            return Err(format!(
                "provider writes and network calls are disabled for auto-link: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--provider" => options.provider = Some(parse_provider(value)?),
            "--kind" => options.media_kind = Some(parse_media_kind(value)?),
            "--id" | "--external-id" | "--provider-id" => {
                options.external_id = Some(value.to_owned())
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => return Err(format!("unexpected argument for auto-link: {other}")),
        }

        index += 2;
    }

    match (options.dry_run, options.apply_local) {
        (true, true) => return Err("choose only one of --dry-run or --apply-local".to_owned()),
        (false, false) => return Err("missing --dry-run or --apply-local".to_owned()),
        _ => {}
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options.provider.is_none() {
        return Err("missing --provider".to_owned());
    }
    if options.media_kind.is_none() {
        return Err("missing --kind".to_owned());
    }
    if options
        .external_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --id".to_owned());
    }

    Ok(options)
}

fn parse_fixture(
    provider: Provider,
    media_kind: MediaKind,
    json: &str,
) -> Result<ProviderCollectionSnapshot, sync_core::provider::ProviderFixtureError> {
    match provider {
        Provider::Bangumi => parse_bangumi_collection_fixture(json, media_kind),
        Provider::AniList => parse_anilist_collection_fixture(json, media_kind),
        Provider::MyAnimeList => parse_myanimelist_collection_fixture(json, media_kind),
    }
}

fn validate_fixture_response(
    fixture: &FixtureResponseInput,
    json: &str,
) -> Result<(), sync_core::provider::ProviderFixtureError> {
    match &fixture.endpoint {
        FixtureResponseEndpoint::Collection => {
            parse_fixture(fixture.provider, fixture.media_kind, json).map(|_| ())
        }
        FixtureResponseEndpoint::BangumiEpisodeCollection { subject_id } => {
            parse_bangumi_episode_collection_fixture(subject_id, json).map(|_| ())
        }
    }
}

fn parse_provider(value: &str) -> Result<Provider, String> {
    match value {
        "bangumi" => Ok(Provider::Bangumi),
        "anilist" => Ok(Provider::AniList),
        "myanimelist" | "mal" => Ok(Provider::MyAnimeList),
        other => Err(format!("unsupported provider: {other}")),
    }
}

fn parse_media_kind(value: &str) -> Result<MediaKind, String> {
    match value {
        "anime" => Ok(MediaKind::Anime),
        "manga" => Ok(MediaKind::Manga),
        other => Err(format!("unsupported media kind: {other}")),
    }
}

fn parse_release_year(value: &str) -> Result<u16, String> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("invalid --release-year: {value}"));
    }

    value
        .parse::<u16>()
        .map_err(|_| format!("invalid --release-year: {value}"))
}

fn parse_dataset_kind(value: &str) -> Result<DatasetKind, String> {
    match value {
        "anime-offline" | "anime-offline-database" => Ok(DatasetKind::AnimeOffline),
        "bangumi-data" => Ok(DatasetKind::BangumiData),
        other => Err(format!("unsupported dataset: {other}")),
    }
}

struct FixtureAccessTokenStore;

impl ProviderCredentialAccessTokenStore for FixtureAccessTokenStore {
    fn get_provider_access_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<ProviderAccessToken, ProviderAuthError> {
        ProviderAccessToken::new(lookup.provider, "fixture-access-token")
    }
}

enum FixturePlanCredentialStore {
    AccessToken(FixtureAccessTokenStore),
    TestCredentialBundle(FileCredentialBundleSecretStore<FixtureCredentialBundleCodec>),
    EncryptedCredentialBundle(FileCredentialBundleSecretStore<PassphraseCredentialBundleCodec>),
}

impl FixturePlanCredentialStore {
    fn source_label(&self) -> &'static str {
        match self {
            Self::AccessToken(_) => "fixture_token_store",
            Self::TestCredentialBundle(_) => "test_credential_bundle",
            Self::EncryptedCredentialBundle(_) => "file_credential_bundle",
        }
    }

    fn codec_label(&self) -> Option<&'static str> {
        match self {
            Self::AccessToken(_) => None,
            Self::TestCredentialBundle(_) => Some("test-reversing"),
            Self::EncryptedCredentialBundle(_) => {
                Some(PassphraseCredentialBundleCodec::CODEC_LABEL)
            }
        }
    }
}

fn fixture_plan_credential_store(
    config: Option<AuthCredentialStoreConfig>,
) -> FixturePlanCredentialStore {
    match config {
        Some(AuthCredentialStoreConfig::TestCredentialBundle(path)) => {
            FixturePlanCredentialStore::TestCredentialBundle(FileCredentialBundleSecretStore::new(
                path,
                FixtureCredentialBundleCodec,
            ))
        }
        Some(AuthCredentialStoreConfig::EncryptedCredentialBundle { path, passphrase }) => {
            FixturePlanCredentialStore::EncryptedCredentialBundle(
                FileCredentialBundleSecretStore::new(
                    path,
                    PassphraseCredentialBundleCodec::new(passphrase),
                ),
            )
        }
        None => FixturePlanCredentialStore::AccessToken(FixtureAccessTokenStore),
    }
}

impl ProviderCredentialAccessTokenStore for FixturePlanCredentialStore {
    fn get_provider_access_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<ProviderAccessToken, ProviderAuthError> {
        match self {
            Self::AccessToken(store) => store.get_provider_access_token(lookup),
            Self::TestCredentialBundle(store) => store.get_provider_access_token(lookup),
            Self::EncryptedCredentialBundle(store) => store.get_provider_access_token(lookup),
        }
    }
}

impl ProviderCredentialSecretStore for FixturePlanCredentialStore {
    fn get_provider_refresh_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<String, ProviderAuthError> {
        match self {
            Self::AccessToken(_) => Err(ProviderAuthError::InvalidOAuthParameter {
                provider: lookup.provider,
                field: "credential_store_ref",
            }),
            Self::TestCredentialBundle(store) => store.get_provider_refresh_token(lookup),
            Self::EncryptedCredentialBundle(store) => store.get_provider_refresh_token(lookup),
        }
    }

    fn put_provider_tokens(
        &mut self,
        input: ProviderCredentialSecretStoreInput,
    ) -> Result<String, ProviderAuthError> {
        match self {
            Self::AccessToken(_) => Err(ProviderAuthError::InvalidOAuthParameter {
                provider: input.provider,
                field: "credential_store_ref",
            }),
            Self::TestCredentialBundle(store) => store.put_provider_tokens(input),
            Self::EncryptedCredentialBundle(store) => store.put_provider_tokens(input),
        }
    }
}

#[derive(Debug, Clone)]
struct FixtureCredentialBundleCodec;

impl CredentialBundleSealCodec for FixtureCredentialBundleCodec {
    fn open_plaintext_bundle(
        &mut self,
        envelope: &CredentialBundleEnvelope,
    ) -> Result<String, CredentialBundleError> {
        if envelope.codec != "cli-fixture-reversing-v1" {
            return Err(CredentialBundleError::OpenFailed);
        }
        Ok(envelope.sealed_payload.chars().rev().collect())
    }

    fn seal_plaintext_bundle(
        &mut self,
        plaintext_json: &str,
    ) -> Result<CredentialBundleEnvelope, CredentialBundleError> {
        Ok(CredentialBundleEnvelope {
            format_version: 1,
            codec: "cli-fixture-reversing-v1".to_owned(),
            sealed_payload: plaintext_json.chars().rev().collect(),
        })
    }
}

impl CliCredentialSecretStore {
    fn delete_provider_tokens(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<bool, ProviderAuthError> {
        match self {
            Self::TestCredentialBundle(store) => store.delete_provider_tokens(lookup),
            Self::EncryptedCredentialBundle(store) => store.delete_provider_tokens(lookup),
        }
    }
}

impl ProviderCredentialAccessTokenStore for CliCredentialSecretStore {
    fn get_provider_access_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<ProviderAccessToken, ProviderAuthError> {
        match self {
            Self::TestCredentialBundle(store) => store.get_provider_access_token(lookup),
            Self::EncryptedCredentialBundle(store) => store.get_provider_access_token(lookup),
        }
    }
}

impl ProviderCredentialSecretStore for CliCredentialSecretStore {
    fn get_provider_refresh_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<String, ProviderAuthError> {
        match self {
            Self::TestCredentialBundle(store) => store.get_provider_refresh_token(lookup),
            Self::EncryptedCredentialBundle(store) => store.get_provider_refresh_token(lookup),
        }
    }

    fn put_provider_tokens(
        &mut self,
        input: ProviderCredentialSecretStoreInput,
    ) -> Result<String, ProviderAuthError> {
        match self {
            Self::TestCredentialBundle(store) => store.put_provider_tokens(input),
            Self::EncryptedCredentialBundle(store) => store.put_provider_tokens(input),
        }
    }
}

struct FixtureAuthorizedReadTransport {
    responses: FixtureResponseMap,
}

#[derive(Default)]
struct FixtureAuthorizedWriteTransport {
    requests: Vec<AuthorizedProviderWriteRequest>,
}

impl AuthorizedProviderReadTransport for FixtureAuthorizedReadTransport {
    fn send_authorized_provider_read_request(
        &mut self,
        request: AuthorizedProviderReadRequest,
    ) -> Result<String, String> {
        let provider = request.request.provider;
        let media_kind = infer_request_media_kind(&request.request)
            .ok_or_else(|| format!("could not infer fixture media kind for {provider:?}"))?;
        let is_bangumi_episode_collection = is_bangumi_episode_collection_request(&request.request);
        let responses = if is_bangumi_episode_collection {
            let subject_id = bangumi_episode_collection_subject_id(&request.request)
                .ok_or_else(|| "could not infer Bangumi episode fixture subject".to_owned())?;
            match self
                .responses
                .bangumi_episode_collections
                .get_mut(&subject_id)
            {
                Some(responses) => responses,
                None => return Ok(r#"{"data":[]}"#.to_owned()),
            }
        } else {
            self.responses
                .collections
                .get_mut(&(provider, media_kind))
                .ok_or_else(|| {
                    format!(
                        "missing fixture response for provider={} kind={}",
                        provider.as_str(),
                        media_kind.as_str()
                    )
                })?
        };
        if responses.is_empty() {
            if is_bangumi_episode_collection {
                return Err(
                    "fixture response exhausted for provider=bangumi kind=anime endpoint=episodes"
                        .to_owned(),
                );
            }
            return Err(format!(
                "fixture response exhausted for provider={} kind={}",
                provider.as_str(),
                media_kind.as_str()
            ));
        }

        Ok(responses.remove(0))
    }
}

impl AuthorizedProviderWriteTransport for FixtureAuthorizedWriteTransport {
    fn send_authorized_provider_write_request(
        &mut self,
        request: AuthorizedProviderWriteRequest,
    ) -> Result<String, String> {
        let provider = request.request.provider;
        self.requests.push(request);
        Ok(format!(
            r#"{{"ok":true,"provider":"{}","test_write_transport":true}}"#,
            provider.as_str()
        ))
    }
}

struct CliProviderHttpClient {
    agent: ureq::Agent,
}

impl CliProviderHttpClient {
    fn new() -> Self {
        Self {
            agent: ureq::Agent::new(),
        }
    }
}

impl ProviderHttpClient for CliProviderHttpClient {
    fn send_provider_http_request(
        &mut self,
        request: ProviderHttpRequest,
    ) -> Result<ProviderHttpResponse, String> {
        let provider = request.provider;
        let mut http_request = match request.method {
            ProviderReadRequestMethod::Get => self.agent.get(&request.url),
            ProviderReadRequestMethod::Post => self.agent.post(&request.url),
        };
        for header in request.headers {
            http_request = http_request.set(&header.name, &header.value);
        }

        let response = match request.body {
            Some(body) => http_request.send_string(&body),
            None => http_request.call(),
        };

        match response {
            Ok(response) => response_to_provider_http_response(response, provider),
            Err(ureq::Error::Status(status, response)) => {
                response_to_provider_http_response_with_status(response, provider, status)
            }
            Err(_) => Err(format!(
                "HTTP transport failed for provider={}",
                provider.as_str()
            )),
        }
    }
}

fn response_to_provider_http_response(
    response: ureq::Response,
    provider: Provider,
) -> Result<ProviderHttpResponse, String> {
    let status = response.status();
    response_to_provider_http_response_with_status(response, provider, status)
}

fn response_to_provider_http_response_with_status(
    response: ureq::Response,
    provider: Provider,
    status: u16,
) -> Result<ProviderHttpResponse, String> {
    let body = response.into_string().map_err(|_| {
        format!(
            "HTTP response body read failed for provider={}",
            provider.as_str()
        )
    })?;
    Ok(ProviderHttpResponse { status, body })
}

fn run_plan_refresh_snapshots<W, E>(options: PlanOptions, stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let db_path = options.db_path.expect("validated db path");
    let account_id = options.account_id.expect("validated account id");
    let media_scope = options.media_scope.expect("validated media scope");
    let providers = options.providers.expect("validated providers");
    let credential_store_config = match auth_optional_credential_store_config(
        options.test_credential_bundle_path.clone(),
        options.credential_bundle_path.clone(),
        options.credential_bundle_passphrase_env.as_deref(),
    ) {
        Ok(config) => config,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            return 2;
        }
    };
    let media_kinds = plan_media_kinds(media_scope);
    let responses = if options.live_read {
        None
    } else {
        match load_fixture_response_map(
            &options.fixture_responses,
            &media_kinds,
            &providers,
            stderr,
        ) {
            Ok(responses) => Some(responses),
            Err(code) => return code,
        }
    };

    if !db_path.exists() {
        let _ = writeln!(
            stderr,
            "failed to open db {}: missing database",
            db_path.display()
        );
        return 1;
    }

    let store = match SqliteStore::open_existing_read_write(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };
    let mut secret_store = fixture_plan_credential_store(credential_store_config.clone());
    let credential_source_label = secret_store.source_label();
    let credential_bundle_codec_label = secret_store.codec_label();
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);

    let (outcome_result, token_request_count) = if options.live_read {
        let mut read_transport = ProviderHttpReadTransport::new(CliProviderHttpClient::new());
        refresh_plan_with_read_transport(
            &store,
            &account_id,
            &media_kinds,
            &providers,
            options.auto_link_local,
            options.test_token_transport,
            options.limit,
            &options.token_responses,
            &options.refresh_client_ids,
            &options.refresh_client_secrets,
            now_unix_seconds,
            &mut secret_store,
            &mut read_transport,
        )
    } else {
        let mut read_transport = FixtureAuthorizedReadTransport {
            responses: responses.expect("fixture responses should be loaded"),
        };
        refresh_plan_with_read_transport(
            &store,
            &account_id,
            &media_kinds,
            &providers,
            options.auto_link_local,
            options.test_token_transport,
            options.limit,
            &options.token_responses,
            &options.refresh_client_ids,
            &options.refresh_client_secrets,
            now_unix_seconds,
            &mut secret_store,
            &mut read_transport,
        )
    };

    let outcome = match outcome_result {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to refresh snapshots and build dry-run plan: {error:?}"
            );
            return 1;
        }
    };

    match options.output_format {
        PlanOutputFormat::Text => write_text_refresh_plan(
            stdout,
            &outcome,
            credential_source_label,
            credential_bundle_codec_label,
            options.auto_link_local,
            options.live_read,
            options.test_token_transport,
            token_request_count,
        ),
        PlanOutputFormat::Json => write_json_refresh_plan(
            stdout,
            &outcome,
            credential_source_label,
            credential_bundle_codec_label,
            options.auto_link_local,
            options.live_read,
            options.test_token_transport,
            token_request_count,
        ),
    }

    0
}

// Keep the injected auth, token, credential, and read transports explicit at
// the CLI orchestration boundary. This avoids silently selecting live effects.
#[allow(clippy::too_many_arguments)]
fn refresh_plan_with_read_transport<T>(
    store: &SqliteStore,
    account_id: &str,
    media_kinds: &[MediaKind],
    providers: &[Provider],
    auto_link_local: bool,
    test_token_transport: bool,
    limit: u16,
    token_responses: &[ProviderTokenResponseInput],
    refresh_client_ids: &[ProviderScopedValue],
    refresh_client_secrets: &[ProviderScopedValue],
    now_unix_seconds: i64,
    secret_store: &mut FixturePlanCredentialStore,
    read_transport: &mut T,
) -> (
    Result<StoredProviderSyncCycleOutcome, StoredProviderSyncCycleError>,
    usize,
)
where
    T: AuthorizedProviderReadTransport,
{
    if test_token_transport {
        let refresh_configs =
            build_refresh_configs(providers, refresh_client_ids, refresh_client_secrets);
        let mut token_transport = FixtureOAuthTokenTransport {
            response_path: None,
            response_paths_by_provider: token_response_path_map(token_responses),
            requests: Vec::new(),
        };
        let result = refresh_snapshots_and_plan_dry_run_with_auto_refresh(
            store,
            account_id,
            media_kinds,
            providers,
            auto_link_local,
            &refresh_configs,
            limit,
            now_unix_seconds,
            &mut token_transport,
            secret_store,
            read_transport,
        );
        (result, token_transport.requests.len())
    } else {
        (
            refresh_snapshots_and_plan_dry_run_with_stored_access_tokens(
                store,
                account_id,
                media_kinds,
                providers,
                auto_link_local,
                limit,
                now_unix_seconds,
                secret_store,
                read_transport,
            ),
            0,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn write_text_refresh_plan<W>(
    stdout: &mut W,
    outcome: &StoredProviderSyncCycleOutcome,
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
    live_read: bool,
    test_token_transport: bool,
    token_request_count: usize,
) where
    W: Write,
{
    let credential_bundle_codec = credential_bundle_codec_label
        .map(|label| format!(" credential_bundle_codec={label}"))
        .unwrap_or_default();
    let token_transport = if test_token_transport {
        format!(" test_token_transport=true token_request_count={token_request_count}")
    } else {
        String::new()
    };
    let auto_link = if auto_link_local {
        " local_auto_link=true"
    } else {
        ""
    };
    let read_transport = if live_read {
        "fixture_transport=false live_read=true"
    } else {
        "fixture_transport=true live_read=false"
    };
    match outcome {
        StoredProviderSyncCycleOutcome::Planned { imports, plan } => {
            let _ = writeln!(
                stdout,
                "sync-v2 plan: dry-run refresh-snapshots local_only={} {} provider_writes=false credential_source={}{}{}{} imports={} actions={} conflicts={} unmatched={}",
                !live_read,
                read_transport,
                credential_source_label,
                credential_bundle_codec,
                auto_link,
                token_transport,
                imports.len(),
                plan.actions().len(),
                plan.conflicts().len(),
                plan.skipped_unmatched_count()
            );
            write_text_import_summaries(stdout, imports);
            write_text_plan(stdout, plan);
        }
        StoredProviderSyncCycleOutcome::CredentialsRequired { imports } => {
            let _ = writeln!(
                stdout,
                "sync-v2 plan: dry-run refresh-snapshots credentials_required local_only={} {} provider_writes=false credential_source={}{}{}{} imports={}",
                !live_read,
                read_transport,
                credential_source_label,
                credential_bundle_codec,
                auto_link,
                token_transport,
                imports.len()
            );
            write_text_import_summaries(stdout, imports);
        }
    }
}

fn write_text_import_summaries<W>(
    stdout: &mut W,
    imports: &[sync_core::sync::ProviderSnapshotImportSummary],
) where
    W: Write,
{
    for summary in imports {
        let _ = writeln!(
            stdout,
            "- import {} {} {}",
            summary.provider.as_str(),
            summary.media_kind.as_str(),
            import_outcome_text(&summary.outcome)
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn write_json_refresh_plan<W>(
    stdout: &mut W,
    outcome: &StoredProviderSyncCycleOutcome,
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
    live_read: bool,
    test_token_transport: bool,
    token_request_count: usize,
) where
    W: Write,
{
    let (status, imports, plan) = match outcome {
        StoredProviderSyncCycleOutcome::Planned { imports, plan } => {
            ("planned", imports, Some(plan_to_json_value(plan)))
        }
        StoredProviderSyncCycleOutcome::CredentialsRequired { imports } => {
            ("credentials_required", imports, None)
        }
    };
    let mut report = json!({
        "dry_run": true,
        "local_only": !live_read,
        "provider_writes": false,
        "fixture_transport": !live_read,
        "live_read": live_read,
        "credential_source": credential_source_label,
        "local_snapshot_import": true,
        "local_auto_link": auto_link_local,
        "outcome": status,
        "imports": imports.iter().map(import_summary_to_json).collect::<Vec<_>>(),
        "plan": plan,
    });
    if let Some(codec_label) = credential_bundle_codec_label {
        report["credential_bundle_codec"] = json!(codec_label);
    }
    if test_token_transport {
        report["test_token_transport"] = json!(true);
        report["token_request_count"] = json!(token_request_count);
    }

    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn import_summary_to_json(
    summary: &sync_core::sync::ProviderSnapshotImportSummary,
) -> serde_json::Value {
    let mut value = json!({
        "provider": summary.provider.as_str(),
        "media_kind": summary.media_kind.as_str(),
    });
    match &summary.outcome {
        StoredProviderCollectionImportOutcome::Imported { imported } => {
            value["outcome"] = json!("imported");
            value["imported"] = json!(imported);
        }
        StoredProviderCollectionImportOutcome::RefreshRequired => {
            value["outcome"] = json!("refresh_required");
        }
        StoredProviderCollectionImportOutcome::Reauthorize { preferred_mode } => {
            value["outcome"] = json!("reauthorize");
            value["mode"] = json!(auth_bootstrap_mode_name(*preferred_mode));
        }
    }
    value
}

fn import_outcome_text(outcome: &StoredProviderCollectionImportOutcome) -> String {
    match outcome {
        StoredProviderCollectionImportOutcome::Imported { imported } => {
            format!("imported={imported}")
        }
        StoredProviderCollectionImportOutcome::RefreshRequired => "refresh_required".to_owned(),
        StoredProviderCollectionImportOutcome::Reauthorize { preferred_mode } => {
            format!(
                "reauthorize mode={}",
                auth_bootstrap_mode_name(*preferred_mode)
            )
        }
    }
}

fn load_fixture_response_map<E>(
    fixture_responses: &[FixtureResponseInput],
    media_kinds: &[MediaKind],
    providers: &[Provider],
    stderr: &mut E,
) -> Result<FixtureResponseMap, i32>
where
    E: Write,
{
    let mut responses = FixtureResponseMap::default();

    for fixture in fixture_responses {
        let body = match read_local_file(&fixture.path, stderr) {
            Ok(body) => body,
            Err(()) => return Err(1),
        };
        if let Err(error) = validate_fixture_response(fixture, &body) {
            let _ = writeln!(
                stderr,
                "invalid fixture response for provider={} kind={}: {error:?}",
                fixture.provider.as_str(),
                fixture.media_kind.as_str()
            );
            return Err(1);
        }
        match &fixture.endpoint {
            FixtureResponseEndpoint::Collection => {
                responses
                    .collections
                    .entry((fixture.provider, fixture.media_kind))
                    .or_insert_with(Vec::new)
                    .push(body);
            }
            FixtureResponseEndpoint::BangumiEpisodeCollection { subject_id } => {
                responses
                    .bangumi_episode_collections
                    .entry(subject_id.clone())
                    .or_insert_with(Vec::new)
                    .push(body);
            }
        }
    }

    for media_kind in media_kinds {
        for provider in providers {
            if !responses
                .collections
                .contains_key(&(*provider, *media_kind))
            {
                let _ = writeln!(
                    stderr,
                    "missing --fixture-response for provider={} kind={}",
                    provider.as_str(),
                    media_kind.as_str()
                );
                return Err(2);
            }
        }
    }

    Ok(responses)
}

fn load_fixture_response_map_partial<E>(
    fixture_responses: &[FixtureResponseInput],
    stderr: &mut E,
) -> Result<FixtureResponseMap, i32>
where
    E: Write,
{
    let mut responses = FixtureResponseMap::default();

    for fixture in fixture_responses {
        let body = match read_local_file(&fixture.path, stderr) {
            Ok(body) => body,
            Err(()) => return Err(1),
        };
        if let Err(error) = validate_fixture_response(fixture, &body) {
            let _ = writeln!(
                stderr,
                "invalid fixture response for provider={} kind={}: {error:?}",
                fixture.provider.as_str(),
                fixture.media_kind.as_str()
            );
            return Err(1);
        }
        match &fixture.endpoint {
            FixtureResponseEndpoint::Collection => {
                responses
                    .collections
                    .entry((fixture.provider, fixture.media_kind))
                    .or_insert_with(Vec::new)
                    .push(body);
            }
            FixtureResponseEndpoint::BangumiEpisodeCollection { subject_id } => {
                responses
                    .bangumi_episode_collections
                    .entry(subject_id.clone())
                    .or_insert_with(Vec::new)
                    .push(body);
            }
        }
    }

    Ok(responses)
}

fn infer_request_media_kind(request: &ProviderReadRequest) -> Option<MediaKind> {
    match request.provider {
        Provider::Bangumi => {
            if is_bangumi_episode_collection_request(request)
                || request.url.contains("subject_type=2")
            {
                Some(MediaKind::Anime)
            } else if request.url.contains("subject_type=1") {
                Some(MediaKind::Manga)
            } else {
                None
            }
        }
        Provider::AniList => {
            let body = request.body.as_deref().unwrap_or_default();
            if body.contains("\"ANIME\"") {
                Some(MediaKind::Anime)
            } else if body.contains("\"MANGA\"") {
                Some(MediaKind::Manga)
            } else {
                None
            }
        }
        Provider::MyAnimeList => {
            if request.url.contains("/animelist?") {
                Some(MediaKind::Anime)
            } else if request.url.contains("/mangalist?") {
                Some(MediaKind::Manga)
            } else {
                None
            }
        }
    }
}

fn is_bangumi_episode_collection_request(request: &ProviderReadRequest) -> bool {
    request.provider == Provider::Bangumi
        && request.url.contains("/v0/users/-/collections/")
        && request.url.contains("/episodes?")
        && request.url.contains("episode_type=0")
}

fn bangumi_episode_collection_subject_id(request: &ProviderReadRequest) -> Option<String> {
    if !is_bangumi_episode_collection_request(request) {
        return None;
    }
    let after_collections = request.url.split("/v0/users/-/collections/").nth(1)?;
    let subject_id = after_collections.split("/episodes?").next()?;
    if subject_id.is_empty() {
        None
    } else {
        Some(subject_id.to_owned())
    }
}

fn plan_media_kinds(scope: PlanMediaScope) -> Vec<MediaKind> {
    match scope {
        PlanMediaScope::One(media_kind) => vec![media_kind],
        PlanMediaScope::All => vec![MediaKind::Anime, MediaKind::Manga],
    }
}

fn run_sync<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_sync_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let db_path = options.db_path.expect("validated db path");
    let account_id = options.account_id.expect("validated account id");
    let media_scope = options.media_scope.expect("validated media scope");
    let providers = options.providers.expect("validated providers");
    let credential_store_config = match auth_optional_credential_store_config(
        options.test_credential_bundle_path.clone(),
        options.credential_bundle_path.clone(),
        options.credential_bundle_passphrase_env.as_deref(),
    ) {
        Ok(config) => config,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            return 2;
        }
    };
    let media_kinds = plan_media_kinds(media_scope);
    let responses = match load_fixture_response_map(
        &options.fixture_responses,
        &media_kinds,
        &providers,
        stderr,
    ) {
        Ok(responses) => responses,
        Err(code) => return code,
    };

    if !db_path.exists() {
        let _ = writeln!(
            stderr,
            "failed to open db {}: missing database",
            db_path.display()
        );
        return 1;
    }

    let store = match SqliteStore::open_existing_read_write(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };
    let mut secret_store = fixture_plan_credential_store(credential_store_config.clone());
    let credential_source_label = secret_store.source_label();
    let credential_bundle_codec_label = secret_store.codec_label();
    let mut read_transport = FixtureAuthorizedReadTransport { responses };
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);

    let mut token_request_count = 0usize;
    let outcome_result = if options.test_token_transport {
        let refresh_configs = build_refresh_configs(
            &providers,
            &options.refresh_client_ids,
            &options.refresh_client_secrets,
        );
        let mut token_transport = FixtureOAuthTokenTransport {
            response_path: None,
            response_paths_by_provider: token_response_path_map(&options.token_responses),
            requests: Vec::new(),
        };
        let result = refresh_snapshots_and_plan_dry_run_with_auto_refresh(
            &store,
            &account_id,
            &media_kinds,
            &providers,
            options.auto_link_local,
            &refresh_configs,
            options.limit,
            now_unix_seconds,
            &mut token_transport,
            &mut secret_store,
            &mut read_transport,
        );
        token_request_count = token_transport.requests.len();
        result
    } else {
        refresh_snapshots_and_plan_dry_run_with_stored_access_tokens(
            &store,
            &account_id,
            &media_kinds,
            &providers,
            options.auto_link_local,
            options.limit,
            now_unix_seconds,
            &mut secret_store,
            &mut read_transport,
        )
    };

    let outcome = match outcome_result {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to refresh snapshots and build sync plan: {error:?}"
            );
            return 1;
        }
    };

    let (imports, plan) = match outcome {
        StoredProviderSyncCycleOutcome::Planned { imports, plan } => (imports, plan),
        StoredProviderSyncCycleOutcome::CredentialsRequired { imports } => {
            match options.output_format {
                PlanOutputFormat::Text => write_text_sync_credentials_required(
                    stdout,
                    &imports,
                    credential_source_label,
                    credential_bundle_codec_label,
                    options.auto_link_local,
                    options.test_token_transport,
                    token_request_count,
                ),
                PlanOutputFormat::Json => write_json_sync_credentials_required(
                    stdout,
                    &imports,
                    credential_source_label,
                    credential_bundle_codec_label,
                    options.auto_link_local,
                    options.test_token_transport,
                    token_request_count,
                ),
            }
            return 1;
        }
    };

    let mut write_transport = FixtureAuthorizedWriteTransport::default();
    let apply_result = if options.verify_post_write {
        let mut write_secret_store = fixture_plan_credential_store(credential_store_config.clone());
        let mut verify_secret_store =
            fixture_plan_credential_store(credential_store_config.clone());
        let mut writer = StoredAccessTokenProviderWriter::new(
            &store,
            now_unix_seconds,
            &mut write_secret_store,
            &mut write_transport,
        );
        let mut verifier = StoredAccessTokenProviderPostWriteVerifier::new_with_snapshot_cache(
            &store,
            now_unix_seconds,
            &mut verify_secret_store,
            &mut read_transport,
            options.limit,
        );
        apply_plan_with_writer_and_deferred_verifier(
            &store,
            &account_id,
            &plan,
            &mut writer,
            &mut verifier,
        )
    } else {
        let mut writer = StoredAccessTokenProviderWriter::new(
            &store,
            now_unix_seconds,
            &mut secret_store,
            &mut write_transport,
        );
        apply_plan_with_writer(&store, &account_id, &plan, &mut writer)
    };
    let summary = match apply_result {
        Ok(summary) => summary,
        Err(ApplyError::ConflictsPresent { count }) => {
            match options.output_format {
                PlanOutputFormat::Text => write_text_sync_blocked_conflicts(
                    stdout,
                    &imports,
                    &plan,
                    count,
                    credential_source_label,
                    credential_bundle_codec_label,
                    options.auto_link_local,
                    options.test_token_transport,
                    token_request_count,
                ),
                PlanOutputFormat::Json => write_json_sync_blocked_conflicts(
                    stdout,
                    &imports,
                    &plan,
                    count,
                    credential_source_label,
                    credential_bundle_codec_label,
                    options.auto_link_local,
                    options.verify_post_write,
                    options.test_token_transport,
                    token_request_count,
                ),
            }
            return 1;
        }
        Err(error) => {
            let _ = writeln!(stderr, "failed to apply test provider writes: {error:?}");
            return 1;
        }
    };

    match options.output_format {
        PlanOutputFormat::Text => write_text_sync_applied(
            stdout,
            &imports,
            &summary,
            &plan,
            &write_transport.requests,
            credential_source_label,
            credential_bundle_codec_label,
            options.auto_link_local,
            options.verify_post_write,
            options.test_token_transport,
            token_request_count,
        ),
        PlanOutputFormat::Json => write_json_sync_applied(
            stdout,
            &imports,
            &summary,
            &plan,
            &write_transport.requests,
            credential_source_label,
            credential_bundle_codec_label,
            options.auto_link_local,
            options.verify_post_write,
            options.test_token_transport,
            token_request_count,
        ),
    }

    0
}

fn run_apply<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let options = match parse_apply_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    let db_path = options.db_path.expect("validated db path");
    let account_id = options.account_id.expect("validated account id");
    let media_scope = options.media_scope.expect("validated media scope");
    let providers = options.providers.expect("validated providers");
    let media_kinds = match media_scope {
        PlanMediaScope::One(media_kind) => vec![media_kind],
        PlanMediaScope::All => vec![MediaKind::Anime, MediaKind::Manga],
    };
    let verify_post_write = options.verify_post_write;
    let fixture_responses = options.fixture_responses;
    let credential_store_config = match auth_optional_credential_store_config(
        options.test_credential_bundle_path.clone(),
        options.credential_bundle_path.clone(),
        options.credential_bundle_passphrase_env.as_deref(),
    ) {
        Ok(config) => config,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            return 2;
        }
    };
    let now_unix_seconds = options
        .now_unix_seconds
        .unwrap_or_else(current_unix_seconds);

    if !db_path.exists() {
        let _ = writeln!(
            stderr,
            "failed to open db {}: missing database",
            db_path.display()
        );
        return 1;
    }

    let store = match SqliteStore::open_existing_read_write(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };
    if options.auto_link_local {
        if let Err(error) =
            auto_link_local_collection_entries(&store, &account_id, &media_kinds, &providers)
        {
            let _ = writeln!(
                stderr,
                "failed to auto-link local collection entries: {error:?}"
            );
            return 1;
        }
    }
    let plan = plan_dry_run_for_media_kinds(&store, &account_id, &media_kinds, &providers);
    let plan = match plan {
        Ok(plan) => plan,
        Err(error) => {
            let _ = writeln!(stderr, "failed to build apply plan: {error:?}");
            return 1;
        }
    };

    let label_store = fixture_plan_credential_store(credential_store_config.clone());
    let credential_source_label = label_store.source_label();
    let credential_bundle_codec_label = label_store.codec_label();
    let mut write_transport = FixtureAuthorizedWriteTransport::default();
    let apply_result = if verify_post_write {
        let responses = match load_fixture_response_map_partial(&fixture_responses, stderr) {
            Ok(responses) => responses,
            Err(code) => return code,
        };
        let mut write_secret_store = fixture_plan_credential_store(credential_store_config.clone());
        let mut verify_secret_store =
            fixture_plan_credential_store(credential_store_config.clone());
        let mut read_transport = FixtureAuthorizedReadTransport { responses };
        let mut writer = StoredAccessTokenProviderWriter::new(
            &store,
            now_unix_seconds,
            &mut write_secret_store,
            &mut write_transport,
        );
        let mut verifier = StoredAccessTokenProviderPostWriteVerifier::new_with_snapshot_cache(
            &store,
            now_unix_seconds,
            &mut verify_secret_store,
            &mut read_transport,
            options.limit,
        );
        apply_plan_with_writer_and_deferred_verifier(
            &store,
            &account_id,
            &plan,
            &mut writer,
            &mut verifier,
        )
    } else {
        let mut secret_store = fixture_plan_credential_store(credential_store_config);
        let mut writer = StoredAccessTokenProviderWriter::new(
            &store,
            now_unix_seconds,
            &mut secret_store,
            &mut write_transport,
        );
        apply_plan_with_writer(&store, &account_id, &plan, &mut writer)
    };
    let summary = match apply_result {
        Ok(summary) => summary,
        Err(ApplyError::ConflictsPresent { count }) => {
            match options.output_format {
                PlanOutputFormat::Text => write_text_apply_blocked_conflicts(
                    stdout,
                    &plan,
                    count,
                    credential_source_label,
                    credential_bundle_codec_label,
                    options.auto_link_local,
                ),
                PlanOutputFormat::Json => write_json_apply_blocked_conflicts(
                    stdout,
                    &plan,
                    count,
                    credential_source_label,
                    credential_bundle_codec_label,
                    options.auto_link_local,
                ),
            }
            return 1;
        }
        Err(error) => {
            let _ = writeln!(stderr, "failed to apply test provider writes: {error:?}");
            return 1;
        }
    };

    match options.output_format {
        PlanOutputFormat::Text => write_text_apply(
            stdout,
            &summary,
            &plan,
            &write_transport.requests,
            credential_source_label,
            credential_bundle_codec_label,
            options.auto_link_local,
        ),
        PlanOutputFormat::Json => write_json_apply(
            stdout,
            &summary,
            &plan,
            &write_transport.requests,
            credential_source_label,
            credential_bundle_codec_label,
            options.auto_link_local,
        ),
    }

    0
}

fn run_plan<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    if args.is_empty() {
        let _ = writeln!(
            stdout,
            "sync-v2 plan: read-only skeleton command; dry-run behavior only"
        );
        return 0;
    }

    let options = match parse_plan_options(args) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        }
    };

    if options.refresh_snapshots {
        return run_plan_refresh_snapshots(options, stdout, stderr);
    }

    let db_path = options.db_path.expect("validated db path");
    let account_id = options.account_id.expect("validated account id");
    let media_scope = options.media_scope.expect("validated media scope");
    let providers = options.providers.expect("validated providers");
    let store = match SqliteStore::open_read_only(&db_path) {
        Ok(store) => store,
        Err(error) => {
            let _ = writeln!(stderr, "failed to open db {}: {error:?}", db_path.display());
            return 1;
        }
    };
    let plan = match media_scope {
        PlanMediaScope::One(media_kind) => {
            plan_dry_run(&store, &account_id, media_kind, &providers)
        }
        PlanMediaScope::All => plan_dry_run_for_media_kinds(
            &store,
            &account_id,
            &[MediaKind::Anime, MediaKind::Manga],
            &providers,
        ),
    };
    let plan = match plan {
        Ok(plan) => plan,
        Err(error) => {
            let _ = writeln!(stderr, "failed to build dry-run plan: {error:?}");
            return 1;
        }
    };

    match options.output_format {
        PlanOutputFormat::Text => write_text_plan(stdout, &plan),
        PlanOutputFormat::Json => write_json_plan(stdout, &plan),
    }

    0
}

fn write_text_plan<W>(stdout: &mut W, plan: &SyncPlan)
where
    W: Write,
{
    let _ = writeln!(
        stdout,
        "sync-v2 plan: dry-run plan actions={} conflicts={} unmatched={} apply_blocked={} apply_blocked_reason={}",
        plan.actions().len(),
        plan.conflicts().len(),
        plan.skipped_unmatched_count(),
        plan_apply_blocked(plan),
        plan_apply_blocked_reason(plan).unwrap_or("none")
    );
    for action in plan.actions() {
        let kind = match action.kind {
            PlannedActionKind::AddEntry => "add",
            PlannedActionKind::UpdateEntry => "update",
        };
        let fields = action
            .field_updates
            .iter()
            .map(|field| format!("{field:?}"))
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(
            stdout,
            "- {kind} {} -> {} target={} fields={} reason={}",
            action.source_provider.as_str(),
            action.target_provider.as_str(),
            action.target_provider_entry_id,
            fields,
            action.reason
        );
        for change in &action.field_changes {
            let old = change.old_value.as_deref().unwrap_or("<unset>");
            let _ = writeln!(
                stdout,
                "  change {} {} -> {}",
                change.field.as_str(),
                old,
                change.new_value
            );
        }
    }
    for conflict in plan.conflicts() {
        let providers = conflict
            .values
            .iter()
            .map(|value| {
                format!(
                    "{}={}",
                    value.provider.as_str(),
                    text_conflict_value(conflict.field, &value.value)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(
            stdout,
            "- conflict work={} field={:?} values={} reason={}",
            conflict.work_id, conflict.field, providers, conflict.reason
        );
    }
}

fn write_json_plan<W>(stdout: &mut W, plan: &SyncPlan)
where
    W: Write,
{
    let report = plan_to_json_value(plan);
    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn write_text_plan_preview<W>(stdout: &mut W, plan: &SyncPlan)
where
    W: Write,
{
    write_text_plan_preview_for_command(stdout, "sync", plan);
}

fn write_text_plan_preview_for_command<W>(stdout: &mut W, command: &str, plan: &SyncPlan)
where
    W: Write,
{
    let _ = writeln!(
        stdout,
        "sync-v2 {}: plan_preview actions={} conflicts={} unmatched={} apply_blocked={} apply_blocked_reason={}",
        command,
        plan.actions().len(),
        plan.conflicts().len(),
        plan.skipped_unmatched_count(),
        plan_apply_blocked(plan),
        plan_apply_blocked_reason(plan).unwrap_or("none")
    );
    for action in plan.actions() {
        let kind = match action.kind {
            PlannedActionKind::AddEntry => "add",
            PlannedActionKind::UpdateEntry => "update",
        };
        let fields = action
            .field_updates
            .iter()
            .map(|field| field.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(
            stdout,
            "- preview {kind} {} -> {} target={} fields={} reason={}",
            action.source_provider.as_str(),
            action.target_provider.as_str(),
            action.target_provider_entry_id,
            fields,
            action.reason
        );
    }
    for conflict in plan.conflicts() {
        let providers = conflict
            .values
            .iter()
            .map(|value| {
                format!(
                    "{}={}",
                    value.provider.as_str(),
                    text_conflict_value(conflict.field, &value.value)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(
            stdout,
            "- preview conflict work={} field={:?} values={} reason={}",
            conflict.work_id, conflict.field, providers, conflict.reason
        );
    }
}

fn text_conflict_value(field: SyncField, value: &str) -> String {
    if field != SyncField::Status {
        return value.to_owned();
    }

    match value {
        "in_progress" => "InProgress".to_owned(),
        "completed" => "Completed".to_owned(),
        "paused" => "Paused".to_owned(),
        "dropped" => "Dropped".to_owned(),
        "planned" => "Planned".to_owned(),
        other => other.to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
fn write_text_sync_applied<W>(
    stdout: &mut W,
    imports: &[sync_core::sync::ProviderSnapshotImportSummary],
    summary: &sync_core::sync::ApplySummary,
    plan: &SyncPlan,
    requests: &[AuthorizedProviderWriteRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
    post_write_verification: bool,
    test_token_transport: bool,
    token_request_count: usize,
) where
    W: Write,
{
    let credential_bundle_codec = credential_bundle_codec_label
        .map(|label| format!(" credential_bundle_codec={label}"))
        .unwrap_or_default();
    let token_transport = if test_token_transport {
        format!(" test_token_transport=true token_request_count={token_request_count}")
    } else {
        String::new()
    };
    let auto_link = if auto_link_local {
        " local_auto_link=true"
    } else {
        ""
    };
    let _ = writeln!(
        stdout,
        "sync-v2 sync: applied local-only fixture_read_transport=true test_write_transport=true live_provider_writes=false post_write_verification={} credential_source={}{}{}{} imports={} actions={} attempted={} succeeded={} failed={} post_write_verified={} post_write_skipped={} write_requests={}",
        post_write_verification,
        credential_source_label,
        credential_bundle_codec,
        auto_link,
        token_transport,
        imports.len(),
        plan.actions().len(),
        summary.attempted,
        summary.succeeded,
        summary.failed,
        summary.post_write_verified,
        summary.post_write_skipped,
        requests.len()
    );
    write_text_import_summaries(stdout, imports);
    write_text_plan_preview(stdout, plan);
    write_text_apply(
        stdout,
        summary,
        plan,
        requests,
        credential_source_label,
        credential_bundle_codec_label,
        auto_link_local,
    );
}

fn write_text_sync_credentials_required<W>(
    stdout: &mut W,
    imports: &[sync_core::sync::ProviderSnapshotImportSummary],
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
    test_token_transport: bool,
    token_request_count: usize,
) where
    W: Write,
{
    let credential_bundle_codec = credential_bundle_codec_label
        .map(|label| format!(" credential_bundle_codec={label}"))
        .unwrap_or_default();
    let token_transport = if test_token_transport {
        format!(" test_token_transport=true token_request_count={token_request_count}")
    } else {
        String::new()
    };
    let auto_link = if auto_link_local {
        " local_auto_link=true"
    } else {
        ""
    };
    let _ = writeln!(
        stdout,
        "sync-v2 sync: credentials_required local-only fixture_read_transport=true test_write_transport=true live_provider_writes=false local_write_journal=false credential_source={}{}{}{} imports={}",
        credential_source_label,
        credential_bundle_codec,
        auto_link,
        token_transport,
        imports.len()
    );
    write_text_import_summaries(stdout, imports);
}

#[allow(clippy::too_many_arguments)]
fn write_json_sync_applied<W>(
    stdout: &mut W,
    imports: &[sync_core::sync::ProviderSnapshotImportSummary],
    summary: &sync_core::sync::ApplySummary,
    plan: &SyncPlan,
    requests: &[AuthorizedProviderWriteRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
    post_write_verification: bool,
    test_token_transport: bool,
    token_request_count: usize,
) where
    W: Write,
{
    let mut report = json!({
        "outcome": "applied",
        "dry_run": false,
        "local_only": true,
        "fixture_read_transport": true,
        "test_write_transport": true,
        "live_provider_writes": false,
        "post_write_verification": post_write_verification,
        "local_snapshot_import": true,
        "local_auto_link": auto_link_local,
        "local_write_journal": true,
        "credential_source": credential_source_label,
        "imports": imports.iter().map(import_summary_to_json).collect::<Vec<_>>(),
        "plan": serde_json::Value::Null,
        "plan_preview": plan_to_json_value(plan),
        "apply": apply_to_json_value(
            summary,
            plan,
            requests,
            credential_source_label,
            credential_bundle_codec_label,
            auto_link_local
        ),
    });
    if let Some(codec_label) = credential_bundle_codec_label {
        report["credential_bundle_codec"] = json!(codec_label);
    }
    if test_token_transport {
        report["test_token_transport"] = json!(true);
        report["token_request_count"] = json!(token_request_count);
    }

    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

#[allow(clippy::too_many_arguments)]
fn write_text_sync_blocked_conflicts<W>(
    stdout: &mut W,
    imports: &[sync_core::sync::ProviderSnapshotImportSummary],
    plan: &SyncPlan,
    conflict_count: usize,
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
    test_token_transport: bool,
    token_request_count: usize,
) where
    W: Write,
{
    let credential_bundle_codec = credential_bundle_codec_label
        .map(|label| format!(" credential_bundle_codec={label}"))
        .unwrap_or_default();
    let token_transport = if test_token_transport {
        format!(" test_token_transport=true token_request_count={token_request_count}")
    } else {
        String::new()
    };
    let auto_link = if auto_link_local {
        " local_auto_link=true"
    } else {
        ""
    };
    let _ = writeln!(
        stdout,
        "sync-v2 sync: blocked_conflicts local-only fixture_read_transport=true test_write_transport=true live_provider_writes=false local_write_journal=false credential_source={}{}{}{} imports={} actions={} conflicts={} apply_blocked=true apply_blocked_reason=conflicts_present",
        credential_source_label,
        credential_bundle_codec,
        auto_link,
        token_transport,
        imports.len(),
        plan.actions().len(),
        conflict_count
    );
    write_text_import_summaries(stdout, imports);
    write_text_plan_preview(stdout, plan);
}

#[allow(clippy::too_many_arguments)]
fn write_json_sync_blocked_conflicts<W>(
    stdout: &mut W,
    imports: &[sync_core::sync::ProviderSnapshotImportSummary],
    plan: &SyncPlan,
    conflict_count: usize,
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
    post_write_verification: bool,
    test_token_transport: bool,
    token_request_count: usize,
) where
    W: Write,
{
    let mut report = json!({
        "outcome": "blocked_conflicts",
        "dry_run": false,
        "local_only": true,
        "fixture_read_transport": true,
        "test_write_transport": true,
        "live_provider_writes": false,
        "post_write_verification": post_write_verification,
        "local_snapshot_import": true,
        "local_auto_link": auto_link_local,
        "local_write_journal": false,
        "credential_source": credential_source_label,
        "blocked_conflicts": conflict_count,
        "imports": imports.iter().map(import_summary_to_json).collect::<Vec<_>>(),
        "plan": serde_json::Value::Null,
        "plan_preview": plan_to_json_value(plan),
        "apply": serde_json::Value::Null,
        "write_requests": serde_json::Value::Null,
    });
    if let Some(codec_label) = credential_bundle_codec_label {
        report["credential_bundle_codec"] = json!(codec_label);
    }
    if test_token_transport {
        report["test_token_transport"] = json!(true);
        report["token_request_count"] = json!(token_request_count);
    }

    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn write_json_sync_credentials_required<W>(
    stdout: &mut W,
    imports: &[sync_core::sync::ProviderSnapshotImportSummary],
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
    test_token_transport: bool,
    token_request_count: usize,
) where
    W: Write,
{
    let mut report = json!({
        "outcome": "credentials_required",
        "dry_run": false,
        "local_only": true,
        "fixture_read_transport": true,
        "test_write_transport": true,
        "live_provider_writes": false,
        "local_snapshot_import": true,
        "local_auto_link": auto_link_local,
        "local_write_journal": false,
        "credential_source": credential_source_label,
        "imports": imports.iter().map(import_summary_to_json).collect::<Vec<_>>(),
        "plan": serde_json::Value::Null,
        "plan_preview": serde_json::Value::Null,
        "apply": serde_json::Value::Null,
    });
    if let Some(codec_label) = credential_bundle_codec_label {
        report["credential_bundle_codec"] = json!(codec_label);
    }
    if test_token_transport {
        report["test_token_transport"] = json!(true);
        report["token_request_count"] = json!(token_request_count);
    }

    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn write_text_apply_blocked_conflicts<W>(
    stdout: &mut W,
    plan: &SyncPlan,
    conflict_count: usize,
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
) where
    W: Write,
{
    let credential_bundle_codec = credential_bundle_codec_label
        .map(|label| format!(" credential_bundle_codec={label}"))
        .unwrap_or_default();
    let auto_link = if auto_link_local {
        " local_auto_link=true"
    } else {
        ""
    };
    let _ = writeln!(
        stdout,
        "sync-v2 apply: blocked_conflicts local-only test_write_transport=true live_provider_writes=false local_write_journal=false credential_source={}{}{} actions={} conflicts={} apply_blocked=true apply_blocked_reason=conflicts_present",
        credential_source_label,
        credential_bundle_codec,
        auto_link,
        plan.actions().len(),
        conflict_count
    );
    write_text_plan_preview_for_command(stdout, "apply", plan);
}

fn write_json_apply_blocked_conflicts<W>(
    stdout: &mut W,
    plan: &SyncPlan,
    conflict_count: usize,
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
) where
    W: Write,
{
    let mut report = json!({
        "outcome": "blocked_conflicts",
        "dry_run": false,
        "local_only": true,
        "test_write_transport": true,
        "live_provider_writes": false,
        "local_auto_link": auto_link_local,
        "local_write_journal": false,
        "credential_source": credential_source_label,
        "blocked_conflicts": conflict_count,
        "plan": serde_json::Value::Null,
        "plan_preview": plan_to_json_value(plan),
        "apply": serde_json::Value::Null,
        "write_requests": serde_json::Value::Null,
    });
    if let Some(codec_label) = credential_bundle_codec_label {
        report["credential_bundle_codec"] = json!(codec_label);
    }

    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn write_text_apply<W>(
    stdout: &mut W,
    summary: &sync_core::sync::ApplySummary,
    plan: &SyncPlan,
    requests: &[AuthorizedProviderWriteRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
) where
    W: Write,
{
    let credential_bundle_codec = credential_bundle_codec_label
        .map(|label| format!(" credential_bundle_codec={label}"))
        .unwrap_or_default();
    let auto_link = if auto_link_local {
        " local_auto_link=true"
    } else {
        ""
    };
    let _ = writeln!(
        stdout,
        "sync-v2 apply: test-write-transport local-only live_provider_writes=false credential_source={}{}{} attempted={} succeeded={} failed={} post_write_verified={} post_write_skipped={} write_requests={}",
        credential_source_label,
        credential_bundle_codec,
        auto_link,
        summary.attempted,
        summary.succeeded,
        summary.failed,
        summary.post_write_verified,
        summary.post_write_skipped,
        requests.len()
    );
    for (index, request) in requests.iter().enumerate() {
        let action = plan.actions().get(index);
        let action_suffix = action
            .map(|action| {
                format!(
                    " kind={} media_kind={} target={} fields={}",
                    planned_action_kind_name(action.kind),
                    action.media_kind.as_str(),
                    action.target_provider_entry_id,
                    action
                        .field_updates
                        .iter()
                        .map(|field| field.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .unwrap_or_default();
        let _ = writeln!(
            stdout,
            "- write {} {} {} content_type={}{}",
            request.request.provider.as_str(),
            provider_write_request_method_name(request.request.method),
            request.request.url,
            request.request.content_type,
            action_suffix
        );
    }
}

fn write_json_apply<W>(
    stdout: &mut W,
    summary: &sync_core::sync::ApplySummary,
    plan: &SyncPlan,
    requests: &[AuthorizedProviderWriteRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
) where
    W: Write,
{
    let report = apply_to_json_value(
        summary,
        plan,
        requests,
        credential_source_label,
        credential_bundle_codec_label,
        auto_link_local,
    );
    let _ = serde_json::to_writer_pretty(&mut *stdout, &report);
    let _ = writeln!(stdout);
}

fn apply_to_json_value(
    summary: &sync_core::sync::ApplySummary,
    plan: &SyncPlan,
    requests: &[AuthorizedProviderWriteRequest],
    credential_source_label: &str,
    credential_bundle_codec_label: Option<&str>,
    auto_link_local: bool,
) -> serde_json::Value {
    let mut report = json!({
        "dry_run": false,
        "local_only": true,
        "live_provider_writes": false,
        "test_write_transport": true,
        "local_auto_link": auto_link_local,
        "local_write_journal": true,
        "credential_source": credential_source_label,
        "summary": {
            "attempted": summary.attempted,
            "succeeded": summary.succeeded,
            "failed": summary.failed,
            "post_write_verified": summary.post_write_verified,
            "post_write_skipped": summary.post_write_skipped,
        },
        "write_requests": requests.iter().enumerate().map(|(index, request)| {
            authorized_write_request_to_json(plan.actions().get(index), request)
        }).collect::<Vec<_>>(),
    });
    if let Some(codec_label) = credential_bundle_codec_label {
        report["credential_bundle_codec"] = json!(codec_label);
    }

    report
}

fn authorized_write_request_to_json(
    action: Option<&PlannedAction>,
    request: &AuthorizedProviderWriteRequest,
) -> serde_json::Value {
    let mut value = json!({
        "provider": request.request.provider.as_str(),
        "method": provider_write_request_method_name(request.request.method),
        "url": request.request.url.as_str(),
        "content_type": request.request.content_type,
        "body_present": true,
        "authorization": "redacted",
    });
    if let Some(action) = action {
        value["kind"] = json!(planned_action_kind_name(action.kind));
        value["work_id"] = json!(action.work_id);
        value["source_provider"] = json!(action.source_provider.as_str());
        value["target_provider"] = json!(action.target_provider.as_str());
        value["target_provider_entry_id"] = json!(action.target_provider_entry_id.as_str());
        value["media_kind"] = json!(action.media_kind.as_str());
        value["field_updates"] = json!(action
            .field_updates
            .iter()
            .map(|field| field.as_str())
            .collect::<Vec<_>>());
    }
    value
}

fn plan_to_json_value(plan: &SyncPlan) -> serde_json::Value {
    let actions = plan
        .actions()
        .iter()
        .map(|action| {
            let kind = match action.kind {
                PlannedActionKind::AddEntry => "add",
                PlannedActionKind::UpdateEntry => "update",
            };
            let field_updates = action
                .field_updates
                .iter()
                .map(|field| field.as_str())
                .collect::<Vec<_>>();
            let field_changes = action
                .field_changes
                .iter()
                .map(|change| {
                    json!({
                        "field": change.field.as_str(),
                        "old": change.old_value.as_deref().unwrap_or("<unset>"),
                        "new": change.new_value.as_str(),
                    })
                })
                .collect::<Vec<_>>();

            json!({
                "kind": kind,
                "work_id": action.work_id,
                "source_provider": action.source_provider.as_str(),
                "target_provider": action.target_provider.as_str(),
                "target_provider_entry_id": action.target_provider_entry_id.as_str(),
                "media_kind": action.media_kind.as_str(),
                "field_updates": field_updates,
                "field_changes": field_changes,
                "reason": action.reason.as_str(),
            })
        })
        .collect::<Vec<_>>();
    let conflicts = plan
        .conflicts()
        .iter()
        .map(|conflict| {
            let values = conflict
                .values
                .iter()
                .map(|value| {
                    json!({
                        "provider": value.provider.as_str(),
                        "value": value.value.as_str(),
                    })
                })
                .collect::<Vec<_>>();

            json!({
                "work_id": conflict.work_id,
                "field": conflict.field.as_str(),
                "values": values,
                "reason": conflict.reason.as_str(),
            })
        })
        .collect::<Vec<_>>();
    let diagnostics = plan
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            json!({
                "work_id": diagnostic.work_id,
                "source_provider": diagnostic.source_provider.as_str(),
                "target_provider": diagnostic.target_provider.as_str(),
                "target_provider_entry_id": diagnostic.target_provider_entry_id.as_str(),
                "media_kind": diagnostic.media_kind.as_str(),
                "field": diagnostic.field.as_str(),
                "reason": diagnostic.reason.as_str(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "dry_run": plan.is_dry_run(),
        "actions": actions,
        "conflicts": conflicts,
        "diagnostics": diagnostics,
        "apply_blocked": plan_apply_blocked(plan),
        "apply_blocked_reason": plan_apply_blocked_reason(plan),
        "apply_blocking_conflicts": plan.conflicts().len(),
        "unmatched": plan.skipped_unmatched_count(),
    })
}

fn plan_apply_blocked(plan: &SyncPlan) -> bool {
    plan_apply_blocked_reason(plan).is_some()
}

fn plan_apply_blocked_reason(plan: &SyncPlan) -> Option<&'static str> {
    if plan.conflicts().is_empty() {
        None
    } else {
        Some("conflicts_present")
    }
}

fn parse_sync_options(args: &[String]) -> Result<SyncOptions, String> {
    let mut options = SyncOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();

        if flag == "--test-write-transport" {
            options.test_write_transport = true;
            index += 1;
            continue;
        }
        if flag == "--verify-post-write" {
            options.verify_post_write = true;
            index += 1;
            continue;
        }
        if flag == "--auto-link-local" {
            options.auto_link_local = true;
            index += 1;
            continue;
        }
        if flag == "--test-token-transport" {
            options.test_token_transport = true;
            index += 1;
            continue;
        }
        if flag == "--dry-run" {
            return Err("dry-run is disabled for sync: --dry-run; use plan --dry-run".to_owned());
        }
        if flag == "--refresh-snapshots" {
            return Err(
                "refresh-snapshots is disabled for sync: --refresh-snapshots; sync refreshes snapshots implicitly from --fixture-response"
                    .to_owned(),
            );
        }
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--open-browser"
                | "--exchange-token"
                | "--client-secret"
                | "--authorization-code"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
                | "--live-read"
                | "--fixture"
        ) {
            return Err(format!(
                "live provider writes and network auth are disabled for sync: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--account" | "--account-id" => options.account_id = Some(value.to_owned()),
            "--kind" => options.media_scope = Some(parse_plan_media_scope(value)?),
            "--providers" => options.providers = Some(parse_provider_list(value)?),
            "--fixture-response" => options
                .fixture_responses
                .push(parse_fixture_response_input(value)?),
            "--test-credential-bundle" => {
                options.test_credential_bundle_path = Some(PathBuf::from(value))
            }
            "--credential-bundle" => options.credential_bundle_path = Some(PathBuf::from(value)),
            "--credential-bundle-passphrase-env" => {
                options.credential_bundle_passphrase_env = Some(value.to_owned())
            }
            "--token-response" => options
                .token_responses
                .push(parse_provider_token_response_input(value)?),
            "--refresh-client-id" => options
                .refresh_client_ids
                .push(parse_provider_scoped_value(value, "--refresh-client-id")?),
            "--refresh-client-secret" => {
                options
                    .refresh_client_secrets
                    .push(parse_provider_scoped_value(
                        value,
                        "--refresh-client-secret",
                    )?)
            }
            "--limit" | "--page-limit" => {
                options.limit = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --limit: {value}"))?;
                if options.limit == 0 {
                    return Err("invalid --limit: must be greater than 0".to_owned());
                }
            }
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("invalid --now unix seconds: {value}"))?,
                )
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => return Err(format!("unexpected argument for sync: {other}")),
        }

        index += 2;
    }

    if !options.test_write_transport {
        return Err("missing --test-write-transport".to_owned());
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --account".to_owned());
    }
    if options.media_scope.is_none() {
        return Err("missing --kind".to_owned());
    }
    if options
        .providers
        .as_ref()
        .map(Vec::is_empty)
        .unwrap_or(true)
    {
        return Err("missing --providers".to_owned());
    }
    if options.fixture_responses.is_empty() {
        return Err("missing --fixture-response".to_owned());
    }
    validate_test_token_transport_options(
        options.test_token_transport,
        has_credential_bundle_source(
            &options.test_credential_bundle_path,
            &options.credential_bundle_path,
        ),
        &options.token_responses,
        &options.refresh_client_ids,
        &options.refresh_client_secrets,
    )?;

    Ok(options)
}

fn parse_apply_options(args: &[String]) -> Result<ApplyOptions, String> {
    let mut options = ApplyOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();

        if flag == "--test-write-transport" {
            options.test_write_transport = true;
            index += 1;
            continue;
        }
        if flag == "--verify-post-write" {
            options.verify_post_write = true;
            index += 1;
            continue;
        }
        if flag == "--auto-link-local" {
            options.auto_link_local = true;
            index += 1;
            continue;
        }
        if flag == "--dry-run" {
            return Err("apply does not accept --dry-run; use plan --dry-run".to_owned());
        }
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--open-browser"
                | "--exchange-token"
                | "--client-secret"
                | "--authorization-code"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
                | "--live-read"
                | "--refresh-snapshots"
                | "--fixture"
        ) {
            return Err(format!(
                "live provider writes and network auth are disabled for apply: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--account" | "--account-id" => options.account_id = Some(value.to_owned()),
            "--kind" => options.media_scope = Some(parse_plan_media_scope(value)?),
            "--providers" => options.providers = Some(parse_provider_list(value)?),
            "--fixture-response" => options
                .fixture_responses
                .push(parse_fixture_response_input(value)?),
            "--test-credential-bundle" => {
                options.test_credential_bundle_path = Some(PathBuf::from(value))
            }
            "--credential-bundle" => options.credential_bundle_path = Some(PathBuf::from(value)),
            "--credential-bundle-passphrase-env" => {
                options.credential_bundle_passphrase_env = Some(value.to_owned())
            }
            "--limit" | "--page-limit" => {
                options.limit = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --limit: {value}"))?;
                if options.limit == 0 {
                    return Err("invalid --limit: must be greater than 0".to_owned());
                }
            }
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("invalid --now unix seconds: {value}"))?,
                )
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => return Err(format!("unexpected argument for apply: {other}")),
        }

        index += 2;
    }

    if !options.test_write_transport {
        return Err("missing --test-write-transport".to_owned());
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --account".to_owned());
    }
    if options.media_scope.is_none() {
        return Err("missing --kind".to_owned());
    }
    if options
        .providers
        .as_ref()
        .map(Vec::is_empty)
        .unwrap_or(true)
    {
        return Err("missing --providers".to_owned());
    }
    if options.verify_post_write && options.fixture_responses.is_empty() {
        return Err("missing --fixture-response".to_owned());
    }
    if !options.verify_post_write && !options.fixture_responses.is_empty() {
        return Err("--fixture-response requires --verify-post-write".to_owned());
    }

    Ok(options)
}

fn parse_plan_options(args: &[String]) -> Result<PlanOptions, String> {
    let mut options = PlanOptions::default();
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();

        if flag == "--dry-run" {
            options.dry_run = true;
            index += 1;
            continue;
        }
        if flag == "--refresh-snapshots" {
            options.refresh_snapshots = true;
            index += 1;
            continue;
        }
        if flag == "--live-read" {
            options.live_read = true;
            index += 1;
            continue;
        }
        if flag == "--auto-link-local" {
            options.auto_link_local = true;
            index += 1;
            continue;
        }
        if flag == "--test-token-transport" {
            options.test_token_transport = true;
            index += 1;
            continue;
        }
        if matches!(
            flag,
            "--apply"
                | "--write"
                | "--delete"
                | "--refresh"
                | "--reauthorize"
                | "--open-browser"
                | "--exchange-token"
                | "--client-secret"
                | "--authorization-code"
                | "--access-token"
                | "--refresh-token"
                | "--cookie"
                | "--session"
                | "--browser-profile"
                | "--remote-debugging-port"
                | "--csrf-token"
        ) {
            if options.refresh_snapshots {
                return Err(format!(
                    "provider writes are disabled for plan refresh-snapshots: {flag}"
                ));
            }
            if matches!(flag, "--apply" | "--write" | "--delete") {
                return Err(format!("writes are disabled for plan: {flag}"));
            }
            return Err(format!(
                "provider writes and network auth are disabled for plan: {flag}"
            ));
        }

        let Some(value) = args.get(index + 1) else {
            return Err(format!("missing value for {flag}"));
        };

        match flag {
            "--db" => options.db_path = Some(PathBuf::from(value)),
            "--account" | "--account-id" => options.account_id = Some(value.to_owned()),
            "--kind" => options.media_scope = Some(parse_plan_media_scope(value)?),
            "--providers" => options.providers = Some(parse_provider_list(value)?),
            "--fixture-response" | "--fixture" => options
                .fixture_responses
                .push(parse_fixture_response_input(value)?),
            "--test-credential-bundle" => {
                options.test_credential_bundle_path = Some(PathBuf::from(value))
            }
            "--credential-bundle" => options.credential_bundle_path = Some(PathBuf::from(value)),
            "--credential-bundle-passphrase-env" => {
                options.credential_bundle_passphrase_env = Some(value.to_owned())
            }
            "--token-response" => options
                .token_responses
                .push(parse_provider_token_response_input(value)?),
            "--refresh-client-id" => options
                .refresh_client_ids
                .push(parse_provider_scoped_value(value, "--refresh-client-id")?),
            "--refresh-client-secret" => {
                options
                    .refresh_client_secrets
                    .push(parse_provider_scoped_value(
                        value,
                        "--refresh-client-secret",
                    )?)
            }
            "--limit" | "--page-limit" => {
                options.limit = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --limit: {value}"))?;
                if options.limit == 0 {
                    return Err("invalid --limit: must be greater than 0".to_owned());
                }
            }
            "--now" => {
                options.now_unix_seconds = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("invalid --now unix seconds: {value}"))?,
                )
            }
            "--format" => options.output_format = parse_plan_output_format(value)?,
            other => return Err(format!("unexpected argument for plan: {other}")),
        }

        index += 2;
    }

    if !options.dry_run {
        return Err("missing --dry-run".to_owned());
    }
    if options.db_path.is_none() {
        return Err("missing --db".to_owned());
    }
    if options
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err("missing --account".to_owned());
    }
    if options.media_scope.is_none() {
        return Err("missing --kind".to_owned());
    }
    if options
        .providers
        .as_ref()
        .map(Vec::is_empty)
        .unwrap_or(true)
    {
        return Err("missing --providers".to_owned());
    }
    if options.live_read && !options.refresh_snapshots {
        return Err("--live-read requires --refresh-snapshots".to_owned());
    }
    if options.refresh_snapshots {
        if options.live_read {
            if !options.fixture_responses.is_empty() {
                return Err("--fixture-response cannot be used with --live-read".to_owned());
            }
            if !has_credential_bundle_source(
                &options.test_credential_bundle_path,
                &options.credential_bundle_path,
            ) {
                return Err(
                    "--live-read requires --test-credential-bundle or --credential-bundle"
                        .to_owned(),
                );
            }
        } else if options.fixture_responses.is_empty() {
            return Err("missing --fixture-response".to_owned());
        }
    }
    if !options.refresh_snapshots {
        if options.live_read {
            return Err("--live-read requires --refresh-snapshots".to_owned());
        }
        if !options.fixture_responses.is_empty() {
            return Err("--fixture-response requires --refresh-snapshots".to_owned());
        }
        if options.test_credential_bundle_path.is_some() {
            return Err("--test-credential-bundle requires --refresh-snapshots".to_owned());
        }
        if options.credential_bundle_path.is_some() {
            return Err("--credential-bundle requires --refresh-snapshots".to_owned());
        }
        if options.credential_bundle_passphrase_env.is_some() {
            return Err(
                "--credential-bundle-passphrase-env requires --refresh-snapshots".to_owned(),
            );
        }
        if options.test_token_transport {
            return Err("--test-token-transport requires --refresh-snapshots".to_owned());
        }
        if options.auto_link_local {
            return Err("--auto-link-local requires --refresh-snapshots".to_owned());
        }
        if !options.token_responses.is_empty() {
            return Err("--token-response requires --refresh-snapshots".to_owned());
        }
        if !options.refresh_client_ids.is_empty() {
            return Err("--refresh-client-id requires --refresh-snapshots".to_owned());
        }
        if !options.refresh_client_secrets.is_empty() {
            return Err("--refresh-client-secret requires --refresh-snapshots".to_owned());
        }
        if options.now_unix_seconds.is_some() {
            return Err("--now requires --refresh-snapshots".to_owned());
        }
        if options.limit != 100 {
            return Err("--limit requires --refresh-snapshots".to_owned());
        }
    }
    validate_test_token_transport_options(
        options.test_token_transport,
        has_credential_bundle_source(
            &options.test_credential_bundle_path,
            &options.credential_bundle_path,
        ),
        &options.token_responses,
        &options.refresh_client_ids,
        &options.refresh_client_secrets,
    )?;

    Ok(options)
}

fn parse_fixture_response_input(value: &str) -> Result<FixtureResponseInput, String> {
    let mut parts = value.splitn(3, ':');
    let provider = parts
        .next()
        .ok_or_else(|| "invalid --fixture-response".to_owned())
        .and_then(parse_provider)?;
    let media_kind = parts
        .next()
        .ok_or_else(|| "invalid --fixture-response".to_owned())
        .and_then(parse_media_kind)?;
    let rest = parts
        .next()
        .ok_or_else(|| "invalid --fixture-response".to_owned())?;

    let (endpoint, path) = if provider == Provider::Bangumi && media_kind == MediaKind::Anime {
        if let Some(episode_rest) = rest.strip_prefix("episodes:") {
            let mut episode_parts = episode_rest.splitn(2, ':');
            let subject_id = episode_parts
                .next()
                .ok_or_else(|| "invalid --fixture-response: missing subject id".to_owned())?;
            let path = episode_parts
                .next()
                .ok_or_else(|| "invalid --fixture-response: missing path".to_owned())?;
            if subject_id.trim().is_empty() {
                return Err("invalid --fixture-response: missing subject id".to_owned());
            }
            (
                FixtureResponseEndpoint::BangumiEpisodeCollection {
                    subject_id: subject_id.to_owned(),
                },
                path,
            )
        } else {
            (FixtureResponseEndpoint::Collection, rest)
        }
    } else {
        (FixtureResponseEndpoint::Collection, rest)
    };

    if path.trim().is_empty() {
        return Err("invalid --fixture-response: missing path".to_owned());
    }

    Ok(FixtureResponseInput {
        provider,
        media_kind,
        endpoint,
        path: PathBuf::from(path),
    })
}

fn parse_provider_token_response_input(value: &str) -> Result<ProviderTokenResponseInput, String> {
    let mut parts = value.splitn(2, ':');
    let provider = parts
        .next()
        .ok_or_else(|| "invalid --token-response; expected <provider>:<path>".to_owned())
        .and_then(parse_provider)?;
    let path = parts
        .next()
        .ok_or_else(|| "invalid --token-response; expected <provider>:<path>".to_owned())?;
    if path.trim().is_empty() {
        return Err("invalid --token-response; path must not be empty".to_owned());
    }

    Ok(ProviderTokenResponseInput {
        provider,
        path: PathBuf::from(path),
    })
}

fn parse_provider_scoped_value(value: &str, flag: &str) -> Result<ProviderScopedValue, String> {
    let mut parts = value.splitn(2, ':');
    let provider = parts
        .next()
        .ok_or_else(|| format!("invalid {flag}; expected <provider>:<value>"))
        .and_then(parse_provider)?;
    let scoped_value = parts
        .next()
        .ok_or_else(|| format!("invalid {flag}; expected <provider>:<value>"))?;
    if scoped_value.trim().is_empty() {
        return Err(format!("invalid {flag}; value must not be empty"));
    }

    Ok(ProviderScopedValue {
        provider,
        value: scoped_value.to_owned(),
    })
}

fn token_response_path_map(inputs: &[ProviderTokenResponseInput]) -> HashMap<Provider, PathBuf> {
    inputs
        .iter()
        .map(|input| (input.provider, input.path.clone()))
        .collect()
}

fn provider_scoped_value(inputs: &[ProviderScopedValue], provider: Provider) -> Option<&str> {
    inputs
        .iter()
        .find(|input| input.provider == provider)
        .map(|input| input.value.as_str())
}

fn build_refresh_configs(
    providers: &[Provider],
    client_ids: &[ProviderScopedValue],
    client_secrets: &[ProviderScopedValue],
) -> Vec<ProviderSyncCredentialRefreshConfig> {
    providers
        .iter()
        .filter_map(|provider| {
            provider_scoped_value(client_ids, *provider).map(|client_id| {
                ProviderSyncCredentialRefreshConfig {
                    provider: *provider,
                    client_id: client_id.to_owned(),
                    client_secret: provider_scoped_value(client_secrets, *provider)
                        .map(ToOwned::to_owned),
                }
            })
        })
        .collect()
}

fn validate_test_token_transport_options(
    test_token_transport: bool,
    has_credential_bundle: bool,
    token_responses: &[ProviderTokenResponseInput],
    refresh_client_ids: &[ProviderScopedValue],
    refresh_client_secrets: &[ProviderScopedValue],
) -> Result<(), String> {
    if !test_token_transport {
        if !token_responses.is_empty() {
            return Err("--token-response requires --test-token-transport".to_owned());
        }
        if !refresh_client_ids.is_empty() {
            return Err("--refresh-client-id requires --test-token-transport".to_owned());
        }
        if !refresh_client_secrets.is_empty() {
            return Err("--refresh-client-secret requires --test-token-transport".to_owned());
        }
        return Ok(());
    }

    if !has_credential_bundle {
        return Err("--test-token-transport requires a credential bundle source".to_owned());
    }

    for token_response in token_responses {
        if provider_scoped_value(refresh_client_ids, token_response.provider).is_none() {
            return Err("--token-response requires matching --refresh-client-id".to_owned());
        }
    }
    for client_secret in refresh_client_secrets {
        if provider_scoped_value(refresh_client_ids, client_secret.provider).is_none() {
            return Err("--refresh-client-secret requires matching --refresh-client-id".to_owned());
        }
    }

    Ok(())
}

fn has_credential_bundle_source(
    test_credential_bundle_path: &Option<PathBuf>,
    credential_bundle_path: &Option<PathBuf>,
) -> bool {
    test_credential_bundle_path.is_some() || credential_bundle_path.is_some()
}

fn parse_plan_output_format(value: &str) -> Result<PlanOutputFormat, String> {
    match value {
        "text" => Ok(PlanOutputFormat::Text),
        "json" => Ok(PlanOutputFormat::Json),
        other => Err(format!("unsupported output format: {other}")),
    }
}

fn parse_plan_media_scope(value: &str) -> Result<PlanMediaScope, String> {
    match value {
        "all" => Ok(PlanMediaScope::All),
        _ => parse_media_kind(value).map(PlanMediaScope::One),
    }
}

fn parse_provider_list(value: &str) -> Result<Vec<Provider>, String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(parse_provider)
        .collect()
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

fn generate_oauth_urlsafe_token(byte_len: usize) -> Result<String, String> {
    let mut bytes = vec![0_u8; byte_len];
    let mut random = std::fs::File::open("/dev/urandom")
        .map_err(|_| "failed to read system random source".to_owned())?;
    random
        .read_exact(&mut bytes)
        .map_err(|_| "failed to read system random source".to_owned())?;
    Ok(base64_url_no_pad(&bytes))
}

fn base64_url_no_pad(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let block = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | bytes[index + 2] as u32;
        output.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
        output.push(TABLE[((block >> 6) & 0x3f) as usize] as char);
        output.push(TABLE[(block & 0x3f) as usize] as char);
        index += 3;
    }
    match bytes.len() - index {
        1 => {
            let block = (bytes[index] as u32) << 16;
            output.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
        }
        2 => {
            let block = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
            output.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
            output.push(TABLE[((block >> 6) & 0x3f) as usize] as char);
        }
        _ => {}
    }
    output
}

fn credential_action_name(action: ProviderCredentialAction) -> &'static str {
    match action {
        ProviderCredentialAction::UseStoredAccessToken => "use",
        ProviderCredentialAction::RefreshWithProvider => "refresh",
        ProviderCredentialAction::Reauthorize { .. } => "reauthorize",
    }
}

fn credential_action_mode_suffix(action: ProviderCredentialAction) -> String {
    match action {
        ProviderCredentialAction::Reauthorize { preferred_mode } => {
            format!(" mode={}", auth_bootstrap_mode_name(preferred_mode))
        }
        _ => String::new(),
    }
}

fn secret_preflight_outcome_name(
    outcome: &StoredProviderCredentialSecretPreflightOutcome,
) -> &'static str {
    match outcome {
        StoredProviderCredentialSecretPreflightOutcome::AccessTokenReadable => {
            "access_secret_readable"
        }
        StoredProviderCredentialSecretPreflightOutcome::RefreshTokenReadable => {
            "refresh_secret_readable"
        }
        StoredProviderCredentialSecretPreflightOutcome::Reauthorize { .. } => {
            "not_checked_reauthorize"
        }
    }
}

fn auth_bootstrap_mode_name(mode: ProviderAuthBootstrapMode) -> &'static str {
    match mode {
        ProviderAuthBootstrapMode::AuthBroker => "auth_broker",
        ProviderAuthBootstrapMode::LocalCallback => "local_callback",
        ProviderAuthBootstrapMode::ManualPin => "manual_pin",
    }
}

fn auth_flow_name(flow: ProviderAuthFlow) -> &'static str {
    match flow {
        ProviderAuthFlow::AuthorizationCode => "authorization_code",
        ProviderAuthFlow::AuthorizationCodePkcePlain => "authorization_code_pkce_plain",
    }
}

fn refresh_token_state_name(state: ProviderRefreshTokenState) -> &'static str {
    match state {
        ProviderRefreshTokenState::Absent => "absent",
        ProviderRefreshTokenState::PresentWithUnknownExpiry => "present_with_unknown_expiry",
        ProviderRefreshTokenState::PresentExpiresAt { .. } => "present_with_expiry",
    }
}

fn credential_refresh_error_kind(error: &StoredCredentialRefreshError) -> &'static str {
    match error {
        StoredCredentialRefreshError::Store(_) => "store_error",
        StoredCredentialRefreshError::Auth(_) => "auth_error",
        StoredCredentialRefreshError::RefreshCommitConflict { .. } => "metadata_commit_conflict",
    }
}

fn provider_write_request_method_name(method: ProviderWriteRequestMethod) -> &'static str {
    match method {
        ProviderWriteRequestMethod::Post => "POST",
        ProviderWriteRequestMethod::Patch => "PATCH",
        ProviderWriteRequestMethod::Put => "PUT",
    }
}

fn planned_action_kind_name(kind: PlannedActionKind) -> &'static str {
    match kind {
        PlannedActionKind::AddEntry => "add",
        PlannedActionKind::UpdateEntry => "update",
    }
}

fn oauth_endpoint_source_name(source: ProviderOAuthEndpointSource) -> &'static str {
    match source {
        ProviderOAuthEndpointSource::OfficialDocs => "official_docs",
        ProviderOAuthEndpointSource::LegacyRepoCode => "legacy_repo_code",
    }
}

fn auto_match_decision_name(decision: &AutoMatchDecision) -> &'static str {
    match decision {
        AutoMatchDecision::Accepted { .. } => "accepted",
        AutoMatchDecision::NeedsReview { .. } => "needs_review",
    }
}

fn auto_match_decision_json(decision: &AutoMatchDecision) -> serde_json::Value {
    match decision {
        AutoMatchDecision::Accepted {
            provider,
            media_kind,
            external_id,
            confidence,
            match_method,
        } => json!({
            "kind": "accepted",
            "provider": provider.as_str(),
            "media_kind": media_kind.as_str(),
            "external_id": external_id.as_str(),
            "confidence": confidence,
            "match_method": match_method.as_str(),
        }),
        AutoMatchDecision::NeedsReview { reason } => json!({
            "kind": "needs_review",
            "reason": reason.as_str(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use sync_core::provider::ProviderHttpHeader;

    use super::*;

    #[test]
    fn cli_provider_http_client_sends_headers_body_and_returns_status_body() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("listener address should load");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("server should accept request");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = stream.read(&mut buffer).expect("request should read");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.ends_with(br#"{"query":"viewer"}"#) {
                    break;
                }
            }
            let request = String::from_utf8(request).expect("request should be utf8");
            assert!(request.starts_with("POST /graphql HTTP/1.1"));
            assert!(request.contains("Authorization: Bearer local-test-token"));
            assert!(request.contains("Content-Type: application/json"));
            assert!(request.contains(r#"{"query":"viewer"}"#));

            stream
                .write_all(
                    b"HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
                )
                .expect("response should write");
        });

        let mut client = CliProviderHttpClient::new();
        let response = client
            .send_provider_http_request(ProviderHttpRequest {
                provider: Provider::AniList,
                method: ProviderReadRequestMethod::Post,
                url: format!("http://{address}/graphql"),
                headers: vec![
                    ProviderHttpHeader::sensitive(
                        "Authorization",
                        "Bearer local-test-token".to_owned(),
                    ),
                    ProviderHttpHeader::public("Content-Type", "application/json"),
                ],
                body: Some(r#"{"query":"viewer"}"#.to_owned()),
            })
            .expect("local HTTP response should succeed");

        assert_eq!(response.status, 202);
        assert_eq!(response.body, r#"{"ok":true}"#);
        server.join().expect("server thread should finish");
    }
}
