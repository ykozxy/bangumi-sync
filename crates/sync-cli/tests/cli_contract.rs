fn run_cli<I, S>(args: I) -> (i32, String, String)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(args, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8(stdout).expect("stdout should be utf8"),
        String::from_utf8(stderr).expect("stderr should be utf8"),
    )
}

#[test]
fn help_lists_read_only_phase_one_commands() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = sync_cli::run(["sync-v2", "--help"], &mut stdout, &mut stderr);

    assert_eq!(code, 0);
    let help = String::from_utf8(stdout).expect("help should be utf8");
    assert!(help.contains("import-fixtures"));
    assert!(help.contains("auth status"));
    assert!(help.contains("auth begin"));
    assert!(help.contains("auth callback-plan"));
    assert!(help.contains("auth complete"));
    assert!(help.contains("--session-file"));
    assert!(help.contains("auth authorize-url"));
    assert!(help.contains("auth exchange-code"));
    assert!(help.contains("auth refresh"));
    assert!(help.contains("auth exchange-code --test-token-transport"));
    assert!(help.contains("--test-token-transport"));
    assert!(help.contains("[--token-response <path>]"));
    assert!(help.contains("--credential-bundle-passphrase-env"));
    assert!(help.contains("--show-sensitive-authorization-url"));
    assert!(help.contains("--format text|json"));
    assert!(help.contains("plan"));
    assert!(help.contains("--refresh-snapshots"));
    assert!(help.contains("--fixture-response"));
    assert!(help.contains("--live-read"));
    assert!(help.contains("--test-credential-bundle"));
    assert!(help.contains("apply --test-write-transport"));
    assert!(help.contains("sync --test-write-transport"));
    assert!(help.contains("--verify-post-write"));
    assert!(help.contains("fixture read transport plus test write transport"));
    assert!(help.contains("inspect-match"));
    assert!(help.contains("dry-run"));
    assert!(help.contains("without browser or network actions"));
    assert!(help.contains("does not open a browser"));
    assert!(help.contains("exchange tokens"));
    assert!(help.contains("write a DB"));
    assert!(
        help.contains("records provider-shaped write attempts only in the local SQLite journal")
    );
    assert!(stderr.is_empty());
}

#[test]
fn plan_live_read_requires_credential_bundle_source() {
    let db_path = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-live-read-missing-credential-{}.sqlite3",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&db_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--live-read",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "anilist",
        "--now",
        "1700000000",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(!db_path.exists());
    assert!(stderr.contains("--live-read requires"));
    assert!(stderr.contains("--credential-bundle"));
}

#[test]
fn plan_live_read_rejects_fixture_responses() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-live-read-fixture-reject-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-live-read-fixture-reject.sqlite3");
    let fixture_path = temp_dir.join("anilist-response.json");
    let bundle_path = temp_dir.join("credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &fixture_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[]}]}}}"#,
    )
    .expect("fixture should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--live-read",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "anilist",
        "--fixture-response",
        &format!("anilist:anime:{}", fixture_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(!db_path.exists());
    assert!(stderr.contains("--fixture-response cannot be used with --live-read"));
}

#[test]
fn plan_live_read_allows_no_fixture_response_before_opening_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-live-read-missing-db-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("missing-live-read.sqlite3");
    let bundle_path = temp_dir.join("not-read-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--live-read",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "anilist",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("missing database"));
    assert!(!stderr.contains("missing --fixture-response"));
    assert!(!bundle_path.exists());
}

#[test]
fn sync_rejects_live_read_without_creating_database() {
    let db_path = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-live-read-reject-{}.sqlite3",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&db_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--live-read",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "anilist",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(!db_path.exists());
    assert!(stderr.contains("live provider writes and network auth are disabled for sync"));
    assert!(stderr.contains("--live-read"));
}

#[test]
fn auth_status_reports_local_credential_preflight_without_secret_reference() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-status.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "keychain:bangumi-sync/secret-ref".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_001_800,
            refresh_token:
                sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "auth",
            "status",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--provider",
            "bangumi",
            "--account",
            "bangumi-user-1",
            "--now",
            "1700000000",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("sync-v2 auth status"));
    assert!(output.contains("provider=bangumi"));
    assert!(output.contains("account=bangumi-user-1"));
    assert!(output.contains("action=refresh"));
    assert!(output.contains("credential=present"));
    assert!(output.contains("read-only"));
    assert!(!output.contains("credential_store_ref"));
    assert!(!output.contains("keychain:"));
    assert!(!output.contains("secret-ref"));
    assert!(!output.contains("access_token"));
    assert!(!output.contains("refresh_token"));
    assert!(!output.contains("token="));
}

#[test]
fn auth_status_reports_reauthorize_for_missing_local_credential() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-missing-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-status-missing.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    let _store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "auth",
            "status",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--provider",
            "anilist",
            "--account",
            "anilist-user-1",
            "--now",
            "1700000000",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("provider=anilist"));
    assert!(output.contains("account=anilist-user-1"));
    assert!(output.contains("action=reauthorize"));
    assert!(output.contains("mode=auth_broker"));
    assert!(output.contains("credential=missing"));
    assert!(!output.contains("credential_store_ref"));
    assert!(!output.contains("access_token"));
    assert!(!output.contains("refresh_token"));
    assert!(!output.contains("token="));
}

#[test]
fn auth_status_rejects_refresh_or_write_flags() {
    for flag in [
        "--apply",
        "--write",
        "--delete",
        "--refresh",
        "--reauthorize",
        "--open-browser",
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = sync_cli::run(
            [
                "sync-v2",
                "auth",
                "status",
                "--db",
                "/tmp/sync-v2.sqlite3",
                "--provider",
                "bangumi",
                "--account",
                "bangumi-user-1",
                flag,
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        let error = String::from_utf8(stderr).expect("error should be utf8");
        assert!(error.contains("disabled for auth status"));
        assert!(error.contains(flag));
    }
}

#[test]
fn auth_status_does_not_create_missing_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-readonly-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("missing-auth-status.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "auth",
            "status",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--provider",
            "bangumi",
            "--account",
            "bangumi-user-1",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(!db_path.exists(), "auth status must not create a db file");
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("failed to open db"));
}

#[test]
fn auth_status_with_test_bundle_preflights_refresh_secret_without_network_or_leaks() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-bundle-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-status-bundle.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let credential_ref = seed_refresh_required_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "auth-status-access-secret",
        "auth-status-refresh-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "status",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth status output should be json");
    assert_eq!(report["provider"], "bangumi");
    assert_eq!(report["account"], "fixture-account");
    assert_eq!(report["action"], "refresh");
    assert_eq!(report["credential"], "present");
    assert_eq!(report["read_only"], true);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["opens_browser"], false);
    assert_eq!(report["network"], false);
    assert_eq!(report["metadata_updated"], false);
    assert_eq!(report["token_request_count"], 0);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["secret_preflight"], "refresh_secret_readable");
    assert!(!stdout.contains("auth-status-access-secret"));
    assert!(!stdout.contains("auth-status-refresh-secret"));
    assert!(!stdout.contains(&credential_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));
}

#[test]
fn auth_status_with_test_bundle_preflights_stored_access_secret_without_network_or_leaks() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-bundle-access-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-status-bundle-access.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let credential_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "auth-status-valid-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "status",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth status output should be json");
    assert_eq!(report["provider"], "bangumi");
    assert_eq!(report["account"], "fixture-account");
    assert_eq!(report["action"], "use");
    assert_eq!(report["credential"], "present");
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["secret_preflight"], "access_secret_readable");
    assert_eq!(report["network"], false);
    assert_eq!(report["metadata_updated"], false);
    assert_eq!(report["token_request_count"], 0);
    assert!(!stdout.contains("auth-status-valid-access-secret"));
    assert!(!stdout.contains("refresh-auth-status-valid-access-secret"));
    assert!(!stdout.contains(&credential_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));
}

#[test]
fn auth_status_with_encrypted_bundle_preflights_refresh_secret_without_network_or_leaks() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-encrypted-bundle-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-status-encrypted-bundle.sqlite3");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let passphrase_env = format!("BANGUMI_SYNC_TEST_STATUS_PASSPHRASE_{}", std::process::id());
    let passphrase = "status encrypted bundle passphrase secret";
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::env::set_var(&passphrase_env, passphrase);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let mut bundle = sync_core::provider::FileCredentialBundleSecretStore::new(
        &bundle_path,
        sync_core::provider::PassphraseCredentialBundleCodec::new(passphrase),
    );
    let credential_ref = sync_core::provider::ProviderCredentialSecretStore::put_provider_tokens(
        &mut bundle,
        sync_core::provider::ProviderCredentialSecretStoreInput {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "encrypted-status-access-secret".to_owned(),
            refresh_token: Some("encrypted-status-refresh-secret".to_owned()),
        },
    )
    .expect("encrypted bundle token should store");
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: credential_ref.clone(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token:
                sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "status",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    std::env::remove_var(&passphrase_env);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth status output should be json");
    assert_eq!(report["provider"], "bangumi");
    assert_eq!(report["account"], "fixture-account");
    assert_eq!(report["action"], "refresh");
    assert_eq!(report["credential_source"], "file_credential_bundle");
    assert_eq!(
        report["credential_bundle_codec"],
        sync_core::provider::PassphraseCredentialBundleCodec::CODEC_LABEL
    );
    assert_eq!(report["secret_preflight"], "refresh_secret_readable");
    assert_eq!(report["network"], false);
    assert_eq!(report["metadata_updated"], false);
    assert_eq!(report["token_request_count"], 0);
    assert!(!stdout.contains("encrypted-status-access-secret"));
    assert!(!stdout.contains("encrypted-status-refresh-secret"));
    assert!(!stdout.contains(&credential_ref));
    assert!(!stdout.contains(passphrase));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));
}

#[test]
fn auth_status_with_test_bundle_skips_secret_read_when_reauthorization_is_required() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-bundle-reauth-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-status-bundle-reauth.sqlite3");
    let missing_bundle_path = temp_dir.join("missing-fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&missing_bundle_path);
    let _store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "status",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "anilist",
        "--account",
        "missing-anilist-user",
        "--test-credential-bundle",
        missing_bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth status output should be json");
    assert_eq!(report["provider"], "anilist");
    assert_eq!(report["account"], "missing-anilist-user");
    assert_eq!(report["action"], "reauthorize");
    assert_eq!(report["mode"], "auth_broker");
    assert_eq!(report["credential"], "missing");
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["secret_preflight"], "not_checked_reauthorize");
    assert_eq!(report["token_request_count"], 0);
    assert!(!stdout.contains(missing_bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!missing_bundle_path.exists());
}

#[test]
fn auth_status_with_test_bundle_skips_secret_read_for_present_reauthorize_credential() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-bundle-present-reauth-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-status-bundle-present-reauth.sqlite3");
    let missing_bundle_path = temp_dir.join("missing-fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&missing_bundle_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider: sync_core::model::Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "bad\nfixture-secret-ref".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: sync_core::provider::ProviderRefreshTokenState::absent(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("reauthorize credential metadata should insert");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "status",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "anilist",
        "--account",
        "anilist-user-1",
        "--test-credential-bundle",
        missing_bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth status output should be json");
    assert_eq!(report["provider"], "anilist");
    assert_eq!(report["account"], "anilist-user-1");
    assert_eq!(report["action"], "reauthorize");
    assert_eq!(report["mode"], "auth_broker");
    assert_eq!(report["credential"], "present");
    assert_eq!(report["secret_preflight"], "not_checked_reauthorize");
    assert_eq!(report["token_request_count"], 0);
    assert!(!stdout.contains("bad"));
    assert!(!stdout.contains("fixture-secret-ref"));
    assert!(!stdout.contains(missing_bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!missing_bundle_path.exists());
}

#[test]
fn auth_status_with_test_bundle_redacts_unreadable_bundle_errors() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-status-bundle-error-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-status-bundle-error.sqlite3");
    let missing_bundle_path = temp_dir.join("missing-fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&missing_bundle_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    seed_refresh_required_cli_credential(
        &store,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "status",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--test-credential-bundle",
        missing_bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to preflight credential secret: auth_error"));
    assert!(!stderr.contains(missing_bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stderr.contains("fixture-secret-ref"));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));
    assert!(!stderr.contains("Authorization"));
    assert!(!stderr.contains("Bearer"));
}

#[test]
fn auth_refresh_test_token_transport_updates_bundle_and_metadata_without_leaking_tokens() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-refresh-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-refresh.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-auth-refresh-access-secret",
            "refresh_token": "rotated-auth-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let old_ref = seed_refresh_required_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "old-auth-refresh-access-secret",
        "old-auth-refresh-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "refresh",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--client-id",
        "bangumi-client",
        "--client-secret",
        "bangumi-client-secret",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth refresh output should be json");
    assert_eq!(report["provider"], "bangumi");
    assert_eq!(report["account"], "fixture-account");
    assert_eq!(report["outcome"], "refreshed");
    assert_eq!(report["local_only"], true);
    assert_eq!(report["test_token_transport"], true);
    assert_eq!(report["opens_browser"], false);
    assert_eq!(report["exchanges_authorization_code"], false);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["metadata_updated"], true);
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["endpoint_source"], "legacy_repo_code");
    assert_eq!(report["expires_at_epoch_secs"], 1_700_003_600);
    assert_eq!(report["refresh_state"], "present_with_unknown_expiry");
    assert!(!stdout.contains("old-auth-refresh-access-secret"));
    assert!(!stdout.contains("old-auth-refresh-secret"));
    assert!(!stdout.contains("new-auth-refresh-access-secret"));
    assert!(!stdout.contains("rotated-auth-refresh-secret"));
    assert!(!stdout.contains("bangumi-client-secret"));
    assert!(!stdout.contains(&old_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::Bangumi, "fixture-account")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_ne!(credential.credential_store_ref, old_ref);
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_003_600);
    assert_eq!(
        credential.refresh_token,
        sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry()
    );
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));

    let mut bundle = sync_core::provider::FileCredentialBundleSecretStore::new(
        &bundle_path,
        CliTestCredentialBundleCodec,
    );
    let token = sync_core::provider::ProviderCredentialAccessTokenStore::get_provider_access_token(
        &mut bundle,
        sync_core::provider::ProviderCredentialSecretLookup {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            credential_store_ref: credential.credential_store_ref,
        },
    )
    .expect("new access token should be readable through refreshed credential ref");
    assert_eq!(token.provider(), sync_core::model::Provider::Bangumi);
    assert!(!format!("{token:?}").contains("new-auth-refresh-access-secret"));
}

#[test]
fn auth_refresh_test_token_transport_updates_encrypted_bundle_without_leaking_tokens() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-refresh-encrypted-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-refresh-encrypted.sqlite3");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_AUTH_REFRESH_PASSPHRASE_{}",
        std::process::id()
    );
    let passphrase = "auth refresh encrypted bundle passphrase secret";
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-encrypted-auth-refresh-access-secret",
            "refresh_token": "rotated-encrypted-auth-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let old_ref = seed_refresh_required_cli_encrypted_bundle_credential(
        &store,
        &bundle_path,
        passphrase,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "old-encrypted-auth-refresh-access-secret",
        "old-encrypted-auth-refresh-secret",
    );
    drop(store);

    std::env::set_var(&passphrase_env, passphrase);
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "refresh",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--client-id",
        "bangumi-client",
        "--client-secret",
        "bangumi-client-secret",
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    std::env::remove_var(&passphrase_env);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth refresh output should be json");
    assert_eq!(report["provider"], "bangumi");
    assert_eq!(report["account"], "fixture-account");
    assert_eq!(report["outcome"], "refreshed");
    assert_eq!(report["credential_source"], "file_credential_bundle");
    assert_eq!(
        report["credential_bundle_codec"],
        "passphrase-argon2id-xchacha20poly1305-v1"
    );
    assert_eq!(report["metadata_updated"], true);
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["expires_at_epoch_secs"], 1_700_003_600);
    assert!(!stdout.contains("old-encrypted-auth-refresh-access-secret"));
    assert!(!stdout.contains("old-encrypted-auth-refresh-secret"));
    assert!(!stdout.contains("new-encrypted-auth-refresh-access-secret"));
    assert!(!stdout.contains("rotated-encrypted-auth-refresh-secret"));
    assert!(!stdout.contains("bangumi-client-secret"));
    assert!(!stdout.contains(passphrase));
    assert!(!stdout.contains(&old_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains(passphrase_env.as_str()));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::Bangumi, "fixture-account")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_ne!(credential.credential_store_ref, old_ref);
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_003_600);
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));

    let mut bundle = sync_core::provider::FileCredentialBundleSecretStore::new(
        &bundle_path,
        sync_core::provider::PassphraseCredentialBundleCodec::new(passphrase),
    );
    let token = sync_core::provider::ProviderCredentialAccessTokenStore::get_provider_access_token(
        &mut bundle,
        sync_core::provider::ProviderCredentialSecretLookup {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            credential_store_ref: credential.credential_store_ref,
        },
    )
    .expect("new access token should be readable through refreshed credential ref");
    assert_eq!(token.provider(), sync_core::model::Provider::Bangumi);
    assert!(!format!("{token:?}").contains("new-encrypted-auth-refresh-access-secret"));
}

#[test]
fn auth_refresh_encrypted_bundle_requires_passphrase_env_before_opening_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-refresh-missing-passphrase-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-refresh-missing-passphrase.sqlite3");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_AUTH_REFRESH_MISSING_PASSPHRASE_{}",
        std::process::id()
    );
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::env::remove_var(&passphrase_env);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "refresh",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--client-id",
        "bangumi-client",
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("missing credential bundle passphrase"));
    assert!(!db_path.exists(), "missing passphrase must not create db");
    assert!(
        !bundle_path.exists(),
        "missing passphrase must not create bundle"
    );
    assert!(!stderr.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stderr.contains(passphrase_env.as_str()));
}

#[test]
fn auth_exchange_code_test_token_transport_installs_bundle_and_metadata_without_leaking_tokens() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-exchange-code-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-exchange-code.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let token_response_path = temp_dir.join("exchange-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-auth-exchange-access-secret",
            "refresh_token": "new-auth-exchange-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "exchange-code",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "myanimelist",
        "--account",
        "mal-user-1",
        "--client-id",
        "mal-client",
        "--client-secret",
        "mal-client-secret",
        "--redirect-uri",
        "http://localhost:3500/callback",
        "--authorization-code",
        "authorization-code-secret",
        "--pkce-code-verifier",
        "plain-code-verifier-123456789012345678901234567890",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth exchange-code output should be json");
    assert_eq!(report["provider"], "myanimelist");
    assert_eq!(report["account"], "mal-user-1");
    assert_eq!(report["outcome"], "installed");
    assert_eq!(report["local_only"], true);
    assert_eq!(report["test_token_transport"], true);
    assert_eq!(report["opens_browser"], false);
    assert_eq!(report["exchanges_authorization_code"], true);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["metadata_updated"], true);
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["endpoint_source"], "official_docs");
    assert_eq!(report["auth_flow"], "authorization_code_pkce_plain");
    assert_eq!(report["bootstrap_mode"], "auth_broker");
    assert_eq!(report["expires_at_epoch_secs"], 1_700_003_600);
    assert_eq!(report["refresh_state"], "present_with_expiry");
    assert!(!stdout.contains("new-auth-exchange-access-secret"));
    assert!(!stdout.contains("new-auth-exchange-refresh-secret"));
    assert!(!stdout.contains("authorization-code-secret"));
    assert!(!stdout.contains("plain-code-verifier"));
    assert!(!stdout.contains("mal-client-secret"));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::MyAnimeList, "mal-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_eq!(
        credential.auth_flow,
        sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain
    );
    assert_eq!(
        credential.bootstrap_mode,
        sync_core::provider::ProviderAuthBootstrapMode::AuthBroker
    );
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_003_600);
    assert_eq!(
        credential.refresh_token,
        sync_core::provider::ProviderRefreshTokenState::present_expires_at(1_702_592_000)
    );
    assert_eq!(credential.last_refresh_at_epoch_secs, None);
    assert_eq!(credential.last_reauth_request_at_epoch_secs, None);

    let mut bundle = sync_core::provider::FileCredentialBundleSecretStore::new(
        &bundle_path,
        CliTestCredentialBundleCodec,
    );
    let token = sync_core::provider::ProviderCredentialAccessTokenStore::get_provider_access_token(
        &mut bundle,
        sync_core::provider::ProviderCredentialSecretLookup {
            provider: sync_core::model::Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            credential_store_ref: credential.credential_store_ref,
        },
    )
    .expect("new access token should be readable through installed credential ref");
    assert_eq!(token.provider(), sync_core::model::Provider::MyAnimeList);
    assert!(!format!("{token:?}").contains("new-auth-exchange-access-secret"));
}

#[test]
fn auth_complete_test_token_transport_consumes_session_callback_and_installs_credentials_without_leaks(
) {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-complete-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-complete.sqlite3");
    let session_path = temp_dir.join("auth-session.json");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let token_response_path = temp_dir.join("complete-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&session_path);
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "new-auth-complete-access-secret",
            "refresh_token": "new-auth-complete-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let (begin_code, _begin_stdout, begin_stderr) = run_cli([
        "sync-v2",
        "auth",
        "begin",
        "--provider",
        "myanimelist",
        "--client-id",
        "mal-client",
        "--redirect-uri",
        "http://localhost:3500/callback",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    assert_eq!(begin_code, 0, "stderr={begin_stderr}");
    let session: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&session_path).expect("session file should exist"),
    )
    .expect("session should parse");
    let state = session["state"].as_str().expect("state should exist");
    let verifier = session["pkce_code_verifier"]
        .as_str()
        .expect("PKCE verifier should exist");
    let callback_url =
        format!("http://localhost:3500/callback?code=authorization-code-secret&state={state}");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "complete",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "mal-user-1",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        callback_url.as_str(),
        "--client-secret",
        "mal-client-secret",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000060",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth complete output should be json");
    assert_eq!(report["provider"], "myanimelist");
    assert_eq!(report["account"], "mal-user-1");
    assert_eq!(report["outcome"], "installed");
    assert_eq!(report["local_only"], true);
    assert_eq!(report["test_token_transport"], true);
    assert_eq!(report["opens_browser"], false);
    assert_eq!(report["uses_auth_session"], true);
    assert_eq!(report["callback_valid"], true);
    assert_eq!(report["exchanges_authorization_code"], true);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["metadata_updated"], true);
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["endpoint_source"], "official_docs");
    assert_eq!(report["auth_flow"], "authorization_code_pkce_plain");
    assert_eq!(report["bootstrap_mode"], "auth_broker");
    assert_eq!(report["expires_at_epoch_secs"], 1_700_003_660);
    assert_eq!(report["refresh_state"], "present_with_expiry");
    assert!(!stdout.contains("new-auth-complete-access-secret"));
    assert!(!stdout.contains("new-auth-complete-refresh-secret"));
    assert!(!stdout.contains("authorization-code-secret"));
    assert!(!stdout.contains(verifier));
    assert!(!stdout.contains(state));
    assert!(!stdout.contains("mal-client-secret"));
    assert!(!stdout.contains(session_path.to_str().expect("utf8 session path")));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::MyAnimeList, "mal-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_eq!(
        credential.auth_flow,
        sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain
    );
    assert_eq!(
        credential.bootstrap_mode,
        sync_core::provider::ProviderAuthBootstrapMode::AuthBroker
    );
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_003_660);

    let mut bundle = sync_core::provider::FileCredentialBundleSecretStore::new(
        &bundle_path,
        CliTestCredentialBundleCodec,
    );
    let token = sync_core::provider::ProviderCredentialAccessTokenStore::get_provider_access_token(
        &mut bundle,
        sync_core::provider::ProviderCredentialSecretLookup {
            provider: sync_core::model::Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            credential_store_ref: credential.credential_store_ref,
        },
    )
    .expect("new access token should be readable through installed credential ref");
    assert_eq!(token.provider(), sync_core::model::Provider::MyAnimeList);
    assert!(!format!("{token:?}").contains("new-auth-complete-access-secret"));
}

#[test]
fn auth_complete_test_token_transport_installs_encrypted_bundle_without_leaks() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-complete-encrypted-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-complete-encrypted.sqlite3");
    let session_path = temp_dir.join("auth-session.json");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let token_response_path = temp_dir.join("complete-response.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_COMPLETE_PASSPHRASE_{}",
        std::process::id()
    );
    let passphrase = "complete encrypted bundle passphrase secret";
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&session_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::env::set_var(&passphrase_env, passphrase);
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "encrypted-complete-access-secret",
            "refresh_token": "encrypted-complete-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let (begin_code, _begin_stdout, begin_stderr) = run_cli([
        "sync-v2",
        "auth",
        "begin",
        "--provider",
        "myanimelist",
        "--client-id",
        "mal-client",
        "--redirect-uri",
        "http://localhost:3500/callback",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    assert_eq!(begin_code, 0, "stderr={begin_stderr}");
    let session: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&session_path).expect("session file should exist"),
    )
    .expect("session should parse");
    let state = session["state"].as_str().expect("state should exist");
    let verifier = session["pkce_code_verifier"]
        .as_str()
        .expect("PKCE verifier should exist");
    let callback_url =
        format!("http://localhost:3500/callback?code=authorization-code-secret&state={state}");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "complete",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "mal-user-1",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        callback_url.as_str(),
        "--client-secret",
        "mal-client-secret",
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000060",
        "--format",
        "json",
    ]);
    std::env::remove_var(&passphrase_env);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth complete output should be json");
    assert_eq!(report["provider"], "myanimelist");
    assert_eq!(report["account"], "mal-user-1");
    assert_eq!(report["outcome"], "installed");
    assert_eq!(report["local_only"], true);
    assert_eq!(report["test_token_transport"], true);
    assert_eq!(report["uses_auth_session"], true);
    assert_eq!(report["callback_valid"], true);
    assert_eq!(report["credential_source"], "file_credential_bundle");
    assert_eq!(
        report["credential_bundle_codec"],
        sync_core::provider::PassphraseCredentialBundleCodec::CODEC_LABEL
    );
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["auth_flow"], "authorization_code_pkce_plain");
    assert!(!stdout.contains("encrypted-complete-access-secret"));
    assert!(!stdout.contains("encrypted-complete-refresh-secret"));
    assert!(!stdout.contains("authorization-code-secret"));
    assert!(!stdout.contains(verifier));
    assert!(!stdout.contains(state));
    assert!(!stdout.contains("mal-client-secret"));
    assert!(!stdout.contains(passphrase));
    assert!(!stdout.contains(session_path.to_str().expect("utf8 session path")));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));

    let raw_file = std::fs::read_to_string(&bundle_path).expect("bundle should exist");
    assert!(raw_file.contains(sync_core::provider::PassphraseCredentialBundleCodec::CODEC_LABEL));
    assert!(!raw_file.contains("encrypted-complete-access-secret"));
    assert!(!raw_file.contains("encrypted-complete-refresh-secret"));
    assert!(!raw_file.contains(passphrase));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::MyAnimeList, "mal-user-1")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    let mut bundle = sync_core::provider::FileCredentialBundleSecretStore::new(
        &bundle_path,
        sync_core::provider::PassphraseCredentialBundleCodec::new(passphrase),
    );
    let token = sync_core::provider::ProviderCredentialAccessTokenStore::get_provider_access_token(
        &mut bundle,
        sync_core::provider::ProviderCredentialSecretLookup {
            provider: sync_core::model::Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            credential_store_ref: credential.credential_store_ref,
        },
    )
    .expect("new access token should be readable through encrypted bundle");
    assert_eq!(token.provider(), sync_core::model::Provider::MyAnimeList);
    assert!(!format!("{token:?}").contains("encrypted-complete-access-secret"));
}

#[test]
fn auth_complete_encrypted_bundle_requires_passphrase_env_before_opening_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-complete-missing-passphrase-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-complete-missing-passphrase.sqlite3");
    let session_path = temp_dir.join("missing-auth-session.json");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_MISSING_PASSPHRASE_{}",
        std::process::id()
    );
    std::env::remove_var(&passphrase_env);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    let callback_url =
        "http://localhost:3500/callback?code=authorization-code-secret&state=state-secret";

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "complete",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "mal-user-1",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        callback_url,
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--token-response",
        "/tmp/token-response-secret.json",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("missing credential bundle passphrase"));
    assert!(!db_path.exists(), "missing passphrase must not create db");
    assert!(
        !bundle_path.exists(),
        "missing passphrase must not create bundle"
    );
    assert!(!stderr.contains(callback_url));
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("state-secret"));
    assert!(!stderr.contains(session_path.to_str().expect("utf8 session path")));
    assert!(!stderr.contains(bundle_path.to_str().expect("utf8 bundle path")));
}

#[test]
fn auth_complete_rejects_invalid_callback_before_opening_database_or_bundle_without_leaks() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-complete-invalid-callback-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-complete-invalid.sqlite3");
    let session_path = temp_dir.join("auth-session.json");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let token_response_path = temp_dir.join("complete-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &session_path,
        r#"{
            "format_version": 1,
            "provider": "anilist",
            "client_id": "anilist-client",
            "redirect_uri": "http://localhost:3499/callback",
            "state": "expected-state-secret",
            "endpoint_source": "official_docs",
            "bootstrap_mode": "auth_broker",
            "created_at_epoch_secs": 1700000000,
            "expires_at_epoch_secs": 1700000600
        }"#,
    )
    .expect("session file should write");
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "unused-auth-complete-access-secret",
            "refresh_token": "unused-auth-complete-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "complete",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "anilist-user-1",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        "http://localhost:3499/callback?code=authorization-code-secret&state=wrong-state-secret",
        "--client-secret",
        "anilist-client-secret",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000060",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to validate auth callback: state_mismatch"));
    assert!(!db_path.exists(), "invalid callback must not create db");
    assert!(
        !bundle_path.exists(),
        "invalid callback must not create bundle"
    );
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("expected-state-secret"));
    assert!(!stderr.contains("wrong-state-secret"));
    assert!(!stderr.contains("anilist-client-secret"));
    assert!(!stderr.contains("unused-auth-complete-access-secret"));
    assert!(!stderr.contains("unused-auth-complete-refresh-secret"));
    assert!(!stderr.contains(session_path.to_str().expect("utf8 session path")));
    assert!(!stderr.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));
}

#[test]
fn auth_complete_parser_errors_do_not_echo_sensitive_arguments() {
    let sensitive_url =
        "http://localhost/callback?code=authorization-code-secret&state=state-secret";
    let sensitive_path = "/tmp/bangumi-sync/auth-session-secret.json";

    for args in [
        vec![
            "sync-v2",
            "auth",
            "complete",
            "--test-token-transport",
            "--session-file",
            sensitive_path,
            sensitive_url,
        ],
        vec![
            "sync-v2",
            "auth",
            "complete",
            "--test-token-transport",
            "--session-file",
            sensitive_path,
            "--callback-url",
            sensitive_url,
            "--now",
            sensitive_url,
        ],
        vec![
            "sync-v2",
            "auth",
            "complete",
            "--test-token-transport",
            "--session-file",
            sensitive_path,
            "--callback-url",
            sensitive_url,
            "--format",
            sensitive_url,
        ],
    ] {
        let (code, stdout, stderr) = run_cli(args);

        assert_eq!(code, 2, "stderr={stderr}");
        assert!(stdout.is_empty());
        assert!(stderr.contains("auth complete"));
        assert!(!stderr.contains(sensitive_url));
        assert!(!stderr.contains("authorization-code-secret"));
        assert!(!stderr.contains("state-secret"));
        assert!(!stderr.contains(sensitive_path));
    }
}

#[test]
fn auth_complete_rejects_live_browser_token_and_override_flags_before_reading_session() {
    for flag in [
        "--apply",
        "--write",
        "--delete",
        "--refresh",
        "--reauthorize",
        "--exchange-token",
        "--open-browser",
        "--provider",
        "--client-id",
        "--redirect-uri",
        "--authorization-code",
        "--pkce-code-verifier",
        "--access-token",
        "--refresh-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
        "--csrf-token",
    ] {
        let temp_dir = std::env::temp_dir().join(format!(
            "bangumi-sync-auth-complete-reject-{}-{}",
            std::process::id(),
            flag.trim_start_matches("--")
        ));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
        let session_path = temp_dir.join("missing-session.json");
        let db_path = temp_dir.join("sync-v2-auth-complete-reject.sqlite3");
        let bundle_path = temp_dir.join("fixture-credential-bundle.json");
        let _ = std::fs::remove_file(&session_path);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&bundle_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "auth",
            "complete",
            "--test-token-transport",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--session-file",
            session_path.to_str().expect("utf8 session path"),
            "--callback-url",
            "http://localhost/callback?code=authorization-code-secret&state=state-secret",
            "--test-credential-bundle",
            bundle_path.to_str().expect("utf8 bundle path"),
            "--token-response",
            "/tmp/token-response-secret.json",
            flag,
            "secret-or-path",
        ]);

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(stderr.contains("disabled for auth complete"));
        assert!(!stderr.contains("authorization-code-secret"));
        assert!(!stderr.contains("state-secret"));
        assert!(!stderr.contains(session_path.to_str().expect("utf8 session path")));
        assert!(!db_path.exists(), "{flag} should fail before opening db");
        assert!(
            !bundle_path.exists(),
            "{flag} should fail before writing bundle"
        );
    }
}

#[test]
fn auth_exchange_code_rolls_back_bundle_entry_when_metadata_persist_fails() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-exchange-code-metadata-failure-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-exchange-code-metadata-failure.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let token_response_path = temp_dir.join("exchange-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "rollback-auth-exchange-access-secret",
            "refresh_token": "rollback-auth-exchange-refresh-secret"
        }"#,
    )
    .expect("token response should write");
    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should create");
    drop(store);
    let connection = rusqlite::Connection::open(&db_path).expect("db should open for trigger");
    connection
        .execute_batch(
            "CREATE TRIGGER fail_provider_credential_insert
             BEFORE INSERT ON provider_credential
             BEGIN
                SELECT RAISE(ABORT, 'forced provider credential failure');
             END;",
        )
        .expect("failure trigger should install");
    drop(connection);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "exchange-code",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "myanimelist",
        "--account",
        "mal-user-1",
        "--client-id",
        "mal-client",
        "--client-secret",
        "mal-client-secret",
        "--redirect-uri",
        "http://localhost:3500/callback",
        "--authorization-code",
        "authorization-code-secret",
        "--pkce-code-verifier",
        "plain-code-verifier-123456789012345678901234567890",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to persist credential metadata"));
    assert!(stderr.contains("store_error"));
    assert!(!stderr.contains("rollback-auth-exchange-access-secret"));
    assert!(!stderr.contains("rollback-auth-exchange-refresh-secret"));
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("mal-client-secret"));
    assert!(!stderr.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stderr.contains("credential_store_ref"));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));

    let bundle_json = std::fs::read_to_string(&bundle_path).expect("bundle should still be valid");
    let envelope: sync_core::provider::CredentialBundleEnvelope =
        serde_json::from_str(&bundle_json).expect("bundle envelope should parse");
    let plaintext: String = envelope.sealed_payload.chars().rev().collect();
    assert!(!plaintext.contains("rollback-auth-exchange-access-secret"));
    assert!(!plaintext.contains("rollback-auth-exchange-refresh-secret"));
    assert!(!plaintext.contains("mal-user-1"));
    assert_eq!(plaintext, r#"{"entries":[]}"#);
}

#[test]
fn auth_exchange_code_rejects_missing_myanimelist_pkce_before_opening_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-exchange-code-missing-pkce-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-exchange-code-missing-pkce.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let token_response_path = temp_dir.join("exchange-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "unused-auth-exchange-access-secret",
            "refresh_token": "unused-auth-exchange-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "exchange-code",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "myanimelist",
        "--account",
        "mal-user-1",
        "--client-id",
        "mal-client",
        "--redirect-uri",
        "http://localhost:3500/callback",
        "--authorization-code",
        "authorization-code-secret",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to build token exchange request"));
    assert!(stderr.contains("auth_error"));
    assert!(
        !db_path.exists(),
        "invalid exchange input must not create db"
    );
    assert!(
        !bundle_path.exists(),
        "invalid exchange input must not create bundle"
    );
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("unused-auth-exchange-access-secret"));
    assert!(!stderr.contains("unused-auth-exchange-refresh-secret"));
    assert!(!stderr.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));
}

#[test]
fn auth_exchange_code_rejects_invalid_myanimelist_pkce_before_opening_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-exchange-code-invalid-pkce-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-exchange-code-invalid-pkce.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let token_response_path = temp_dir.join("exchange-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "unused-auth-exchange-access-secret",
            "refresh_token": "unused-auth-exchange-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "exchange-code",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "myanimelist",
        "--account",
        "mal-user-1",
        "--client-id",
        "mal-client",
        "--redirect-uri",
        "http://localhost:3500/callback",
        "--authorization-code",
        "authorization-code-secret",
        "--pkce-code-verifier",
        "plain-code-verifier-1234567890123456789012!",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to build token exchange request"));
    assert!(stderr.contains("auth_error"));
    assert!(
        !db_path.exists(),
        "invalid exchange input must not create db"
    );
    assert!(
        !bundle_path.exists(),
        "invalid exchange input must not create bundle"
    );
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("unused-auth-exchange-access-secret"));
    assert!(!stderr.contains("unused-auth-exchange-refresh-secret"));
    assert!(!stderr.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));
}

#[test]
fn auth_exchange_code_rejects_live_browser_token_and_write_flags_before_opening_database() {
    for flag in [
        "--apply",
        "--write",
        "--delete",
        "--exchange-token",
        "--open-browser",
        "--access-token",
        "--refresh-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
        "--csrf-token",
    ] {
        let db_path = std::env::temp_dir().join(format!(
            "bangumi-sync-auth-exchange-code-rejects-{}-{}.sqlite3",
            std::process::id(),
            flag.trim_start_matches("--")
        ));
        let _ = std::fs::remove_file(&db_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "auth",
            "exchange-code",
            "--test-token-transport",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--provider",
            "bangumi",
            "--account",
            "fixture-account",
            "--client-id",
            "bangumi-client",
            "--redirect-uri",
            "http://localhost:3500/callback",
            "--authorization-code",
            "authorization-code-secret",
            "--test-credential-bundle",
            "/tmp/does-not-matter-bundle.json",
            "--token-response",
            "/tmp/does-not-matter-response.json",
            flag,
            "secret-or-path",
        ]);

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(
            !db_path.exists(),
            "{flag} should be rejected before opening db"
        );
        assert!(
            stderr.contains("disabled for auth exchange-code"),
            "{flag}: {stderr}"
        );
        assert!(stderr.contains(flag), "{flag}: {stderr}");
        assert!(!stderr.contains("authorization-code-secret"));
        assert!(!stderr.contains("access_token"));
        assert!(!stderr.contains("refresh_token"));
    }
}

#[test]
fn auth_refresh_reauthorize_does_not_read_corrupt_bundle_or_send_token_request() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-refresh-reauth-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-refresh-reauth.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bundle_path,
        "{not-json anilist-auth-refresh-access-secret fixture-secret-ref",
    )
    .expect("corrupt bundle should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider: sync_core::model::Provider::AniList,
            account_id: "fixture-account".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "fixture-secret-ref".to_owned(),
            access_token_expires_at_epoch_secs: 1_701_728_000,
            refresh_token: sync_core::provider::ProviderRefreshTokenState::absent(),
            last_refresh_at_epoch_secs: None,
            last_reauth_request_at_epoch_secs: Some(1_699_999_000),
        })
        .expect("credential metadata should insert");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "refresh",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "anilist",
        "--account",
        "fixture-account",
        "--client-id",
        "anilist-client",
        "--client-secret",
        "anilist-client-secret",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth refresh output should be json");
    assert_eq!(report["provider"], "anilist");
    assert_eq!(report["outcome"], "reauthorize");
    assert_eq!(report["mode"], "auth_broker");
    assert_eq!(report["metadata_updated"], false);
    assert_eq!(report["token_request_count"], 0);
    assert!(!stdout.contains("anilist-auth-refresh-access-secret"));
    assert!(!stdout.contains("fixture-secret-ref"));
    assert!(!stdout.contains("unused-token"));
    assert!(!stdout.contains("anilist-client-secret"));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
}

#[test]
fn auth_refresh_reauthorize_for_missing_credential_does_not_require_token_response() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-refresh-missing-credential-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-refresh-missing-credential.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "refresh",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "missing-fixture-account",
        "--client-id",
        "bangumi-client",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth refresh output should be json");
    assert_eq!(report["provider"], "bangumi");
    assert_eq!(report["account"], "missing-fixture-account");
    assert_eq!(report["outcome"], "reauthorize");
    assert_eq!(report["mode"], "auth_broker");
    assert_eq!(report["metadata_updated"], false);
    assert_eq!(report["token_request_count"], 0);
    assert_eq!(report["endpoint_source"], "none");
    assert!(
        !bundle_path.exists(),
        "bundle should not be read or created"
    );
}

#[test]
fn auth_refresh_required_without_token_response_fails_without_updating_metadata() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-refresh-missing-token-response-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-refresh-missing-token-response.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let credential_ref = "bundle:bangumi:fixture-account:v1-missing-response";
    let plaintext = format!(
        r#"{{
            "entries": [{{
                "credential_store_ref": "{credential_ref}",
                "provider": "bangumi",
                "account_id": "fixture-account",
                "token_type": "Bearer",
                "access_token": "old-auth-refresh-access-secret",
                "refresh_token": "old-auth-refresh-secret"
            }}]
        }}"#
    );
    let envelope = sync_core::provider::CredentialBundleEnvelope {
        format_version: 1,
        codec: "cli-fixture-reversing-v1".to_owned(),
        sealed_payload: plaintext.chars().rev().collect(),
    };
    std::fs::write(
        &bundle_path,
        serde_json::to_string_pretty(&envelope).expect("envelope should encode"),
    )
    .expect("bundle should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: credential_ref.to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token:
                sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "refresh",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--client-id",
        "bangumi-client",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to refresh credential"));
    assert!(stderr.contains("auth_error"));
    assert!(!stderr.contains("old-auth-refresh-access-secret"));
    assert!(!stderr.contains("old-auth-refresh-secret"));
    assert!(!stderr.contains(credential_ref));
    assert!(!stderr.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stderr.contains("credential_store_ref"));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::Bangumi, "fixture-account")
        .expect("credential lookup should work")
        .expect("credential metadata should still exist");
    assert_eq!(credential.credential_store_ref, credential_ref);
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_000_300);
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_699_900_000));
}

#[test]
fn auth_refresh_redacts_token_field_names_on_refresh_errors() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-refresh-error-redaction-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auth-refresh-error-redaction.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let _ = std::fs::remove_file(&db_path);
    let credential_ref = "bundle:bangumi:fixture-account:v1-redaction";
    let plaintext = format!(
        r#"{{
            "entries": [{{
                "credential_store_ref": "{credential_ref}",
                "provider": "bangumi",
                "account_id": "fixture-account",
                "token_type": "Bearer",
                "access_token": "old-auth-refresh-access-secret",
                "refresh_token": "bad\nrefresh-token-secret"
            }}]
        }}"#
    );
    let envelope = sync_core::provider::CredentialBundleEnvelope {
        format_version: 1,
        codec: "cli-fixture-reversing-v1".to_owned(),
        sealed_payload: plaintext.chars().rev().collect(),
    };
    std::fs::write(
        &bundle_path,
        serde_json::to_string_pretty(&envelope).expect("envelope should encode"),
    )
    .expect("bundle should write");
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 3600,
            "access_token": "unused-new-access-secret",
            "refresh_token": "unused-rotated-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: credential_ref.to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token:
                sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "refresh",
        "--test-token-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--provider",
        "bangumi",
        "--account",
        "fixture-account",
        "--client-id",
        "bangumi-client",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--token-response",
        token_response_path.to_str().expect("utf8 response path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to refresh credential"));
    assert!(stderr.contains("auth_error"));
    assert!(!stderr.contains("old-auth-refresh-access-secret"));
    assert!(!stderr.contains("bad"));
    assert!(!stderr.contains("refresh-token-secret"));
    assert!(!stderr.contains("unused-new-access-secret"));
    assert!(!stderr.contains("unused-rotated-refresh-secret"));
    assert!(!stderr.contains(credential_ref));
    assert!(!stderr.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stderr.contains("credential_store_ref"));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));
}

#[test]
fn auth_refresh_rejects_live_browser_token_and_write_flags_before_opening_database() {
    for flag in [
        "--apply",
        "--write",
        "--delete",
        "--open-browser",
        "--exchange-token",
        "--authorization-code",
        "--access-token",
        "--refresh-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
        "--csrf-token",
    ] {
        let db_path = std::env::temp_dir().join(format!(
            "bangumi-sync-auth-refresh-rejects-{}-{}.sqlite3",
            std::process::id(),
            flag.trim_start_matches("--")
        ));
        let _ = std::fs::remove_file(&db_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "auth",
            "refresh",
            "--test-token-transport",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--provider",
            "bangumi",
            "--account",
            "fixture-account",
            "--client-id",
            "bangumi-client",
            "--test-credential-bundle",
            "/tmp/does-not-matter-bundle.json",
            "--token-response",
            "/tmp/does-not-matter-response.json",
            flag,
            "secret-or-path",
        ]);

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(
            !db_path.exists(),
            "{flag} should be rejected before opening db"
        );
        assert!(
            stderr.contains("disabled for auth refresh"),
            "{flag}: {stderr}"
        );
        assert!(stderr.contains(flag), "{flag}: {stderr}");
    }
}

#[test]
fn auth_begin_writes_local_session_without_printing_authorization_secrets_by_default() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-begin-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    let db_path = temp_dir.join("must-not-create.sqlite3");
    let _ = std::fs::remove_file(&session_path);
    let _ = std::fs::remove_file(&db_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "begin",
        "--provider",
        "anilist",
        "--client-id",
        "anilist-client",
        "--redirect-uri",
        "http://localhost:3499/callback",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    assert!(!db_path.exists(), "auth begin must not create a db");
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth begin output should be json");
    assert_eq!(report["provider"], "anilist");
    assert_eq!(report["local_only"], true);
    assert_eq!(report["opens_browser"], false);
    assert_eq!(report["network"], false);
    assert_eq!(report["exchanges_token"], false);
    assert_eq!(report["writes_database"], false);
    assert_eq!(report["session_file_written"], true);
    assert_eq!(report["endpoint_source"], "official_docs");
    assert_eq!(report["bootstrap_mode"], "auth_broker");
    assert_eq!(report["sensitive_url"], true);
    assert_eq!(report["authorization_url_suppressed"], true);
    assert!(report.get("authorization_url").is_none());
    assert_eq!(report["created_at_epoch_secs"], 1_700_000_000);
    assert_eq!(report["expires_at_epoch_secs"], 1_700_000_600);
    assert!(!stdout.contains(session_path.to_str().expect("utf8 session path")));
    assert!(!stdout.contains("\"authorization_url\":"));
    assert!(!stdout.contains("pkce_code_verifier"));
    assert!(!stdout.contains("client_secret"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));

    let session: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&session_path).expect("session file should exist"),
    )
    .expect("session file should be json");
    assert_eq!(session["format_version"], 1);
    assert_eq!(session["provider"], "anilist");
    assert_eq!(session["client_id"], "anilist-client");
    assert_eq!(session["redirect_uri"], "http://localhost:3499/callback");
    assert_eq!(session["endpoint_source"], "official_docs");
    assert_eq!(session["bootstrap_mode"], "auth_broker");
    assert_eq!(session["created_at_epoch_secs"], 1_700_000_000);
    assert_eq!(session["expires_at_epoch_secs"], 1_700_000_600);
    assert!(session.get("pkce_code_verifier").is_none());
    let state = session["state"].as_str().expect("state should be present");
    assert!(state.len() >= 32);
    assert!(!stdout.contains(state));
    assert!(!stdout.contains("anilist-client"));
    assert!(!stdout.contains("http://localhost:3499/callback"));
}

#[test]
fn auth_begin_can_explicitly_print_sensitive_authorization_url() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-begin-show-url-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    let _ = std::fs::remove_file(&session_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "begin",
        "--show-sensitive-authorization-url",
        "--provider",
        "anilist",
        "--client-id",
        "anilist-client",
        "--redirect-uri",
        "http://localhost:3499/callback",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth begin output should be json");
    assert_eq!(report["authorization_url_suppressed"], false);
    let url = report["authorization_url"]
        .as_str()
        .expect("authorization url should be present");
    assert!(url.starts_with("https://anilist.co/api/v2/oauth/authorize?"));

    let session: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&session_path).expect("session file should exist"),
    )
    .expect("session file should be json");
    let state = session["state"].as_str().expect("state should be present");
    assert!(url.contains(&format!("state={state}")));
    assert!(url.contains("client_id=anilist-client"));
    assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3499%2Fcallback"));
}

#[test]
fn auth_begin_generates_myanimelist_pkce_session_and_suppresses_url_by_default() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-begin-mal-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session-mal.json");
    let _ = std::fs::remove_file(&session_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "begin",
        "--provider",
        "myanimelist",
        "--client-id",
        "mal-client",
        "--redirect-uri",
        "http://localhost:3500/callback",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth begin output should be json");
    assert_eq!(report["provider"], "myanimelist");
    assert_eq!(report["endpoint_source"], "official_docs");
    assert_eq!(report["sensitive_url"], true);
    assert_eq!(report["authorization_url_suppressed"], true);
    assert!(report.get("authorization_url").is_none());
    assert!(!stdout.contains("\"authorization_url\":"));
    assert!(!stdout.contains("pkce_code_verifier"));
    assert!(!stdout.contains(session_path.to_str().expect("utf8 session path")));

    let session: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&session_path).expect("session file should exist"),
    )
    .expect("session file should be json");
    let verifier = session["pkce_code_verifier"]
        .as_str()
        .expect("MAL session should store PKCE verifier");
    assert!((43..=128).contains(&verifier.len()));
    assert!(verifier
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~')));
    assert!(!stdout.contains(verifier));
    assert!(!stdout.contains("code_challenge"));
}

#[test]
fn auth_begin_omits_pkce_for_bangumi_and_anilist() {
    for provider in ["bangumi", "anilist"] {
        let temp_dir = std::env::temp_dir().join(format!(
            "bangumi-sync-auth-begin-no-pkce-{provider}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
        let session_path = temp_dir.join("auth-session.json");
        let _ = std::fs::remove_file(&session_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "auth",
            "begin",
            "--provider",
            provider,
            "--client-id",
            "client",
            "--redirect-uri",
            "http://localhost:3499/callback",
            "--session-file",
            session_path.to_str().expect("utf8 session path"),
            "--format",
            "json",
        ]);

        assert_eq!(code, 0, "{provider} stderr={stderr}");
        assert!(stderr.is_empty());
        assert!(!stdout.contains("code_challenge"));
        let session: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&session_path).expect("session file should exist"),
        )
        .expect("session file should be json");
        assert_eq!(session["provider"], provider);
        assert!(session.get("pkce_code_verifier").is_none());
    }
}

#[test]
fn auth_begin_rejects_browser_network_db_token_and_write_flags_without_writing_session() {
    for flag in [
        "--open-browser",
        "--exchange-token",
        "--client-secret",
        "--authorization-code",
        "--access-token",
        "--refresh-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
        "--csrf-token",
        "--refresh",
        "--reauthorize",
        "--write",
        "--apply",
        "--delete",
        "--db",
        "--test-credential-bundle",
        "--token-response",
    ] {
        let temp_dir = std::env::temp_dir().join(format!(
            "bangumi-sync-auth-begin-reject-{}-{}",
            std::process::id(),
            flag.trim_start_matches("--")
        ));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
        let session_path = temp_dir.join("auth-session.json");
        let _ = std::fs::remove_file(&session_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "auth",
            "begin",
            "--provider",
            "anilist",
            "--client-id",
            "client",
            "--redirect-uri",
            "http://localhost:3499/callback",
            "--session-file",
            session_path.to_str().expect("utf8 session path"),
            flag,
            "secret-or-path",
        ]);

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(
            !session_path.exists(),
            "{flag} should be rejected before writing session"
        );
        assert!(
            stderr.contains("disabled for auth begin"),
            "{flag}: {stderr}"
        );
        assert!(stderr.contains(flag), "{flag}: {stderr}");
    }
}

#[test]
fn auth_begin_refuses_to_overwrite_existing_session_file() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-begin-overwrite-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    std::fs::write(&session_path, "existing-session").expect("existing session should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "begin",
        "--provider",
        "anilist",
        "--client-id",
        "client",
        "--redirect-uri",
        "http://localhost:3499/callback",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to write auth session"));
    assert!(!stderr.contains(session_path.to_str().expect("utf8 session path")));
    assert_eq!(
        std::fs::read_to_string(&session_path).expect("session should remain"),
        "existing-session"
    );
}

#[cfg(unix)]
#[test]
fn auth_begin_writes_owner_only_session_file_permissions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-begin-permissions-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    let _ = std::fs::remove_file(&session_path);

    let (code, _stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "begin",
        "--provider",
        "anilist",
        "--client-id",
        "client",
        "--redirect-uri",
        "http://localhost:3499/callback",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(&session_path)
            .expect("session metadata should exist")
            .permissions(),
    ) & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn auth_callback_plan_validates_session_and_callback_without_leaking_code_or_pkce() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-callback-plan-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session-mal.json");
    let _ = std::fs::remove_file(&session_path);

    let (begin_code, _begin_stdout, begin_stderr) = run_cli([
        "sync-v2",
        "auth",
        "begin",
        "--provider",
        "myanimelist",
        "--client-id",
        "mal-client",
        "--redirect-uri",
        "http://localhost:3500/callback",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    assert_eq!(begin_code, 0, "stderr={begin_stderr}");
    let session: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&session_path).expect("session file should exist"),
    )
    .expect("session should parse");
    let state = session["state"].as_str().expect("state should exist");
    let verifier = session["pkce_code_verifier"]
        .as_str()
        .expect("PKCE verifier should exist");

    let callback_url =
        format!("http://localhost:3500/callback?code=authorization-code-secret&state={state}");
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "callback-plan",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        callback_url.as_str(),
        "--now",
        "1700000060",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("callback-plan output should be json");
    assert_eq!(report["provider"], "myanimelist");
    assert_eq!(report["local_only"], true);
    assert_eq!(report["opens_browser"], false);
    assert_eq!(report["network"], false);
    assert_eq!(report["exchanges_token"], false);
    assert_eq!(report["writes_database"], false);
    assert_eq!(report["state_valid"], true);
    assert_eq!(report["authorization_code_present"], true);
    assert_eq!(report["pkce_required"], true);
    assert_eq!(report["pkce_available"], true);
    assert_eq!(report["ready_for_exchange"], true);
    assert_eq!(report["endpoint_source"], "official_docs");
    assert_eq!(report["bootstrap_mode"], "auth_broker");
    assert_eq!(report["expires_at_epoch_secs"], 1_700_000_600);
    assert!(!stdout.contains("authorization-code-secret"));
    assert!(!stdout.contains(verifier));
    assert!(!stdout.contains(state));
    assert!(!stdout.contains(session_path.to_str().expect("utf8 session path")));
    assert!(!stdout.contains("pkce_code_verifier"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
}

#[test]
fn auth_callback_plan_rejects_state_mismatch_without_leaking_callback_values() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-callback-plan-state-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    std::fs::write(
        &session_path,
        r#"{
            "format_version": 1,
            "provider": "anilist",
            "client_id": "anilist-client",
            "redirect_uri": "http://localhost:3499/callback",
            "state": "expected-state-secret",
            "endpoint_source": "official_docs",
            "bootstrap_mode": "auth_broker",
            "created_at_epoch_secs": 1700000000,
            "expires_at_epoch_secs": 1700000600
        }"#,
    )
    .expect("session file should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "callback-plan",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        "http://localhost:3499/callback?code=authorization-code-secret&state=wrong-state-secret",
        "--now",
        "1700000060",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to validate auth callback: state_mismatch"));
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("expected-state-secret"));
    assert!(!stderr.contains("wrong-state-secret"));
    assert!(!stderr.contains(session_path.to_str().expect("utf8 session path")));
}

#[test]
fn auth_callback_plan_rejects_expired_session_without_leaking_session_material() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-callback-plan-expired-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    std::fs::write(
        &session_path,
        r#"{
            "format_version": 1,
            "provider": "bangumi",
            "client_id": "bangumi-client",
            "redirect_uri": "http://localhost:3498/callback",
            "state": "expected-state-secret",
            "endpoint_source": "legacy_repo_code",
            "bootstrap_mode": "auth_broker",
            "created_at_epoch_secs": 1700000000,
            "expires_at_epoch_secs": 1700000010
        }"#,
    )
    .expect("session file should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "callback-plan",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        "http://localhost:3498/callback?code=authorization-code-secret&state=expected-state-secret",
        "--now",
        "1700000060",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to validate auth callback: session_expired"));
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("expected-state-secret"));
}

#[test]
fn auth_callback_plan_treats_exact_expiry_time_as_expired() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-callback-plan-expiry-boundary-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    std::fs::write(
        &session_path,
        r#"{
            "format_version": 1,
            "provider": "bangumi",
            "client_id": "bangumi-client",
            "redirect_uri": "http://localhost:3498/callback",
            "state": "expected-state-secret",
            "endpoint_source": "legacy_repo_code",
            "bootstrap_mode": "auth_broker",
            "created_at_epoch_secs": 1700000000,
            "expires_at_epoch_secs": 1700000600
        }"#,
    )
    .expect("session file should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "callback-plan",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        "code=authorization-code-secret&state=expected-state-secret",
        "--now",
        "1700000600",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to validate auth callback: session_expired"));
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("expected-state-secret"));
}

#[test]
fn auth_callback_plan_rejects_duplicate_sensitive_callback_parameters() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-callback-plan-duplicate-param-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    std::fs::write(
        &session_path,
        r#"{
            "format_version": 1,
            "provider": "anilist",
            "client_id": "anilist-client",
            "redirect_uri": "http://localhost:3499/callback",
            "state": "expected-state-secret",
            "endpoint_source": "official_docs",
            "bootstrap_mode": "auth_broker",
            "created_at_epoch_secs": 1700000000,
            "expires_at_epoch_secs": 1700000600
        }"#,
    )
    .expect("session file should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "callback-plan",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        "code=authorization-code-secret&state=wrong-state-secret&state=expected-state-secret",
        "--now",
        "1700000060",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to validate auth callback: callback_query_invalid"));
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("expected-state-secret"));
    assert!(!stderr.contains("wrong-state-secret"));
}

#[test]
fn auth_callback_plan_rejects_invalid_session_metadata_before_output() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-callback-plan-session-metadata-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    std::fs::write(
        &session_path,
        r#"{
            "format_version": 1,
            "provider": "anilist",
            "client_id": "anilist-client",
            "redirect_uri": "http://localhost:3499/callback",
            "state": "expected-state-secret",
            "endpoint_source": "unexpected-source-secret",
            "bootstrap_mode": "unexpected-mode-secret",
            "created_at_epoch_secs": 1700000000,
            "expires_at_epoch_secs": 1700000600
        }"#,
    )
    .expect("session file should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "callback-plan",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        "code=authorization-code-secret&state=expected-state-secret",
        "--now",
        "1700000060",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to validate auth callback: session_invalid"));
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("expected-state-secret"));
    assert!(!stderr.contains("unexpected-source-secret"));
    assert!(!stderr.contains("unexpected-mode-secret"));
}

#[test]
fn auth_callback_plan_rejects_invalid_myanimelist_pkce_session() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-callback-plan-pkce-session-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    std::fs::write(
        &session_path,
        r#"{
            "format_version": 1,
            "provider": "myanimelist",
            "client_id": "mal-client",
            "redirect_uri": "http://localhost:3500/callback",
            "state": "expected-state-secret",
            "pkce_code_verifier": "short-secret",
            "endpoint_source": "official_docs",
            "bootstrap_mode": "auth_broker",
            "created_at_epoch_secs": 1700000000,
            "expires_at_epoch_secs": 1700000600
        }"#,
    )
    .expect("session file should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "callback-plan",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        "code=authorization-code-secret&state=expected-state-secret",
        "--now",
        "1700000060",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to validate auth callback: session_invalid"));
    assert!(!stderr.contains("authorization-code-secret"));
    assert!(!stderr.contains("expected-state-secret"));
    assert!(!stderr.contains("short-secret"));
}

#[test]
fn auth_callback_plan_reports_provider_callback_error_without_leaking_description() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auth-callback-plan-error-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let session_path = temp_dir.join("auth-session.json");
    std::fs::write(
        &session_path,
        r#"{
            "format_version": 1,
            "provider": "anilist",
            "client_id": "anilist-client",
            "redirect_uri": "http://localhost:3499/callback",
            "state": "expected-state-secret",
            "endpoint_source": "official_docs",
            "bootstrap_mode": "auth_broker",
            "created_at_epoch_secs": 1700000000,
            "expires_at_epoch_secs": 1700000600
        }"#,
    )
    .expect("session file should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "callback-plan",
        "--session-file",
        session_path.to_str().expect("utf8 session path"),
        "--callback-url",
        "http://localhost:3499/callback?error=access_denied&error_description=user%20denied%20secret&state=expected-state-secret",
        "--now",
        "1700000060",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to validate auth callback: provider_error"));
    assert!(!stderr.contains("user denied secret"));
    assert!(!stderr.contains("expected-state-secret"));
}

#[test]
fn auth_callback_plan_rejects_browser_network_db_token_and_write_flags_without_reading_session() {
    for flag in [
        "--open-browser",
        "--exchange-token",
        "--client-secret",
        "--access-token",
        "--refresh-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
        "--csrf-token",
        "--refresh",
        "--reauthorize",
        "--write",
        "--apply",
        "--delete",
        "--db",
        "--test-credential-bundle",
        "--token-response",
    ] {
        let temp_dir = std::env::temp_dir().join(format!(
            "bangumi-sync-auth-callback-plan-reject-{}-{}",
            std::process::id(),
            flag.trim_start_matches("--")
        ));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
        let session_path = temp_dir.join("missing-session.json");
        let _ = std::fs::remove_file(&session_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "auth",
            "callback-plan",
            "--session-file",
            session_path.to_str().expect("utf8 session path"),
            "--callback-url",
            "http://localhost/callback?code=authorization-code-secret&state=state-secret",
            flag,
            "secret-or-path",
        ]);

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(stderr.contains("disabled for auth callback-plan"));
        assert!(!stderr.contains("authorization-code-secret"));
        assert!(!stderr.contains("state-secret"));
        assert!(
            !session_path.exists(),
            "{flag} should fail before reading or writing session"
        );
    }
}

#[test]
fn auth_callback_plan_parser_errors_do_not_echo_sensitive_arguments() {
    let sensitive_url =
        "http://localhost/callback?code=authorization-code-secret&state=state-secret";
    let sensitive_path = "/tmp/bangumi-sync/auth-session-secret.json";

    for args in [
        vec![
            "sync-v2",
            "auth",
            "callback-plan",
            "--session-file",
            sensitive_path,
            sensitive_url,
        ],
        vec![
            "sync-v2",
            "auth",
            "callback-plan",
            "--session-file",
            sensitive_path,
            "--callback-url",
            sensitive_url,
            sensitive_url,
        ],
        vec![
            "sync-v2",
            "auth",
            "callback-plan",
            "--session-file",
            sensitive_path,
            "--callback-url",
            sensitive_url,
            "--now",
            sensitive_url,
        ],
        vec![
            "sync-v2",
            "auth",
            "callback-plan",
            "--session-file",
            sensitive_path,
            "--callback-url",
            sensitive_url,
            "--format",
            sensitive_url,
        ],
    ] {
        let (code, stdout, stderr) = run_cli(args);

        assert_eq!(code, 2, "stderr={stderr}");
        assert!(stdout.is_empty());
        assert!(stderr.contains("auth callback-plan"));
        assert!(!stderr.contains(sensitive_url));
        assert!(!stderr.contains("authorization-code-secret"));
        assert!(!stderr.contains("state-secret"));
        assert!(!stderr.contains(sensitive_path));
    }
}

#[test]
fn auth_authorize_url_suppresses_sensitive_url_by_default() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = sync_cli::run(
        [
            "sync-v2",
            "auth",
            "authorize-url",
            "--provider",
            "myanimelist",
            "--client-id",
            "mal-client",
            "--redirect-uri",
            "http://localhost:3500/callback",
            "--state",
            "csrf-state",
            "--pkce-code-challenge",
            "plain-code-verifier-123456789012345678901234567890",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output: serde_json::Value =
        serde_json::from_slice(&stdout).expect("authorize-url output should be json");
    assert_eq!(output["provider"], "myanimelist");
    assert_eq!(output["local_only"], true);
    assert_eq!(output["opens_browser"], false);
    assert_eq!(output["exchanges_token"], false);
    assert_eq!(output["endpoint_source"], "official_docs");
    assert_eq!(output["sensitive_url"], true);
    assert_eq!(output["authorization_url_suppressed"], true);
    assert!(output.get("authorization_url").is_none());
    assert!(!output.to_string().contains("csrf-state"));
    assert!(!output
        .to_string()
        .contains("plain-code-verifier-123456789012345678901234567890"));
    assert!(!output.to_string().contains("access_token"));
    assert!(!output.to_string().contains("refresh_token"));
}

#[test]
fn auth_authorize_url_can_explicitly_report_sensitive_oauth_url_as_json() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = sync_cli::run(
        [
            "sync-v2",
            "auth",
            "authorize-url",
            "--show-sensitive-authorization-url",
            "--provider",
            "myanimelist",
            "--client-id",
            "mal-client",
            "--redirect-uri",
            "http://localhost:3500/callback",
            "--state",
            "csrf-state",
            "--pkce-code-challenge",
            "plain-code-verifier-123456789012345678901234567890",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output: serde_json::Value =
        serde_json::from_slice(&stdout).expect("authorize-url output should be json");
    assert_eq!(output["provider"], "myanimelist");
    assert_eq!(output["local_only"], true);
    assert_eq!(output["opens_browser"], false);
    assert_eq!(output["exchanges_token"], false);
    assert_eq!(output["endpoint_source"], "official_docs");
    assert_eq!(output["sensitive_url"], true);
    assert_eq!(output["authorization_url_suppressed"], false);
    let url = output["authorization_url"]
        .as_str()
        .expect("authorization url should be present");
    assert!(url.starts_with("https://myanimelist.net/v1/oauth2/authorize?"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("client_id=mal-client"));
    assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3500%2Fcallback"));
    assert!(url.contains("state=csrf-state"));
    assert!(url.contains("code_challenge_method=plain"));
    assert!(url.contains("code_challenge=plain-code-verifier-123456789012345678901234567890"));
    assert!(!url.contains("client_secret"));
    assert!(!output.to_string().contains("access_token"));
    assert!(!output.to_string().contains("refresh_token"));
}

#[test]
fn auth_authorize_url_reports_provider_specific_json_urls() {
    for (provider, expected_prefix, expected_source) in [
        (
            "bangumi",
            "https://bgm.tv/oauth/authorize?",
            "legacy_repo_code",
        ),
        (
            "anilist",
            "https://anilist.co/api/v2/oauth/authorize?",
            "official_docs",
        ),
        (
            "myanimelist",
            "https://myanimelist.net/v1/oauth2/authorize?",
            "official_docs",
        ),
    ] {
        let mut args = vec![
            "sync-v2".to_owned(),
            "auth".to_owned(),
            "authorize-url".to_owned(),
            "--provider".to_owned(),
            provider.to_owned(),
            "--client-id".to_owned(),
            format!("{provider}-client"),
            "--redirect-uri".to_owned(),
            "http://localhost:3499/callback".to_owned(),
            "--state".to_owned(),
            "csrf-state".to_owned(),
            "--show-sensitive-authorization-url".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ];
        if provider == "myanimelist" {
            args.extend([
                "--pkce-code-challenge".to_owned(),
                "plain-code-verifier-123456789012345678901234567890".to_owned(),
            ]);
        }

        let (code, stdout, stderr) = run_cli(args);

        assert_eq!(code, 0, "{provider} stderr={stderr}");
        assert!(stderr.is_empty());
        let output: serde_json::Value =
            serde_json::from_str(&stdout).expect("authorize-url output should be json");
        assert_eq!(output["provider"], provider);
        assert_eq!(output["local_only"], true);
        assert_eq!(output["opens_browser"], false);
        assert_eq!(output["exchanges_token"], false);
        assert_eq!(output["endpoint_source"], expected_source);
        assert_eq!(output["sensitive_url"], true);
        assert_eq!(output["authorization_url_suppressed"], false);
        let url = output["authorization_url"]
            .as_str()
            .expect("authorization url should be present");
        assert!(url.starts_with(expected_prefix), "{provider} url={url}");
        assert!(url.contains("response_type=code"));
        assert!(url.contains(&format!("client_id={provider}-client")));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3499%2Fcallback"));
        assert!(url.contains("state=csrf-state"));
        assert!(!stdout.contains("client_secret"));
        assert!(!stdout.contains("access_token"));
        assert!(!stdout.contains("refresh_token"));
    }
}

#[test]
fn auth_authorize_url_text_output_is_self_describing_and_local_only() {
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "authorize-url",
        "--provider",
        "bangumi",
        "--client-id",
        "bangumi-client",
        "--redirect-uri",
        "http://localhost:3498/callback",
        "--state",
        "csrf-state",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("sync-v2 auth authorize-url"));
    assert!(stdout.contains("provider=bangumi"));
    assert!(stdout.contains("endpoint_source=legacy_repo_code"));
    assert!(stdout.contains("local-only"));
    assert!(stdout.contains("opens_browser=false"));
    assert!(stdout.contains("exchanges_token=false"));
    assert!(stdout.contains("sensitive_url=true"));
    assert!(stdout.contains("authorization_url_suppressed=true"));
    assert!(!stdout.contains("authorization_url=https://bgm.tv/oauth/authorize?"));
    assert!(!stdout.contains("csrf-state"));
    assert!(!stdout.contains("client_secret"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
}

#[test]
fn auth_authorize_url_percent_encodes_redirect_uri_and_state_values() {
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "auth",
        "authorize-url",
        "--provider",
        "anilist",
        "--client-id",
        "anilist-client",
        "--redirect-uri",
        "http://localhost:3499/callback?x=1&next=/done",
        "--state",
        "a+b/c d",
        "--show-sensitive-authorization-url",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let output: serde_json::Value =
        serde_json::from_str(&stdout).expect("authorize-url output should be json");
    let url = output["authorization_url"]
        .as_str()
        .expect("authorization url should be present");
    assert!(url.contains(
        "redirect_uri=http%3A%2F%2Flocalhost%3A3499%2Fcallback%3Fx%3D1%26next%3D%2Fdone"
    ));
    assert!(url.contains("state=a%2Bb%2Fc%20d"));
    assert!(!url.contains("state=a+b/c d"));
}

#[test]
fn auth_authorize_url_rejects_browser_network_and_database_flags() {
    for flag in [
        "--open-browser",
        "--exchange-token",
        "--client-secret",
        "--authorization-code",
        "--access-token",
        "--refresh-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
        "--csrf-token",
        "--refresh",
        "--reauthorize",
        "--write",
        "--apply",
        "--delete",
        "--db",
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let db_path =
            std::env::temp_dir().join(format!("bangumi-sync-authorize-url-{}", std::process::id()));
        let _ = std::fs::remove_file(&db_path);

        let code = sync_cli::run(
            [
                "sync-v2",
                "auth",
                "authorize-url",
                "--provider",
                "anilist",
                "--client-id",
                "anilist-client",
                "--redirect-uri",
                "http://localhost:3499/callback",
                "--state",
                "csrf-state",
                flag,
                db_path.to_str().expect("utf8 db path"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(!db_path.exists(), "{flag} should not create a database");
        let error = String::from_utf8(stderr).expect("error should be utf8");
        assert!(error.contains("disabled for auth authorize-url"));
        assert!(error.contains(flag));
    }
}

#[test]
fn auth_authorize_url_requires_myanimelist_pkce_challenge() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = sync_cli::run(
        [
            "sync-v2",
            "auth",
            "authorize-url",
            "--provider",
            "myanimelist",
            "--client-id",
            "mal-client",
            "--redirect-uri",
            "http://localhost:3500/callback",
            "--state",
            "csrf-state",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("pkce_code_challenge"));
}

#[test]
fn auth_authorize_url_validates_myanimelist_pkce_boundaries() {
    let run_with_challenge = |challenge: String| {
        run_cli(vec![
            "sync-v2".to_owned(),
            "auth".to_owned(),
            "authorize-url".to_owned(),
            "--provider".to_owned(),
            "myanimelist".to_owned(),
            "--client-id".to_owned(),
            "mal-client".to_owned(),
            "--redirect-uri".to_owned(),
            "http://localhost:3500/callback".to_owned(),
            "--state".to_owned(),
            "csrf-state".to_owned(),
            "--pkce-code-challenge".to_owned(),
            challenge,
            "--show-sensitive-authorization-url".to_owned(),
        ])
    };

    for invalid in ["a".repeat(42), "a".repeat(129), "bad\nchallenge".to_owned()] {
        let (code, stdout, stderr) = run_with_challenge(invalid);

        assert_eq!(code, 2, "invalid PKCE should be rejected");
        assert!(stdout.is_empty());
        assert!(stderr.contains("pkce_code_challenge"));
    }

    for valid in ["a".repeat(43), "a".repeat(128)] {
        let (code, stdout, stderr) = run_with_challenge(valid);

        assert_eq!(code, 0, "valid PKCE boundary should pass: {stderr}");
        assert!(stdout.contains("authorization_url="));
    }
}

#[test]
fn auth_authorize_url_rejects_pkce_for_non_myanimelist_providers() {
    for provider in ["bangumi", "anilist"] {
        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "auth",
            "authorize-url",
            "--provider",
            provider,
            "--client-id",
            "client",
            "--redirect-uri",
            "http://localhost:3499/callback",
            "--state",
            "csrf-state",
            "--pkce-code-challenge",
            "plain-code-verifier-123456789012345678901234567890",
        ]);

        assert_eq!(code, 2, "{provider} should reject PKCE challenge");
        assert!(stdout.is_empty());
        assert!(stderr.contains("only supported for myanimelist"));
    }
}

#[test]
fn auth_authorize_url_rejects_required_and_invalid_inputs_without_stdout() {
    let cases: Vec<(&str, Vec<&str>, &str)> = vec![
        (
            "missing provider",
            vec![
                "sync-v2",
                "auth",
                "authorize-url",
                "--client-id",
                "client",
                "--redirect-uri",
                "http://localhost:3499/callback",
                "--state",
                "state",
            ],
            "missing --provider",
        ),
        (
            "missing client id",
            vec![
                "sync-v2",
                "auth",
                "authorize-url",
                "--provider",
                "anilist",
                "--redirect-uri",
                "http://localhost:3499/callback",
                "--state",
                "state",
            ],
            "missing --client-id",
        ),
        (
            "missing redirect uri",
            vec![
                "sync-v2",
                "auth",
                "authorize-url",
                "--provider",
                "anilist",
                "--client-id",
                "client",
                "--state",
                "state",
            ],
            "missing --redirect-uri",
        ),
        (
            "missing state",
            vec![
                "sync-v2",
                "auth",
                "authorize-url",
                "--provider",
                "anilist",
                "--client-id",
                "client",
                "--redirect-uri",
                "http://localhost:3499/callback",
            ],
            "missing --state",
        ),
        (
            "unsupported provider",
            vec![
                "sync-v2",
                "auth",
                "authorize-url",
                "--provider",
                "kitsu",
                "--client-id",
                "client",
                "--redirect-uri",
                "http://localhost:3499/callback",
                "--state",
                "state",
            ],
            "unsupported provider",
        ),
        (
            "unsupported format",
            vec![
                "sync-v2",
                "auth",
                "authorize-url",
                "--provider",
                "anilist",
                "--client-id",
                "client",
                "--redirect-uri",
                "http://localhost:3499/callback",
                "--state",
                "state",
                "--format",
                "yaml",
            ],
            "unsupported output format",
        ),
    ];

    for (label, args, expected_error) in cases {
        let (code, stdout, stderr) = run_cli(args);

        assert_eq!(code, 2, "{label} should fail");
        assert!(stdout.is_empty(), "{label} should not write stdout");
        assert!(stderr.contains(expected_error), "{label} stderr={stderr:?}");
        assert!(!stderr.contains("plan output format"));
    }

    for (flag, value, expected_error) in [
        ("--client-id", " ", "missing --client-id"),
        ("--redirect-uri", " ", "missing --redirect-uri"),
        ("--state", " ", "missing --state"),
        ("--client-id", "bad\nclient", "client_id"),
        ("--redirect-uri", "bad\nuri", "redirect_uri"),
        ("--state", "bad\nstate", "state"),
    ] {
        let mut args = vec![
            "sync-v2".to_owned(),
            "auth".to_owned(),
            "authorize-url".to_owned(),
            "--provider".to_owned(),
            "anilist".to_owned(),
            "--client-id".to_owned(),
            "client".to_owned(),
            "--redirect-uri".to_owned(),
            "http://localhost:3499/callback".to_owned(),
            "--state".to_owned(),
            "state".to_owned(),
        ];
        let index = args
            .iter()
            .position(|arg| arg == flag)
            .expect("flag should exist")
            + 1;
        args[index] = value.to_owned();

        let (code, stdout, stderr) = run_cli(args);

        assert_eq!(code, 2, "{flag}={value:?} should fail");
        assert!(stdout.is_empty());
        assert!(
            stderr.contains(expected_error),
            "{flag}={value:?} stderr={stderr:?}"
        );
    }
}

#[test]
fn known_commands_are_read_only_skeletons() {
    for command in ["plan", "inspect-match"] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = sync_cli::run(["sync-v2", command], &mut stdout, &mut stderr);

        assert_eq!(code, 0, "{command} should succeed in skeleton mode");
        let output = String::from_utf8(stdout).expect("output should be utf8");
        assert!(output.contains(command));
        assert!(output.contains("read-only"));
        assert!(stderr.is_empty());
    }
}

#[test]
fn unknown_command_exits_with_usage_error() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = sync_cli::run(
        ["sync-v2", "definitely-unknown-command"],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("unknown command"));
    assert!(error.contains("definitely-unknown-command"));
}

#[test]
fn read_only_commands_reject_unexpected_trailing_args() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = sync_cli::run(["sync-v2", "plan", "--apply"], &mut stdout, &mut stderr);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("writes are disabled"));
    assert!(error.contains("--apply"));
}

#[test]
fn import_fixtures_loads_local_snapshot_into_sqlite() {
    let temp_dir =
        std::env::temp_dir().join(format!("bangumi-sync-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let fixture_path = temp_dir.join("bangumi-anime.json");
    let db_path = temp_dir.join("sync-v2.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &fixture_path,
        r#"{
            "data": [
                {
                    "subject_id": 253,
                    "subject_type": "anime",
                    "collection_type": "collect",
                    "rate": 10,
                    "ep_status": 26,
                    "vol_status": 0
                }
            ]
        }"#,
    )
    .expect("fixture should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = sync_cli::run(
        [
            "sync-v2",
            "import-fixtures",
            "--provider",
            "bangumi",
            "--kind",
            "anime",
            "--fixture",
            fixture_path.to_str().expect("utf8 fixture path"),
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("imported 1 collection entries"));
    assert!(output.contains("read-only"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let entry = store
        .collection_entry_details(
            "fixture-account",
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("entry lookup should work")
        .expect("entry should exist");
    assert_eq!(entry.score_hundred, Some(100));
    assert_eq!(entry.progress_episodes, Some(26));
}

#[test]
fn import_fixtures_imports_anilist_id_mal_identity_edges() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-cli-idmal-import-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let fixture_path = temp_dir.join("anilist-anime.json");
    let db_path = temp_dir.join("sync-v2-idmal-import.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &fixture_path,
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME", "idMal": 5114 },
                                    "status": "COMPLETED",
                                    "score": 90,
                                    "progress": 64,
                                    "progressVolumes": null
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
    )
    .expect("fixture should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = sync_cli::run(
        [
            "sync-v2",
            "import-fixtures",
            "--provider",
            "anilist",
            "--kind",
            "anime",
            "--fixture",
            fixture_path.to_str().expect("utf8 fixture path"),
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let anilist_work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    let mal_work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::MyAnimeList,
            sync_core::model::MediaKind::Anime,
            "5114",
        )
        .expect("mal edge lookup should work")
        .expect("mal edge should exist");
    assert_eq!(anilist_work_id, mal_work_id);
}

#[test]
fn import_legacy_config_loads_manual_relations_and_ignores_into_sqlite() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-legacy-config-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let manual_path = temp_dir.join("manual_relations.json");
    let ignore_path = temp_dir.join("ignore_entries.json");
    let db_path = temp_dir.join("sync-v2-legacy-config.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(&manual_path, "[[253,1]]").expect("manual config should write");
    std::fs::write(
        &ignore_path,
        r#"{"bangumi":[274613],"anilist":[12345],"mal":[67890]}"#,
    )
    .expect("ignore config should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-legacy-config",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--kind",
            "anime",
            "--manual-relations",
            manual_path.to_str().expect("utf8 manual path"),
            "--ignore-entries",
            ignore_path.to_str().expect("utf8 ignore path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("sync-v2 import-legacy-config"));
    assert!(output.contains("manual_relations=1"));
    assert!(output.contains("ignore_entries=3"));
    assert!(output.contains("local-only"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let bangumi_work = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("bangumi lookup should work")
        .expect("bangumi edge should exist");
    let anilist_work = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("anilist lookup should work")
        .expect("anilist edge should exist");
    assert_eq!(bangumi_work, anilist_work);
    assert_eq!(
        store
            .manual_mapping_decision(
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "253",
            )
            .expect("manual decision lookup should work"),
        Some(sync_core::store::ManualMappingDecision::Link)
    );
    assert_eq!(
        store
            .manual_mapping_decision(
                sync_core::model::Provider::MyAnimeList,
                sync_core::model::MediaKind::Anime,
                "67890",
            )
            .expect("ignore decision lookup should work"),
        Some(sync_core::store::ManualMappingDecision::Ignore)
    );
}

#[test]
fn import_legacy_config_rejects_provider_write_or_auth_flags() {
    for flag in [
        "--apply",
        "--write",
        "--delete",
        "--refresh",
        "--reauthorize",
        "--open-browser",
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = sync_cli::run(
            [
                "sync-v2",
                "import-legacy-config",
                "--db",
                "/tmp/sync-v2.sqlite3",
                "--kind",
                "anime",
                "--manual-relations",
                "/tmp/manual_relations.json",
                flag,
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        let error = String::from_utf8(stderr).expect("error should be utf8");
        assert!(error.contains("disabled for import-legacy-config"));
        assert!(error.contains(flag));
    }
}

#[test]
fn import_legacy_config_preflights_files_before_creating_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-legacy-config-preflight-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let manual_path = temp_dir.join("manual_relations.json");
    let missing_ignore_path = temp_dir.join("missing_ignore_entries.json");
    let db_path = temp_dir.join("sync-v2-legacy-config-preflight.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(&manual_path, "[[253,1]]").expect("manual config should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-legacy-config",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--kind",
            "anime",
            "--manual-relations",
            manual_path.to_str().expect("utf8 manual path"),
            "--ignore-entries",
            missing_ignore_path.to_str().expect("utf8 ignore path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "failed legacy import must not create or mutate the local db"
    );
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("failed to read"));
    assert!(error.contains("missing_ignore_entries.json"));
}

#[test]
fn import_legacy_config_validates_json_before_creating_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-legacy-config-json-preflight-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let manual_path = temp_dir.join("manual_relations.json");
    let ignore_path = temp_dir.join("ignore_entries.json");
    let db_path = temp_dir.join("sync-v2-legacy-config-json-preflight.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(&manual_path, "[[253,1]]").expect("manual config should write");
    std::fs::write(&ignore_path, "{not-json").expect("ignore config should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-legacy-config",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--kind",
            "anime",
            "--manual-relations",
            manual_path.to_str().expect("utf8 manual path"),
            "--ignore-entries",
            ignore_path.to_str().expect("utf8 ignore path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "invalid legacy JSON must not create or mutate the local db"
    );
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("invalid ignore entries"));
}

#[test]
fn import_dataset_loads_local_provider_items_and_crosswalks_into_sqlite() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-dataset-import-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let anime_offline_path = temp_dir.join("anime-offline-database.json");
    let bangumi_data_path = temp_dir.join("bangumi-data.json");
    let db_path = temp_dir.join("sync-v2-datasets.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &anime_offline_path,
        r#"{
            "data": [
                {
                    "title": "Cowboy Bebop",
                    "synonyms": ["カウボーイビバップ"],
                    "type": "TV",
                    "sources": [
                        "https://anilist.co/anime/1",
                        "https://myanimelist.net/anime/1/Cowboy_Bebop"
                    ]
                }
            ]
        }"#,
    )
    .expect("anime-offline fixture should write");
    std::fs::write(
        &bangumi_data_path,
        r#"{
            "items": [
                {
                    "title": "Cowboy Bebop",
                    "titleTranslate": {"ja": ["カウボーイビバップ"]},
                    "type": "tv",
                    "sites": [{"site": "bangumi", "id": "253"}]
                }
            ]
        }"#,
    )
    .expect("bangumi-data fixture should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-dataset",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--dataset",
            "anime-offline",
            "--file",
            anime_offline_path.to_str().expect("utf8 dataset path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("sync-v2 import-dataset"));
    assert!(output.contains("dataset=anime-offline"));
    assert!(output.contains("imported=1"));
    assert!(output.contains("local-only"));

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-dataset",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--dataset",
            "bangumi-data",
            "--kind",
            "anime",
            "--file",
            bangumi_data_path.to_str().expect("utf8 dataset path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("dataset=bangumi-data"));
    assert!(output.contains("media_kind=anime"));
    assert!(output.contains("imported=1"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert_eq!(
        store
            .provider_item_payload_hash(
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "253",
            )
            .expect("bangumi payload hash lookup should work")
            .as_deref(),
        Some("bangumi-data:253")
    );
    assert_eq!(
        store
            .provider_item_payload_hash(
                sync_core::model::Provider::AniList,
                sync_core::model::MediaKind::Anime,
                "1",
            )
            .expect("anilist payload hash lookup should work")
            .as_deref(),
        Some("anime-offline:Cowboy Bebop")
    );
    let anilist_work = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    let mal_work = store
        .find_work_by_external_id(
            sync_core::model::Provider::MyAnimeList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("mal edge lookup should work")
        .expect("mal edge should exist");
    assert_eq!(anilist_work, mal_work);
}

#[test]
fn import_dataset_preflights_file_before_creating_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-dataset-preflight-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let missing_dataset_path = temp_dir.join("missing-anime-offline.json");
    let db_path = temp_dir.join("sync-v2-dataset-preflight.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-dataset",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--dataset",
            "anime-offline",
            "--file",
            missing_dataset_path.to_str().expect("utf8 dataset path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "failed dataset import must not create or mutate the local db"
    );
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("failed to read"));
    assert!(error.contains("missing-anime-offline.json"));
}

#[test]
fn import_dataset_validates_json_before_creating_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-dataset-json-preflight-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let dataset_path = temp_dir.join("anime-offline-database.json");
    let db_path = temp_dir.join("sync-v2-dataset-json-preflight.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(&dataset_path, "{not-json").expect("dataset fixture should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-dataset",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--dataset",
            "anime-offline",
            "--file",
            dataset_path.to_str().expect("utf8 dataset path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "invalid dataset JSON must not create or mutate the local db"
    );
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("invalid dataset"));
}

#[test]
fn import_dataset_validates_bangumi_data_json_before_creating_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-bangumi-data-json-preflight-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let dataset_path = temp_dir.join("bangumi-data.json");
    let db_path = temp_dir.join("sync-v2-bangumi-data-json-preflight.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(&dataset_path, "{not-json").expect("dataset fixture should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-dataset",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--dataset",
            "bangumi-data",
            "--kind",
            "manga",
            "--file",
            dataset_path.to_str().expect("utf8 dataset path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "invalid bangumi-data JSON must not create or mutate the local db"
    );
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("invalid dataset"));
}

#[test]
fn import_dataset_conflict_does_not_leave_provider_item_rows() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-dataset-conflict-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let dataset_path = temp_dir.join("anime-offline-conflict.json");
    let db_path = temp_dir.join("sync-v2-dataset-conflict.sqlite3");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &dataset_path,
        r#"{
            "data": [
                {
                    "title": "Cowboy Bebop",
                    "synonyms": [],
                    "type": "TV",
                    "sources": [
                        "https://anilist.co/anime/1",
                        "https://myanimelist.net/anime/5/Cowboy_Bebop"
                    ]
                },
                {
                    "title": "Conflicting Cowboy Bebop",
                    "synonyms": [],
                    "type": "TV",
                    "sources": [
                        "https://anilist.co/anime/1",
                        "https://myanimelist.net/anime/6/Conflicting_Cowboy_Bebop"
                    ]
                }
            ]
        }"#,
    )
    .expect("dataset fixture should write");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-dataset",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--dataset",
            "anime-offline",
            "--file",
            dataset_path.to_str().expect("utf8 dataset path"),
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("failed to import dataset"));
    assert!(error.contains("ConflictingBatchRows"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert_eq!(
        store
            .provider_item_payload_hash(
                sync_core::model::Provider::AniList,
                sync_core::model::MediaKind::Anime,
                "1",
            )
            .expect("payload hash lookup should work"),
        None,
        "failed dataset import must not leave provider item rows"
    );
}

#[test]
fn import_dataset_rejects_provider_write_or_auth_flags() {
    for flag in [
        "--apply",
        "--write",
        "--delete",
        "--refresh",
        "--reauthorize",
        "--open-browser",
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = sync_cli::run(
            [
                "sync-v2",
                "import-dataset",
                "--db",
                "/tmp/sync-v2.sqlite3",
                "--dataset",
                "anime-offline",
                "--file",
                "/tmp/anime-offline-database.json",
                flag,
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        let error = String::from_utf8(stderr).expect("error should be utf8");
        assert!(error.contains("disabled for import-dataset"));
        assert!(error.contains(flag));
    }
}

#[test]
fn import_dataset_requires_kind_for_bangumi_data_only() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-dataset",
            "--db",
            "/tmp/sync-v2.sqlite3",
            "--dataset",
            "bangumi-data",
            "--file",
            "/tmp/bangumi-data.json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("missing --kind for bangumi-data"));

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "import-dataset",
            "--db",
            "/tmp/sync-v2.sqlite3",
            "--dataset",
            "anime-offline",
            "--kind",
            "anime",
            "--file",
            "/tmp/anime-offline-database.json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("anime-offline does not accept --kind"));
}

#[test]
fn plan_dry_run_reports_local_sqlite_actions() {
    let temp_dir =
        std::env::temp_dir().join(format!("bangumi-sync-plan-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let snapshot = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{
            "data": [
                {
                    "subject_id": 253,
                    "subject_type": "anime",
                    "collection_type": "collect",
                    "rate": 10,
                    "ep_status": 26,
                    "vol_status": 0
                }
            ]
        }"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &snapshot)
        .expect("snapshot should import");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "plan",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "anime",
            "--providers",
            "bangumi,anilist",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("dry-run plan"));
    assert!(output.contains("actions=1"));
    assert!(output.contains("conflicts=0"));
    assert!(output.contains("add bangumi -> anilist"));
    assert!(output.contains("target=1"));
}

#[test]
fn plan_dry_run_does_not_create_missing_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-readonly-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("missing-plan.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "plan",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "anime",
            "--providers",
            "bangumi,anilist",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(!db_path.exists(), "dry-run plan must not create a db file");
    let error = String::from_utf8(stderr).expect("error should be utf8");
    assert!(error.contains("failed to open db"));
}

#[test]
fn plan_dry_run_reports_update_actions_for_existing_entries() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-update-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-update.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "plan",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "anime",
            "--providers",
            "bangumi,anilist",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("actions=1"));
    assert!(output.contains("update bangumi -> anilist"));
    assert!(output.contains("fields=Score"));
    assert!(output.contains("change score <unset> -> 100"));
    assert!(output.contains("missing target field"));
}

#[test]
fn plan_dry_run_reports_json_field_changes() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-json-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-json.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "plan",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "anime",
            "--providers",
            "bangumi,anilist",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&stdout).expect("json report should parse");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["actions"][0]["kind"], "update");
    assert_eq!(report["actions"][0]["source_provider"], "bangumi");
    assert_eq!(report["actions"][0]["target_provider"], "anilist");
    assert_eq!(report["actions"][0]["field_changes"][0]["field"], "score");
    assert_eq!(report["actions"][0]["field_changes"][0]["old"], "<unset>");
    assert_eq!(report["actions"][0]["field_changes"][0]["new"], "100");
    assert!(report["conflicts"].as_array().expect("array").is_empty());
}

#[test]
fn plan_dry_run_reports_json_diagnostics_for_unsupported_bangumi_episode_progress() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-json-diagnostics-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-json-diagnostics.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"do","rate":7,"ep_status":12,"vol_status":0,"updated_at":"2026-06-01T00:00:00Z"}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":90,"progress":26,"progressVolumes":null,"updatedAt":1780444800}]}]}}}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "plan",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "anime",
            "--providers",
            "bangumi,anilist",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&stdout).expect("json report should parse");
    assert_eq!(report["actions"][0]["target_provider"], "bangumi");
    assert_eq!(
        report["actions"][0]["field_updates"],
        serde_json::json!(["status", "score"])
    );
    assert_eq!(report["diagnostics"][0]["source_provider"], "anilist");
    assert_eq!(report["diagnostics"][0]["target_provider"], "bangumi");
    assert_eq!(report["diagnostics"][0]["target_provider_entry_id"], "253");
    assert_eq!(report["diagnostics"][0]["media_kind"], "anime");
    assert_eq!(report["diagnostics"][0]["field"], "progress_episodes");
    assert_eq!(
        report["diagnostics"][0]["reason"],
        "Bangumi anime episode progress writes require resolved episode IDs; aggregate progress is unsupported"
    );
}

#[test]
fn plan_refresh_snapshots_dry_run_refreshes_fixture_responses_before_planning() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-dry-run-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-dry-run.sqlite3");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let bangumi_episodes_path = temp_dir.join("bangumi-episodes-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &bangumi_episodes_path,
        r#"{"data":[{"episode":{"id":1001,"type":0,"sort":1,"ep":1},"type":2,"updated_at":1700000000}]}"#,
    )
    .expect("bangumi episodes response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
    );
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::AniList,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!(
            "bangumi:anime:episodes:253:{}",
            bangumi_episodes_path.display()
        ),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["provider_writes"], false);
    assert_eq!(report["fixture_transport"], true);
    assert_eq!(report["outcome"], "planned");
    assert_eq!(
        report["imports"].as_array().expect("imports array").len(),
        2
    );
    assert_eq!(report["plan"]["actions"][0]["kind"], "update");
    assert_eq!(report["plan"]["actions"][0]["source_provider"], "bangumi");
    assert_eq!(report["plan"]["actions"][0]["target_provider"], "anilist");
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("fixture-secret-ref"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert!(store
        .collection_entry_details(
            "fixture-account",
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("bangumi entry lookup should work")
        .is_some());
    assert!(store
        .collection_entry_details(
            "fixture-account",
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("anilist entry lookup should work")
        .is_some());
    assert_eq!(
        store
            .bangumi_episode_ids_for_done_prefix("fixture-account", "253", 1)
            .expect("bangumi episode ids should query"),
        vec![1001]
    );
}

#[test]
fn plan_refresh_snapshots_requires_collection_fixture_when_only_bangumi_episode_fixture_is_present()
{
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-episode-only-fixture-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-episode-only-fixture.sqlite3");
    let episode_path = temp_dir.join("bangumi-episodes-response.json");
    let _ = std::fs::remove_file(&db_path);
    sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    std::fs::write(
        &episode_path,
        r#"{"data":[{"episode":{"id":1001,"type":0,"sort":1,"ep":1},"type":2,"updated_at":1700000000}]}"#,
    )
    .expect("episode response should write");

    let (code, _stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:episodes:253:{}", episode_path.display()),
    ]);

    assert_eq!(code, 2, "stderr={stderr}");
    assert!(stderr.contains("missing --fixture-response for provider=bangumi kind=anime"));
}

#[test]
fn plan_refresh_snapshots_auto_link_local_links_imported_entries_before_planning() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-auto-link-local-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-auto-link-local.sqlite3");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":254,"subject_type":"anime","collection_type":"collect","rate":9,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::Bangumi,
        sync_core::model::MediaKind::Anime,
        "253",
        "Cowboy Bebop",
        Some("TV"),
        Some(1998),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::AniList,
        sync_core::model::MediaKind::Anime,
        "1",
        "Cowboy Bebop",
        Some("TV"),
        Some(1998),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::Bangumi,
        sync_core::model::MediaKind::Anime,
        "254",
        "Samurai Champloo",
        Some("TV"),
        Some(2004),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::AniList,
        sync_core::model::MediaKind::Anime,
        "2",
        "Samurai Champloo",
        Some("TV"),
        Some(2004),
    );
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
    );
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::AniList,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--auto-link-local",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["provider_writes"], false);
    assert_eq!(report["local_snapshot_import"], true);
    assert_eq!(report["local_auto_link"], true);
    assert_eq!(report["outcome"], "planned");
    assert_eq!(
        report["imports"].as_array().expect("imports array").len(),
        2
    );
    assert!(report["imports"]
        .as_array()
        .expect("imports array")
        .iter()
        .all(|import| import["outcome"] == "imported"));
    assert_eq!(report["plan"]["unmatched"], 0);
    assert!(report["plan"]["conflicts"]
        .as_array()
        .expect("conflicts array")
        .is_empty());
    let actions = report["plan"]["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().any(|action| {
        action["kind"] == "add"
            && action["source_provider"] == "bangumi"
            && action["target_provider"] == "anilist"
            && action["target_provider_entry_id"] == "1"
    }));
    assert!(actions.iter().any(|action| {
        action["kind"] == "update"
            && action["source_provider"] == "bangumi"
            && action["target_provider"] == "anilist"
            && action["target_provider_entry_id"] == "2"
            && action["field_updates"]
                .as_array()
                .expect("field updates")
                .iter()
                .any(|field| field == "score")
    }));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let cowboy_bangumi = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("bangumi edge lookup should work")
        .expect("bangumi edge should exist");
    let cowboy_anilist = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    assert_eq!(cowboy_bangumi, cowboy_anilist);
    let champloo_bangumi = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "254",
        )
        .expect("bangumi edge lookup should work")
        .expect("bangumi edge should exist");
    let champloo_anilist = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "2",
        )
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    assert_eq!(champloo_bangumi, champloo_anilist);
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "253",
            )
            .expect("entry lookup should work")
            .expect("entry should exist")
            .work_id,
        Some(cowboy_bangumi)
    );
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "254",
            )
            .expect("entry lookup should work")
            .expect("entry should exist")
            .work_id,
        Some(champloo_bangumi)
    );
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::AniList,
                sync_core::model::MediaKind::Anime,
                "2",
            )
            .expect("entry lookup should work")
            .expect("entry should exist")
            .work_id,
        Some(champloo_anilist)
    );
    assert!(store
        .write_journal_entries("fixture-account", sync_core::model::Provider::Bangumi)
        .expect("bangumi journal lookup should work")
        .is_empty());
    assert!(store
        .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
        .expect("anilist journal lookup should work")
        .is_empty());
}

#[test]
fn plan_refresh_snapshots_auto_link_local_skips_partial_import_when_credentials_required() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-auto-link-local-credentials-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-auto-link-local-credentials.sqlite3");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::Bangumi,
        sync_core::model::MediaKind::Anime,
        "253",
        "Cowboy Bebop",
        Some("TV"),
        Some(1998),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::AniList,
        sync_core::model::MediaKind::Anime,
        "1",
        "Cowboy Bebop",
        Some("TV"),
        Some(1998),
    );
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--auto-link-local",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["outcome"], "credentials_required");
    assert_eq!(report["local_auto_link"], true);
    assert!(report["plan"].is_null());
    let imports = report["imports"].as_array().expect("imports array");
    assert_eq!(imports.len(), 2);
    assert_eq!(imports[0]["outcome"], "imported");
    assert_eq!(imports[1]["outcome"], "reauthorize");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert_eq!(
        store
            .find_work_by_external_id(
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "253",
            )
            .expect("bangumi edge lookup should work"),
        None
    );
    assert_eq!(
        store
            .find_work_by_external_id(
                sync_core::model::Provider::AniList,
                sync_core::model::MediaKind::Anime,
                "1",
            )
            .expect("anilist edge lookup should work"),
        None
    );
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "253",
            )
            .expect("entry lookup should work")
            .expect("bangumi entry should exist")
            .work_id,
        None
    );
}

#[test]
fn plan_refresh_snapshots_uses_test_credential_bundle_without_leaking_tokens() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-bundle-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-refresh-bundle.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-bundle-access-secret",
    );
    let anilist_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-bundle-access-secret",
    );
    drop(store);

    let raw_bundle = std::fs::read_to_string(&bundle_path).expect("bundle should exist");
    assert!(!raw_bundle.contains("bangumi-bundle-access-secret"));
    assert!(!raw_bundle.contains("anilist-bundle-access-secret"));

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["outcome"], "planned");
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["plan"]["actions"][0]["kind"], "update");
    assert!(!stdout.contains("bangumi-bundle-access-secret"));
    assert!(!stdout.contains("anilist-bundle-access-secret"));
    assert!(!stdout.contains(&bangumi_ref));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store"));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
}

#[test]
fn plan_refresh_snapshots_uses_encrypted_credential_bundle_without_leaking_tokens() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-encrypted-bundle-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-refresh-encrypted-bundle.sqlite3");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_PLAN_ENCRYPTED_PASSPHRASE_{}",
        std::process::id()
    );
    let passphrase = "plan encrypted bundle passphrase secret";
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi_ref = seed_valid_cli_encrypted_bundle_credential(
        &store,
        &bundle_path,
        passphrase,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-encrypted-plan-access-secret",
    );
    let anilist_ref = seed_valid_cli_encrypted_bundle_credential(
        &store,
        &bundle_path,
        passphrase,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-encrypted-plan-access-secret",
    );
    drop(store);

    let raw_bundle = std::fs::read_to_string(&bundle_path).expect("bundle should exist");
    assert!(!raw_bundle.contains("bangumi-encrypted-plan-access-secret"));
    assert!(!raw_bundle.contains("anilist-encrypted-plan-access-secret"));
    assert!(!raw_bundle.contains(passphrase));

    std::env::set_var(&passphrase_env, passphrase);
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    std::env::remove_var(&passphrase_env);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["outcome"], "planned");
    assert_eq!(report["credential_source"], "file_credential_bundle");
    assert_eq!(
        report["credential_bundle_codec"],
        "passphrase-argon2id-xchacha20poly1305-v1"
    );
    assert_eq!(report["plan"]["actions"][0]["kind"], "update");
    assert!(!stdout.contains("bangumi-encrypted-plan-access-secret"));
    assert!(!stdout.contains("anilist-encrypted-plan-access-secret"));
    assert!(!stdout.contains(passphrase));
    assert!(!stdout.contains(&bangumi_ref));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains(passphrase_env.as_str()));
    assert!(!stdout.contains("credential_store"));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
}

#[test]
fn sync_test_write_transport_refreshes_snapshots_plans_and_applies_without_live_provider_writes() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-cycle-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-cycle.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-sync-cycle-access-secret",
    );
    let anilist_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-sync-cycle-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "applied");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["fixture_read_transport"], true);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["local_snapshot_import"], true);
    assert_eq!(report["local_write_journal"], true);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(
        report["imports"].as_array().expect("imports array").len(),
        2
    );
    assert!(report["plan"].is_null());
    assert_eq!(report["plan_preview"]["dry_run"], true);
    assert_eq!(report["plan_preview"]["actions"][0]["kind"], "update");
    assert_eq!(
        report["plan_preview"]["actions"][0]["source_provider"],
        "bangumi"
    );
    assert_eq!(
        report["plan_preview"]["actions"][0]["target_provider"],
        "anilist"
    );
    assert_eq!(report["apply"]["summary"]["attempted"], 1);
    assert_eq!(report["apply"]["summary"]["succeeded"], 1);
    assert_eq!(report["apply"]["summary"]["failed"], 0);
    assert_eq!(report["apply"]["write_requests"][0]["provider"], "anilist");
    assert_eq!(
        report["apply"]["write_requests"][0]["target_provider_entry_id"],
        "1"
    );
    assert!(!stdout.contains("bangumi-sync-cycle-access-secret"));
    assert!(!stdout.contains("anilist-sync-cycle-access-secret"));
    assert!(!stdout.contains(&bangumi_ref));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert!(store
        .collection_entry_details(
            "fixture-account",
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("bangumi entry lookup should work")
        .is_some());
    assert!(store
        .collection_entry_details(
            "fixture-account",
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("anilist entry lookup should work")
        .is_some());
    let journals = store
        .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
        .expect("write journal should query");
    assert_eq!(journals.len(), 1);
    assert_eq!(
        journals[0].result_status,
        sync_core::store::WriteJournalStatus::Succeeded
    );
}

#[test]
fn sync_test_write_transport_auto_link_local_links_refreshed_snapshots_before_apply() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-auto-link-local-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-auto-link-local.sqlite3");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":254,"subject_type":"anime","collection_type":"collect","rate":9,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::Bangumi,
        sync_core::model::MediaKind::Anime,
        "253",
        "Cowboy Bebop",
        Some("TV"),
        Some(1998),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::AniList,
        sync_core::model::MediaKind::Anime,
        "1",
        "Cowboy Bebop",
        Some("TV"),
        Some(1998),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::Bangumi,
        sync_core::model::MediaKind::Anime,
        "254",
        "Samurai Champloo",
        Some("TV"),
        Some(2004),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::AniList,
        sync_core::model::MediaKind::Anime,
        "2",
        "Samurai Champloo",
        Some("TV"),
        Some(2004),
    );
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
    );
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::AniList,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--auto-link-local",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "applied");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["fixture_read_transport"], true);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["local_snapshot_import"], true);
    assert_eq!(report["local_auto_link"], true);
    assert_eq!(report["local_write_journal"], true);
    assert_eq!(report["credential_source"], "fixture_token_store");
    assert!(report["imports"]
        .as_array()
        .expect("imports array")
        .iter()
        .all(|import| import["outcome"] == "imported"));
    assert!(report["plan"].is_null());
    assert_eq!(report["plan_preview"]["dry_run"], true);
    assert_eq!(report["plan_preview"]["unmatched"], 0);
    assert!(report["plan_preview"]["conflicts"]
        .as_array()
        .expect("conflicts array")
        .is_empty());
    let actions = report["plan_preview"]["actions"]
        .as_array()
        .expect("actions array");
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().any(|action| {
        action["kind"] == "add"
            && action["source_provider"] == "bangumi"
            && action["target_provider"] == "anilist"
            && action["target_provider_entry_id"] == "1"
    }));
    assert!(actions.iter().any(|action| {
        action["kind"] == "update"
            && action["source_provider"] == "bangumi"
            && action["target_provider"] == "anilist"
            && action["target_provider_entry_id"] == "2"
            && action["field_updates"]
                .as_array()
                .expect("field updates")
                .iter()
                .any(|field| field == "score")
    }));
    assert_eq!(report["apply"]["summary"]["attempted"], 2);
    assert_eq!(report["apply"]["summary"]["succeeded"], 2);
    assert_eq!(report["apply"]["summary"]["failed"], 0);
    let write_requests = report["apply"]["write_requests"]
        .as_array()
        .expect("write requests array");
    assert_eq!(write_requests.len(), 2);
    assert!(write_requests.iter().all(|request| {
        request["provider"] == "anilist" && request["authorization"] == "redacted"
    }));
    assert!(write_requests
        .iter()
        .any(|request| { request["kind"] == "add" && request["target_provider_entry_id"] == "1" }));
    assert!(write_requests.iter().any(|request| {
        request["kind"] == "update" && request["target_provider_entry_id"] == "2"
    }));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let cowboy_bangumi = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("bangumi edge lookup should work")
        .expect("bangumi edge should exist");
    let cowboy_anilist = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    assert_eq!(cowboy_bangumi, cowboy_anilist);
    let champloo_bangumi = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "254",
        )
        .expect("bangumi edge lookup should work")
        .expect("bangumi edge should exist");
    let champloo_anilist = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "2",
        )
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    assert_eq!(champloo_bangumi, champloo_anilist);
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "253",
            )
            .expect("entry lookup should work")
            .expect("entry should exist")
            .work_id,
        Some(cowboy_bangumi)
    );
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "254",
            )
            .expect("entry lookup should work")
            .expect("entry should exist")
            .work_id,
        Some(champloo_bangumi)
    );
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::AniList,
                sync_core::model::MediaKind::Anime,
                "2",
            )
            .expect("entry lookup should work")
            .expect("entry should exist")
            .work_id,
        Some(champloo_anilist)
    );
    assert!(store
        .write_journal_entries("fixture-account", sync_core::model::Provider::Bangumi)
        .expect("bangumi journal lookup should work")
        .is_empty());
    let journals = store
        .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
        .expect("anilist journal lookup should work");
    assert_eq!(journals.len(), 2);
    assert!(journals
        .iter()
        .all(|journal| journal.result_status == sync_core::store::WriteJournalStatus::Succeeded));
    assert!(journals
        .iter()
        .any(|journal| journal.operation_id.contains("-add-anilist-anime-1-")));
    assert!(journals.iter().any(|journal| journal
        .operation_id
        .contains("-update-anilist-anime-2-score-")));
}

#[test]
fn sync_test_write_transport_auto_link_local_kind_all_links_anime_and_manga_before_apply() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-auto-link-kind-all-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-auto-link-kind-all.sqlite3");
    let bangumi_anime_path = temp_dir.join("bangumi-anime-response.json");
    let bangumi_manga_path = temp_dir.join("bangumi-manga-response.json");
    let anilist_anime_path = temp_dir.join("anilist-anime-response.json");
    let anilist_manga_path = temp_dir.join("anilist-manga-response.json");
    let mal_anime_path = temp_dir.join("mal-anime-response.json");
    let mal_manga_path = temp_dir.join("mal-manga-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_anime_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi anime response should write");
    std::fs::write(
        &bangumi_manga_path,
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"do","rate":8,"ep_status":12,"vol_status":3}]}"#,
    )
    .expect("bangumi manga response should write");
    std::fs::write(
        &anilist_anime_path,
        r#"{"data":{"MediaListCollection":{"lists":[]}}}"#,
    )
    .expect("anilist anime response should write");
    std::fs::write(
        &anilist_manga_path,
        r#"{"data":{"MediaListCollection":{"lists":[]}}}"#,
    )
    .expect("anilist manga response should write");
    std::fs::write(&mal_anime_path, r#"{"data":[]}"#).expect("mal anime response should write");
    std::fs::write(&mal_manga_path, r#"{"data":[]}"#).expect("mal manga response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    for (provider, media_kind, external_id, title, format, year) in [
        (
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
            "Cowboy Bebop",
            "TV",
            1998,
        ),
        (
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
            "Cowboy Bebop",
            "TV",
            1998,
        ),
        (
            sync_core::model::Provider::MyAnimeList,
            sync_core::model::MediaKind::Anime,
            "5",
            "Cowboy Bebop",
            "TV",
            1998,
        ),
        (
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Manga,
            "9001",
            "Yotsuba",
            "Manga",
            2003,
        ),
        (
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Manga,
            "2",
            "Yotsuba",
            "Manga",
            2003,
        ),
        (
            sync_core::model::Provider::MyAnimeList,
            sync_core::model::MediaKind::Manga,
            "30013",
            "Yotsuba",
            "Manga",
            2003,
        ),
    ] {
        seed_cli_provider_item(
            &store,
            provider,
            media_kind,
            external_id,
            title,
            Some(format),
            Some(year),
        );
    }
    for provider in [
        sync_core::model::Provider::Bangumi,
        sync_core::model::Provider::AniList,
        sync_core::model::Provider::MyAnimeList,
    ] {
        seed_valid_cli_credential(&store, provider, "fixture-account");
    }
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--auto-link-local",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "all",
        "--providers",
        "bangumi,anilist,myanimelist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_anime_path.display()),
        "--fixture-response",
        &format!("bangumi:manga:{}", bangumi_manga_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_anime_path.display()),
        "--fixture-response",
        &format!("anilist:manga:{}", anilist_manga_path.display()),
        "--fixture-response",
        &format!("myanimelist:anime:{}", mal_anime_path.display()),
        "--fixture-response",
        &format!("myanimelist:manga:{}", mal_manga_path.display()),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "applied");
    assert_eq!(report["local_auto_link"], true);
    assert_eq!(report["local_write_journal"], true);
    assert_eq!(report["credential_source"], "fixture_token_store");
    assert_eq!(
        report["imports"].as_array().expect("imports array").len(),
        6
    );
    assert!(report["imports"]
        .as_array()
        .expect("imports array")
        .iter()
        .all(|import| import["outcome"] == "imported"));
    assert_eq!(report["plan_preview"]["dry_run"], true);
    assert_eq!(report["plan_preview"]["unmatched"], 0);
    assert!(report["plan_preview"]["conflicts"]
        .as_array()
        .expect("conflicts array")
        .is_empty());
    let actions = report["plan_preview"]["actions"]
        .as_array()
        .expect("actions array");
    assert_eq!(actions.len(), 4);
    for (media_kind, target_provider, target_provider_entry_id) in [
        ("anime", "anilist", "1"),
        ("anime", "myanimelist", "5"),
        ("manga", "anilist", "2"),
        ("manga", "myanimelist", "30013"),
    ] {
        assert!(actions.iter().any(|action| {
            action["kind"] == "add"
                && action["source_provider"] == "bangumi"
                && action["media_kind"] == media_kind
                && action["target_provider"] == target_provider
                && action["target_provider_entry_id"] == target_provider_entry_id
        }));
    }
    assert_eq!(report["apply"]["summary"]["attempted"], 4);
    assert_eq!(report["apply"]["summary"]["succeeded"], 4);
    assert_eq!(report["apply"]["summary"]["failed"], 0);
    let write_requests = report["apply"]["write_requests"]
        .as_array()
        .expect("write requests array");
    assert_eq!(write_requests.len(), 4);
    assert!(write_requests
        .iter()
        .all(|request| request["authorization"] == "redacted"));
    for (media_kind, target_provider, target_provider_entry_id) in [
        ("anime", "anilist", "1"),
        ("anime", "myanimelist", "5"),
        ("manga", "anilist", "2"),
        ("manga", "myanimelist", "30013"),
    ] {
        assert!(write_requests.iter().any(|request| {
            request["kind"] == "add"
                && request["media_kind"] == media_kind
                && request["provider"] == target_provider
                && request["target_provider_entry_id"] == target_provider_entry_id
        }));
    }
    assert!(write_requests.iter().any(|request| {
        request["provider"] == "myanimelist"
            && request["media_kind"] == "manga"
            && request["url"] == "https://api.myanimelist.net/v2/manga/30013/my_list_status"
    }));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let anime_work = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("anime bangumi edge lookup should work")
        .expect("anime bangumi edge should exist");
    let manga_work = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Manga,
            "9001",
        )
        .expect("manga bangumi edge lookup should work")
        .expect("manga bangumi edge should exist");
    for (provider, media_kind, external_id, expected_work) in [
        (
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
            anime_work,
        ),
        (
            sync_core::model::Provider::MyAnimeList,
            sync_core::model::MediaKind::Anime,
            "5",
            anime_work,
        ),
        (
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Manga,
            "2",
            manga_work,
        ),
        (
            sync_core::model::Provider::MyAnimeList,
            sync_core::model::MediaKind::Manga,
            "30013",
            manga_work,
        ),
    ] {
        assert_eq!(
            store
                .find_work_by_external_id(provider, media_kind, external_id)
                .expect("edge lookup should work")
                .expect("edge should exist"),
            expected_work
        );
    }
    assert_eq!(
        store
            .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
            .expect("anilist journal should query")
            .len(),
        2
    );
    assert_eq!(
        store
            .write_journal_entries("fixture-account", sync_core::model::Provider::MyAnimeList)
            .expect("mal journal should query")
            .len(),
        2
    );
    assert!(store
        .write_journal_entries("fixture-account", sync_core::model::Provider::Bangumi)
        .expect("bangumi journal should query")
        .is_empty());
}

#[test]
fn sync_test_write_transport_uses_encrypted_bundle_without_live_provider_writes() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-cycle-encrypted-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-cycle-encrypted.sqlite3");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_SYNC_ENCRYPTED_PASSPHRASE_{}",
        std::process::id()
    );
    let passphrase = "sync encrypted bundle passphrase secret";
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi_ref = seed_valid_cli_encrypted_bundle_credential(
        &store,
        &bundle_path,
        passphrase,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-sync-encrypted-access-secret",
    );
    let anilist_ref = seed_valid_cli_encrypted_bundle_credential(
        &store,
        &bundle_path,
        passphrase,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-sync-encrypted-access-secret",
    );
    drop(store);

    std::env::set_var(&passphrase_env, passphrase);
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    std::env::remove_var(&passphrase_env);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "applied");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["fixture_read_transport"], true);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["credential_source"], "file_credential_bundle");
    assert_eq!(
        report["credential_bundle_codec"],
        "passphrase-argon2id-xchacha20poly1305-v1"
    );
    assert_eq!(report["apply"]["summary"]["attempted"], 1);
    assert_eq!(report["apply"]["summary"]["succeeded"], 1);
    assert!(!stdout.contains("bangumi-sync-encrypted-access-secret"));
    assert!(!stdout.contains("anilist-sync-encrypted-access-secret"));
    assert!(!stdout.contains(passphrase));
    assert!(!stdout.contains(&bangumi_ref));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains(passphrase_env.as_str()));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));
}

#[test]
fn sync_test_write_transport_auto_refreshes_expired_bundle_credential_before_snapshot_read() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-auto-refresh-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-auto-refresh.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 200000,
            "access_token": "sync-auto-refreshed-access-secret",
            "refresh_token": "sync-auto-rotated-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let stale_ref = seed_refresh_required_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "sync-stale-bangumi-access-secret",
        "sync-stale-bangumi-refresh-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--test-token-transport",
        "--token-response",
        &format!("bangumi:{}", token_response_path.display()),
        "--refresh-client-id",
        "bangumi:bangumi-client",
        "--refresh-client-secret",
        "bangumi:bangumi-secret",
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "applied");
    assert_eq!(report["test_token_transport"], true);
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["imports"][0]["outcome"], "imported");
    assert_eq!(report["apply"]["summary"]["attempted"], 0);
    assert!(!stdout.contains("sync-stale-bangumi-access-secret"));
    assert!(!stdout.contains("sync-stale-bangumi-refresh-secret"));
    assert!(!stdout.contains("sync-auto-refreshed-access-secret"));
    assert!(!stdout.contains("sync-auto-rotated-refresh-secret"));
    assert!(!stdout.contains("bangumi-secret"));
    assert!(!stdout.contains(&stale_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::Bangumi, "fixture-account")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_ne!(credential.credential_store_ref, stale_ref);
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_200_000);
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));
    for output in [&stdout, &stderr] {
        assert!(!output.contains(&credential.credential_store_ref));
    }
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        1
    );
}

#[test]
fn sync_test_write_transport_auto_refreshes_expired_encrypted_bundle_credential_before_snapshot_read(
) {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-auto-refresh-encrypted-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-auto-refresh-encrypted.sqlite3");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_SYNC_AUTO_REFRESH_PASSPHRASE_{}",
        std::process::id()
    );
    let passphrase = "sync auto refresh encrypted bundle passphrase secret";
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 200000,
            "access_token": "encrypted-sync-auto-refreshed-access-secret",
            "refresh_token": "encrypted-sync-auto-rotated-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let stale_ref = seed_refresh_required_cli_encrypted_bundle_credential(
        &store,
        &bundle_path,
        passphrase,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "encrypted-sync-stale-bangumi-access-secret",
        "encrypted-sync-stale-bangumi-refresh-secret",
    );
    drop(store);

    std::env::set_var(&passphrase_env, passphrase);
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--test-token-transport",
        "--token-response",
        &format!("bangumi:{}", token_response_path.display()),
        "--refresh-client-id",
        "bangumi:bangumi-client",
        "--refresh-client-secret",
        "bangumi:bangumi-secret",
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    std::env::remove_var(&passphrase_env);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "applied");
    assert_eq!(report["credential_source"], "file_credential_bundle");
    assert_eq!(
        report["credential_bundle_codec"],
        "passphrase-argon2id-xchacha20poly1305-v1"
    );
    assert_eq!(report["test_token_transport"], true);
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["imports"][0]["outcome"], "imported");
    assert_eq!(report["apply"]["summary"]["attempted"], 0);
    for output in [&stdout, &stderr] {
        assert!(!output.contains("encrypted-sync-stale-bangumi-access-secret"));
        assert!(!output.contains("encrypted-sync-stale-bangumi-refresh-secret"));
        assert!(!output.contains("encrypted-sync-auto-refreshed-access-secret"));
        assert!(!output.contains("encrypted-sync-auto-rotated-refresh-secret"));
        assert!(!output.contains("bangumi-secret"));
        assert!(!output.contains(passphrase));
        assert!(!output.contains(&stale_ref));
        assert!(!output.contains(bundle_path.to_str().expect("utf8 bundle path")));
        assert!(!output.contains(passphrase_env.as_str()));
        assert!(!output.contains("credential_store_ref"));
        assert!(!output.contains("Authorization"));
        assert!(!output.contains("Bearer"));
        assert!(!output.contains("access_token"));
        assert!(!output.contains("refresh_token"));
    }

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::Bangumi, "fixture-account")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_ne!(credential.credential_store_ref, stale_ref);
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_200_000);
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));
    for output in [&stdout, &stderr] {
        assert!(!output.contains(&credential.credential_store_ref));
    }
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        1
    );

    let mut bundle = sync_core::provider::FileCredentialBundleSecretStore::new(
        &bundle_path,
        sync_core::provider::PassphraseCredentialBundleCodec::new(passphrase),
    );
    let token = sync_core::provider::ProviderCredentialAccessTokenStore::get_provider_access_token(
        &mut bundle,
        sync_core::provider::ProviderCredentialSecretLookup {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            credential_store_ref: credential.credential_store_ref,
        },
    )
    .expect("new access token should be readable through refreshed credential ref");
    assert_eq!(token.provider(), sync_core::model::Provider::Bangumi);
    assert!(!format!("{token:?}").contains("encrypted-sync-auto-refreshed-access-secret"));
}

#[test]
fn sync_test_token_transport_requires_matching_refresh_client_id_before_opening_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-missing-client-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("missing-sync-v2.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &token_response_path,
        r#"{"access_token":"sync-missing-client-access-secret"}"#,
    )
    .expect("token response should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--test-token-transport",
        "--token-response",
        &format!("bangumi:{}", token_response_path.display()),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("--token-response requires matching --refresh-client-id"));
    assert!(!stderr.contains("sync-missing-client-access-secret"));
    assert!(
        !db_path.exists(),
        "argument validation should fail before opening db"
    );
}

#[test]
fn sync_test_write_transport_can_verify_post_write_state_from_fixture_read_back() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-post-write-verify-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-post-write.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_initial_path = temp_dir.join("anilist-initial-response.json");
    let anilist_read_back_path = temp_dir.join("anilist-read-back-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_initial_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist initial response should write");
    std::fs::write(
        &anilist_read_back_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist read-back response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-sync-cycle-access-secret",
    );
    seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-sync-cycle-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--verify-post-write",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_initial_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_read_back_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "applied");
    assert_eq!(report["post_write_verification"], true);
    assert_eq!(report["apply"]["summary"]["attempted"], 1);
    assert_eq!(report["apply"]["summary"]["succeeded"], 1);
    assert_eq!(report["apply"]["summary"]["failed"], 0);
    assert_eq!(report["apply"]["summary"]["post_write_verified"], 1);
    assert_eq!(report["apply"]["summary"]["post_write_skipped"], 0);
    assert_eq!(report["apply"]["write_requests"][0]["provider"], "anilist");
    assert_eq!(report["live_provider_writes"], false);
    assert!(!stdout.contains("anilist-sync-cycle-access-secret"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));
}

#[test]
fn sync_post_write_verification_reuses_one_read_back_snapshot_for_same_target_collection() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-post-write-cache-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-post-write-cache.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_initial_path = temp_dir.join("anilist-initial-response.json");
    let anilist_read_back_path = temp_dir.join("anilist-read-back-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":254,"subject_type":"anime","collection_type":"collect","rate":9,"ep_status":12,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_initial_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null},{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":12,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist initial response should write");
    std::fs::write(
        &anilist_read_back_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null},{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":90,"progress":12,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist read-back response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1], [254, 2]]",
    )
    .expect("manual relation import should work");
    seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-sync-cycle-access-secret",
    );
    seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-sync-cycle-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--verify-post-write",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_initial_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_read_back_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "applied");
    assert_eq!(report["post_write_verification"], true);
    assert_eq!(report["apply"]["summary"]["attempted"], 2);
    assert_eq!(report["apply"]["summary"]["succeeded"], 2);
    assert_eq!(report["apply"]["summary"]["failed"], 0);
    assert_eq!(report["apply"]["summary"]["post_write_verified"], 2);
    assert_eq!(report["apply"]["summary"]["post_write_skipped"], 0);
    assert_eq!(
        report["apply"]["write_requests"]
            .as_array()
            .expect("write requests should be an array")
            .len(),
        2
    );
    assert!(!stdout.contains("anilist-sync-cycle-access-secret"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let journals = store
        .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
        .expect("write journal should query");
    assert_eq!(journals.len(), 2);
    assert!(journals.iter().all(|journal| {
        journal.result_status == sync_core::store::WriteJournalStatus::Succeeded
    }));
}

#[test]
fn sync_post_write_verification_cached_miss_completes_pending_journals_failed() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-post-write-cache-miss-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-post-write-cache-miss.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_initial_path = temp_dir.join("anilist-initial-response.json");
    let anilist_read_back_path = temp_dir.join("anilist-read-back-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":254,"subject_type":"anime","collection_type":"collect","rate":9,"ep_status":12,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_initial_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null},{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":12,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist initial response should write");
    std::fs::write(
        &anilist_read_back_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":90,"progress":12,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist read-back response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1], [254, 2]]",
    )
    .expect("manual relation import should work");
    seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-sync-cycle-access-secret",
    );
    seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-sync-cycle-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--verify-post-write",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_initial_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_read_back_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("PostWriteVerification"));
    assert!(stderr.contains("target_provider_entry_id: \"1\""));
    assert!(!stderr.contains("anilist-sync-cycle-access-secret"));
    assert!(!stderr.contains("Authorization"));
    assert!(!stderr.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let journals = store
        .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
        .expect("write journal should query");
    assert_eq!(journals.len(), 2);
    assert!(journals
        .iter()
        .all(|journal| { journal.result_status == sync_core::store::WriteJournalStatus::Failed }));
    assert!(journals
        .iter()
        .all(|journal| journal.completed_at.is_some()));
}

#[test]
fn sync_credentials_required_does_not_apply_or_create_journal() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-credentials-required-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-credentials-required.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bundle_path,
        "{not-json bangumi-sync-cycle-access-secret fixture-secret-ref",
    )
    .expect("corrupt bundle should write");
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    seed_refresh_required_cli_credential(
        &store,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "credentials_required");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["fixture_read_transport"], true);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["local_write_journal"], false);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["imports"][0]["outcome"], "refresh_required");
    assert!(report["plan"].is_null());
    assert!(report["plan_preview"].is_null());
    assert!(report["apply"].is_null());
    assert!(!stdout.contains("bangumi-sync-cycle-access-secret"));
    assert!(!stdout.contains("fixture-secret-ref"));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        0
    );
    assert!(store
        .write_journal_entries("fixture-account", sync_core::model::Provider::Bangumi)
        .expect("write journal should query")
        .is_empty());
}

#[test]
fn sync_test_write_transport_text_blocked_conflicts_includes_conflict_details() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-blocked-conflicts-text-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-blocked-conflicts-text.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let mal_path = temp_dir.join("mal-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"collect","rate":7,"ep_status":12,"vol_status":3,"updated_at":"2026-06-01T00:00:00Z"}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"MANGA"},"status":"CURRENT","score":90,"progress":12,"progressVolumes":3,"updatedAt":1780444800}]}]}}}"#,
    )
    .expect("anilist response should write");
    std::fs::write(
        &mal_path,
        r#"{"data":[{"node":{"id":30013,"media_type":"manga"},"list_status":{"status":"dropped","score":7,"num_chapters_read":12,"num_volumes_read":3,"updated_at":"2026-06-03T00:00:00Z"}}]}"#,
    )
    .expect("mal response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Manga,
        "[[9001, 2]]",
    )
    .expect("manual relation import should work");
    let work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Manga,
            "9001",
        )
        .expect("work lookup should work")
        .expect("work should exist");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id,
            provider: sync_core::model::Provider::MyAnimeList,
            media_kind: sync_core::model::MediaKind::Manga,
            external_id: "30013".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");
    let bangumi_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-sync-blocked-text-access-secret",
    );
    let anilist_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-sync-blocked-text-access-secret",
    );
    let mal_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::MyAnimeList,
        "fixture-account",
        "mal-sync-blocked-text-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "manga",
        "--providers",
        "bangumi,anilist,myanimelist",
        "--fixture-response",
        &format!("bangumi:manga:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:manga:{}", anilist_path.display()),
        "--fixture-response",
        &format!("myanimelist:manga:{}", mal_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
    ]);

    assert_eq!(code, 1);
    assert!(stderr.is_empty());
    assert!(stdout.contains("sync-v2 sync: blocked_conflicts"));
    assert!(stdout.contains("live_provider_writes=false"));
    assert!(stdout.contains("local_write_journal=false"));
    assert!(stdout.contains("actions=2"));
    assert!(stdout.contains("conflicts=1"));
    assert!(stdout.contains("apply_blocked_reason=conflicts_present"));
    assert!(stdout.contains("- preview update anilist -> bangumi target=9001 fields=score"));
    assert!(stdout.contains("- preview conflict work="));
    assert!(stdout.contains("field=Status"));
    assert!(stdout.contains("bangumi=Completed"));
    assert!(stdout.contains("anilist=InProgress"));
    assert!(stdout.contains("myanimelist=Dropped"));
    assert!(!stdout.contains("bangumi-sync-blocked-text-access-secret"));
    assert!(!stdout.contains("anilist-sync-blocked-text-access-secret"));
    assert!(!stdout.contains("mal-sync-blocked-text-access-secret"));
    assert!(!stdout.contains(&bangumi_ref));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(&mal_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    for provider in [
        sync_core::model::Provider::Bangumi,
        sync_core::model::Provider::AniList,
        sync_core::model::Provider::MyAnimeList,
    ] {
        assert!(
            store
                .write_journal_entries("fixture-account", provider)
                .expect("write journal should query")
                .is_empty(),
            "{provider:?} should not have write journal entries"
        );
    }
}

#[test]
fn sync_test_write_transport_json_blocked_conflicts_preserves_verify_intent_and_no_writes() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-sync-blocked-conflicts-json-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-sync-blocked-conflicts-json.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let mal_path = temp_dir.join("mal-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"collect","rate":7,"ep_status":12,"vol_status":3,"updated_at":"2026-06-01T00:00:00Z"}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"MANGA"},"status":"CURRENT","score":90,"progress":12,"progressVolumes":3,"updatedAt":1780444800}]}]}}}"#,
    )
    .expect("anilist response should write");
    std::fs::write(
        &mal_path,
        r#"{"data":[{"node":{"id":30013,"media_type":"manga"},"list_status":{"status":"dropped","score":7,"num_chapters_read":12,"num_volumes_read":3,"updated_at":"2026-06-03T00:00:00Z"}}]}"#,
    )
    .expect("mal response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Manga,
        "[[9001, 2]]",
    )
    .expect("manual relation import should work");
    let work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Manga,
            "9001",
        )
        .expect("work lookup should work")
        .expect("work should exist");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id,
            provider: sync_core::model::Provider::MyAnimeList,
            media_kind: sync_core::model::MediaKind::Manga,
            external_id: "30013".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");
    let bangumi_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "bangumi-sync-blocked-json-access-secret",
    );
    let anilist_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-sync-blocked-json-access-secret",
    );
    let mal_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::MyAnimeList,
        "fixture-account",
        "mal-sync-blocked-json-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "sync",
        "--test-write-transport",
        "--verify-post-write",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "manga",
        "--providers",
        "bangumi,anilist,myanimelist",
        "--fixture-response",
        &format!("bangumi:manga:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:manga:{}", anilist_path.display()),
        "--fixture-response",
        &format!("myanimelist:manga:{}", mal_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync output should be json");
    assert_eq!(report["outcome"], "blocked_conflicts");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["fixture_read_transport"], true);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["post_write_verification"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["local_snapshot_import"], true);
    assert_eq!(report["local_write_journal"], false);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["blocked_conflicts"], 1);
    assert_eq!(
        report["blocked_conflicts"],
        report["plan_preview"]["apply_blocking_conflicts"]
    );
    assert_eq!(
        report["imports"].as_array().expect("imports array").len(),
        3
    );
    assert!(report["plan"].is_null());
    assert_eq!(report["plan_preview"]["apply_blocked"], true);
    assert_eq!(
        report["plan_preview"]["apply_blocked_reason"],
        "conflicts_present"
    );
    assert_eq!(
        report["plan_preview"]["actions"]
            .as_array()
            .expect("actions array")
            .len(),
        2
    );
    assert_eq!(
        report["plan_preview"]["conflicts"]
            .as_array()
            .expect("conflicts array")
            .len(),
        1
    );
    assert!(report["apply"].is_null());
    assert!(report
        .as_object()
        .expect("report should be object")
        .contains_key("write_requests"));
    assert!(report["write_requests"].is_null());
    assert!(!stdout.contains("bangumi-sync-blocked-json-access-secret"));
    assert!(!stdout.contains("anilist-sync-blocked-json-access-secret"));
    assert!(!stdout.contains("mal-sync-blocked-json-access-secret"));
    assert!(!stdout.contains(&bangumi_ref));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(&mal_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    for provider in [
        sync_core::model::Provider::Bangumi,
        sync_core::model::Provider::AniList,
        sync_core::model::Provider::MyAnimeList,
    ] {
        assert!(
            store
                .write_journal_entries("fixture-account", provider)
                .expect("write journal should query")
                .is_empty(),
            "{provider:?} should not have write journal entries"
        );
    }
}

#[test]
fn sync_rejects_live_write_secret_browser_and_dry_run_flags_before_opening_database() {
    for flag in [
        "--dry-run",
        "--apply",
        "--write",
        "--delete",
        "--refresh",
        "--refresh-snapshots",
        "--reauthorize",
        "--open-browser",
        "--exchange-token",
        "--client-secret",
        "--authorization-code",
        "--access-token",
        "--refresh-token",
        "--csrf-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
        "--fixture",
    ] {
        let db_path = std::env::temp_dir().join(format!(
            "bangumi-sync-sync-rejects-{}-{}.sqlite3",
            std::process::id(),
            flag.trim_start_matches("--")
        ));
        let _ = std::fs::remove_file(&db_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "sync",
            "--test-write-transport",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "anime",
            "--providers",
            "bangumi,anilist",
            "--fixture-response",
            "bangumi:anime:/tmp/does-not-matter.json",
            flag,
            "secret-or-path",
        ]);

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(
            !db_path.exists(),
            "{flag} should be rejected before opening db"
        );
        assert!(stderr.contains("disabled for sync"), "{flag}: {stderr}");
        assert!(stderr.contains(flag), "{flag}: {stderr}");
    }
}

#[test]
fn apply_test_write_transport_uses_bundle_token_without_live_provider_writes() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-test-write-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-apply-test-write.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    let anilist_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-apply-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("apply output should be json");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["summary"]["attempted"], 1);
    assert_eq!(report["summary"]["succeeded"], 1);
    assert_eq!(report["summary"]["failed"], 0);
    assert_eq!(report["write_requests"][0]["provider"], "anilist");
    assert_eq!(report["write_requests"][0]["method"], "POST");
    assert_eq!(
        report["write_requests"][0]["url"],
        "https://graphql.anilist.co"
    );
    assert_eq!(
        report["write_requests"][0]["content_type"],
        "application/json"
    );
    assert!(!stdout.contains("anilist-apply-access-secret"));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let journals = store
        .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
        .expect("write journal should query");
    assert_eq!(journals.len(), 1);
    assert_eq!(
        journals[0].result_status,
        sync_core::store::WriteJournalStatus::Succeeded
    );
}

#[test]
fn apply_test_write_transport_uses_encrypted_bundle_without_live_provider_writes() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-encrypted-write-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-apply-encrypted-write.sqlite3");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_APPLY_ENCRYPTED_PASSPHRASE_{}",
        std::process::id()
    );
    let passphrase = "apply encrypted bundle passphrase secret";
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    let anilist_ref = seed_valid_cli_encrypted_bundle_credential(
        &store,
        &bundle_path,
        passphrase,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-apply-encrypted-access-secret",
    );
    drop(store);

    std::env::set_var(&passphrase_env, passphrase);
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    std::env::remove_var(&passphrase_env);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("apply output should be json");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["credential_source"], "file_credential_bundle");
    assert_eq!(
        report["credential_bundle_codec"],
        "passphrase-argon2id-xchacha20poly1305-v1"
    );
    assert_eq!(report["summary"]["attempted"], 1);
    assert_eq!(report["summary"]["succeeded"], 1);
    assert_eq!(report["write_requests"][0]["provider"], "anilist");
    assert!(!stdout.contains("anilist-apply-encrypted-access-secret"));
    assert!(!stdout.contains(passphrase));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains(passphrase_env.as_str()));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));
}

#[test]
fn apply_test_write_transport_reports_mixed_action_conflict_plan_as_blocked_json() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-blocked-conflicts-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-apply-blocked-conflicts.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bundle_path,
        "{not-json blocked-conflict-access-secret fixture-secret-ref",
    )
    .expect("corrupt bundle should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Manga,
        "[[9001, 2]]",
    )
    .expect("manual relation import should work");
    let work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Manga,
            "9001",
        )
        .expect("work lookup should work")
        .expect("work should exist");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id,
            provider: sync_core::model::Provider::MyAnimeList,
            media_kind: sync_core::model::MediaKind::Manga,
            external_id: "30013".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("mal edge should insert");
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"collect","rate":7,"ep_status":12,"vol_status":3,"updated_at":"2026-06-01T00:00:00Z"}]}"#,
        sync_core::model::MediaKind::Manga,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"MANGA"},"status":"CURRENT","score":90,"progress":12,"progressVolumes":3,"updatedAt":1780444800}]}]}}}"#,
        sync_core::model::MediaKind::Manga,
    )
    .expect("anilist fixture should parse");
    let mal = sync_core::provider::parse_myanimelist_collection_fixture(
        r#"{"data":[{"node":{"id":30013,"media_type":"manga"},"list_status":{"status":"dropped","score":7,"num_chapters_read":12,"num_volumes_read":3,"updated_at":"2026-06-03T00:00:00Z"}}]}"#,
        sync_core::model::MediaKind::Manga,
    )
    .expect("mal fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &mal)
        .expect("mal snapshot should import");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "manga",
        "--providers",
        "bangumi,anilist,myanimelist",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("apply output should be json");
    assert_eq!(report["outcome"], "blocked_conflicts");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["local_write_journal"], false);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert!(report["plan"].is_null());
    assert!(report["apply"].is_null());
    assert_eq!(report["plan_preview"]["apply_blocked"], true);
    assert_eq!(
        report["plan_preview"]["apply_blocked_reason"],
        "conflicts_present"
    );
    assert_eq!(
        report["plan_preview"]["actions"]
            .as_array()
            .expect("actions should be array")
            .len(),
        2
    );
    assert_eq!(
        report["plan_preview"]["conflicts"]
            .as_array()
            .expect("conflicts should be array")
            .len(),
        1
    );
    assert!(report
        .as_object()
        .expect("report should be object")
        .contains_key("write_requests"));
    assert!(report["write_requests"].is_null());
    assert!(!stdout.contains("blocked-conflict-access-secret"));
    assert!(!stdout.contains("fixture-secret-ref"));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    for provider in [
        sync_core::model::Provider::Bangumi,
        sync_core::model::Provider::AniList,
        sync_core::model::Provider::MyAnimeList,
    ] {
        assert!(
            store
                .write_journal_entries("fixture-account", provider)
                .expect("write journal should query")
                .is_empty(),
            "{provider:?} should not have write journal entries"
        );
    }
}

#[test]
fn apply_test_write_transport_can_verify_post_write_state_from_fixture_read_back() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-post-write-verify-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-apply-post-write.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let anilist_read_back_path = temp_dir.join("anilist-read-back-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &anilist_read_back_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":100,"progress":26,"progressVolumes":null},{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":90,"progress":12,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist read-back response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1], [254, 2]]",
    )
    .expect("manual relation import should work");
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":254,"subject_type":"anime","collection_type":"collect","rate":9,"ep_status":12,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null},{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":12,"progressVolumes":null}]}]}}}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-apply-access-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--test-write-transport",
        "--verify-post-write",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_read_back_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("apply output should be json");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["summary"]["attempted"], 2);
    assert_eq!(report["summary"]["succeeded"], 2);
    assert_eq!(report["summary"]["failed"], 0);
    assert_eq!(report["summary"]["post_write_verified"], 2);
    assert_eq!(report["summary"]["post_write_skipped"], 0);
    assert_eq!(
        report["write_requests"]
            .as_array()
            .expect("write requests should be an array")
            .len(),
        2
    );
    assert!(!stdout.contains("anilist-apply-access-secret"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let journals = store
        .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
        .expect("write journal should query");
    assert_eq!(journals.len(), 2);
    assert!(journals.iter().all(|journal| {
        journal.result_status == sync_core::store::WriteJournalStatus::Succeeded
    }));
}

#[test]
fn apply_test_write_transport_reports_three_provider_anime_and_manga_actions() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-all-kind-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-apply-all-kind.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("anime manual relation import should work");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Manga,
        "[[9001, 2]]",
    )
    .expect("manga manual relation import should work");
    let anime_work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("anime work lookup should work")
        .expect("anime work should exist");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: anime_work_id,
            provider: sync_core::model::Provider::MyAnimeList,
            media_kind: sync_core::model::MediaKind::Anime,
            external_id: "5114".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("anime mal edge should insert");
    let manga_work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Manga,
            "9001",
        )
        .expect("manga work lookup should work")
        .expect("manga work should exist");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: manga_work_id,
            provider: sync_core::model::Provider::MyAnimeList,
            media_kind: sync_core::model::MediaKind::Manga,
            external_id: "300".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("manga mal edge should insert");
    let anime_bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anime bangumi fixture should parse");
    let manga_bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"do","rate":8,"ep_status":12,"vol_status":3}]}"#,
        sync_core::model::MediaKind::Manga,
    )
    .expect("manga bangumi fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &anime_bangumi)
        .expect("anime snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &manga_bangumi)
        .expect("manga snapshot should import");
    let anilist_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::AniList,
        "fixture-account",
        "anilist-apply-all-secret",
    );
    let mal_ref = seed_valid_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::MyAnimeList,
        "fixture-account",
        "mal-apply-all-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "all",
        "--providers",
        "bangumi,anilist,myanimelist",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("apply output should be json");
    assert_eq!(report["summary"]["attempted"], 4);
    assert_eq!(report["summary"]["succeeded"], 4);
    assert_eq!(report["summary"]["failed"], 0);
    let write_requests = report["write_requests"]
        .as_array()
        .expect("write requests should be an array");
    assert_eq!(write_requests.len(), 4);
    let expected_requests = [
        ("add", "anime", "anilist", "1"),
        ("add", "anime", "myanimelist", "5114"),
        ("add", "manga", "anilist", "2"),
        ("add", "manga", "myanimelist", "300"),
    ];
    for (request, (kind, media_kind, provider, target_provider_entry_id)) in
        write_requests.iter().zip(expected_requests)
    {
        assert_eq!(request["kind"], kind);
        assert_eq!(request["media_kind"], media_kind);
        assert_eq!(request["provider"], provider);
        assert_eq!(request["target_provider"], provider);
        assert_eq!(
            request["target_provider_entry_id"],
            target_provider_entry_id
        );
        let fields = request["field_updates"]
            .as_array()
            .expect("field updates should be an array");
        assert!(fields.iter().any(|field| field == "status"));
    }
    assert!(!stdout.contains("anilist-apply-all-secret"));
    assert!(!stdout.contains("mal-apply-all-secret"));
    assert!(!stdout.contains(&anilist_ref));
    assert!(!stdout.contains(&mal_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert_eq!(
        store
            .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
            .expect("anilist journal should query")
            .len(),
        2
    );
    assert_eq!(
        store
            .write_journal_entries("fixture-account", sync_core::model::Provider::MyAnimeList,)
            .expect("mal journal should query")
            .len(),
        2
    );
}

#[test]
fn apply_test_write_transport_auto_link_local_links_existing_snapshots_before_apply() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-auto-link-local-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-apply-auto-link-local.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::Bangumi,
        sync_core::model::MediaKind::Anime,
        "253",
        "Cowboy Bebop",
        Some("TV"),
        Some(1998),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::AniList,
        sync_core::model::MediaKind::Anime,
        "1",
        "Cowboy Bebop",
        Some("TV"),
        Some(1998),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::Bangumi,
        sync_core::model::MediaKind::Anime,
        "254",
        "Samurai Champloo",
        Some("TV"),
        Some(2004),
    );
    seed_cli_provider_item(
        &store,
        sync_core::model::Provider::AniList,
        sync_core::model::MediaKind::Anime,
        "2",
        "Samurai Champloo",
        Some("TV"),
        Some(2004),
    );
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0},{"subject_id":254,"subject_type":"anime","collection_type":"collect","rate":9,"ep_status":26,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":2,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::AniList,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--test-write-transport",
        "--auto-link-local",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("apply output should be json");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["test_write_transport"], true);
    assert_eq!(report["live_provider_writes"], false);
    assert_eq!(report["local_auto_link"], true);
    assert_eq!(report["local_write_journal"], true);
    assert_eq!(report["credential_source"], "fixture_token_store");
    assert_eq!(report["summary"]["attempted"], 2);
    assert_eq!(report["summary"]["succeeded"], 2);
    assert_eq!(report["summary"]["failed"], 0);
    let write_requests = report["write_requests"]
        .as_array()
        .expect("write requests should be an array");
    assert_eq!(write_requests.len(), 2);
    assert!(write_requests.iter().all(|request| {
        request["provider"] == "anilist" && request["authorization"] == "redacted"
    }));
    assert!(write_requests
        .iter()
        .any(|request| request["kind"] == "add" && request["target_provider_entry_id"] == "1"));
    assert!(write_requests
        .iter()
        .any(|request| request["kind"] == "update" && request["target_provider_entry_id"] == "2"));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));
    assert!(!stdout.contains("Authorization"));
    assert!(!stdout.contains("Bearer"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let cowboy_bangumi = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("bangumi edge lookup should work")
        .expect("bangumi edge should exist");
    let cowboy_anilist = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    assert_eq!(cowboy_bangumi, cowboy_anilist);
    let champloo_bangumi = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "254",
        )
        .expect("bangumi edge lookup should work")
        .expect("bangumi edge should exist");
    let champloo_anilist = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "2",
        )
        .expect("anilist edge lookup should work")
        .expect("anilist edge should exist");
    assert_eq!(champloo_bangumi, champloo_anilist);
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "253",
            )
            .expect("entry lookup should work")
            .expect("entry should exist")
            .work_id,
        Some(cowboy_bangumi)
    );
    assert_eq!(
        store
            .collection_entry_details(
                "fixture-account",
                sync_core::model::Provider::AniList,
                sync_core::model::MediaKind::Anime,
                "2",
            )
            .expect("entry lookup should work")
            .expect("entry should exist")
            .work_id,
        Some(champloo_anilist)
    );
    let journals = store
        .write_journal_entries("fixture-account", sync_core::model::Provider::AniList)
        .expect("anilist journal should query");
    assert_eq!(journals.len(), 2);
    assert!(journals
        .iter()
        .all(|journal| journal.result_status == sync_core::store::WriteJournalStatus::Succeeded));
    assert!(journals
        .iter()
        .any(|journal| journal.operation_id.contains("-add-anilist-anime-1-")));
    assert!(journals.iter().any(|journal| journal
        .operation_id
        .contains("-update-anilist-anime-2-score-")));
}

#[test]
fn apply_requires_test_write_transport_without_opening_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-requires-test-transport-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("missing-sync-v2-apply.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "apply option validation must not create a missing db"
    );
    assert!(stderr.contains("missing --test-write-transport"));
}

#[test]
fn apply_rejects_live_write_refresh_and_browser_flags_before_opening_database() {
    for flag in [
        "--write",
        "--delete",
        "--refresh",
        "--reauthorize",
        "--open-browser",
        "--exchange-token",
        "--client-secret",
        "--authorization-code",
        "--access-token",
        "--refresh-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
        "--refresh-snapshots",
    ] {
        let db_path = std::env::temp_dir().join(format!(
            "bangumi-sync-apply-rejects-{}-{}.sqlite3",
            std::process::id(),
            flag.trim_start_matches("--")
        ));
        let _ = std::fs::remove_file(&db_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "apply",
            "--test-write-transport",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "anime",
            "--providers",
            "bangumi,anilist",
            flag,
            "secret-or-path",
        ]);

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(
            !db_path.exists(),
            "{flag} should be rejected before opening db"
        );
        assert!(stderr.contains("disabled for apply"), "{flag}: {stderr}");
        assert!(stderr.contains(flag), "{flag}: {stderr}");
    }
}

#[test]
fn apply_fixture_response_requires_post_write_verification_before_opening_database() {
    let db_path = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-fixture-response-requires-verify-{}.sqlite3",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&db_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        "anilist:anime:/tmp/does-not-matter.json",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "apply option validation must not create a missing db"
    );
    assert!(stderr.contains("--fixture-response requires --verify-post-write"));
}

#[test]
fn apply_refresh_required_does_not_read_corrupt_bundle_and_records_failed_journal() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-apply-refresh-required-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-apply-refresh-required.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bundle_path,
        "{not-json bangumi-apply-access-secret fixture-secret-ref",
    )
    .expect("corrupt bundle should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    let bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"do","rate":0,"ep_status":12,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("bangumi fixture should parse");
    let anilist = sync_core::provider::parse_anilist_collection_fixture(
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"CURRENT","score":70,"progress":12,"progressVolumes":null}]}]}}}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anilist fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &bangumi)
        .expect("bangumi snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &anilist)
        .expect("anilist snapshot should import");
    seed_refresh_required_cli_credential(
        &store,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "apply",
        "--test-write-transport",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "anilist,bangumi",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("ProviderCredentialRefreshRequired"));
    assert!(!stderr.contains("bangumi-apply-access-secret"));
    assert!(!stderr.contains("fixture-secret-ref"));
    assert!(!stderr.contains("credential_store_ref"));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let journals = store
        .write_journal_entries("fixture-account", sync_core::model::Provider::Bangumi)
        .expect("write journal should query");
    assert_eq!(journals.len(), 1);
    assert_eq!(
        journals[0].result_status,
        sync_core::store::WriteJournalStatus::Failed
    );
}

#[test]
fn plan_refresh_snapshots_redacts_corrupt_test_credential_bundle_errors() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-corrupt-bundle-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-refresh-corrupt-bundle.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bundle_path,
        "{not-json bangumi-bundle-access-secret fixture-secret-ref",
    )
    .expect("corrupt bundle should write");
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "fixture-secret-ref".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_259_200,
            refresh_token:
                sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("failed to refresh snapshots and build dry-run plan"));
    assert!(stderr.contains("CredentialSecretStore"));
    assert!(stderr.contains("credential bundle envelope is invalid"));
    assert!(!stderr.contains("bangumi-bundle-access-secret"));
    assert!(!stderr.contains("fixture-secret-ref"));
    assert!(!stderr.contains("credential_store_ref"));
    assert!(!stderr.contains("access_token"));
    assert!(!stderr.contains("refresh_token"));
}

#[test]
fn plan_test_credential_bundle_requires_refresh_snapshots_without_opening_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-test-bundle-requires-refresh-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("missing-sync-v2.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let _ = std::fs::remove_file(&db_path);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "test credential bundle parse failure must not create a database"
    );
    assert!(stderr.contains("--test-credential-bundle requires --refresh-snapshots"));
}

#[test]
fn plan_refresh_snapshots_does_not_read_test_credential_bundle_when_refresh_required() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-expired-bundle-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-refresh-expired-bundle.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bundle_path,
        "{not-json bangumi-bundle-access-secret fixture-secret-ref",
    )
    .expect("corrupt bundle should write");
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            auth_flow: sync_core::provider::ProviderAuthFlow::AuthorizationCode,
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "fixture-secret-ref".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_001,
            refresh_token:
                sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry(),
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["outcome"], "credentials_required");
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["imports"][0]["outcome"], "refresh_required");
    assert!(report["plan"].is_null());
    assert!(!stdout.contains("bangumi-bundle-access-secret"));
    assert!(!stdout.contains("fixture-secret-ref"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        0
    );
}

#[test]
fn plan_refresh_snapshots_auto_refreshes_expired_bundle_credential_with_test_token_transport() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-auto-refresh-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-refresh-auto-refresh.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 200000,
            "access_token": "auto-refreshed-access-secret",
            "refresh_token": "auto-rotated-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let stale_ref = seed_refresh_required_cli_bundle_credential(
        &store,
        &bundle_path,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "stale-bangumi-access-secret",
        "stale-bangumi-refresh-secret",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--test-token-transport",
        "--token-response",
        &format!("bangumi:{}", token_response_path.display()),
        "--refresh-client-id",
        "bangumi:bangumi-client",
        "--refresh-client-secret",
        "bangumi:bangumi-secret",
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["outcome"], "planned");
    assert_eq!(report["credential_source"], "test_credential_bundle");
    assert_eq!(report["credential_bundle_codec"], "test-reversing");
    assert_eq!(report["test_token_transport"], true);
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["imports"][0]["outcome"], "imported");
    assert!(report["plan"]["actions"]
        .as_array()
        .expect("actions")
        .is_empty());
    assert!(!stdout.contains("stale-bangumi-access-secret"));
    assert!(!stdout.contains("stale-bangumi-refresh-secret"));
    assert!(!stdout.contains("auto-refreshed-access-secret"));
    assert!(!stdout.contains("auto-rotated-refresh-secret"));
    assert!(!stdout.contains("bangumi-secret"));
    assert!(!stdout.contains(&stale_ref));
    assert!(!stdout.contains(bundle_path.to_str().expect("utf8 bundle path")));
    assert!(!stdout.contains("credential_store_ref"));
    assert!(!stdout.contains("access_token"));
    assert!(!stdout.contains("refresh_token"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::Bangumi, "fixture-account")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_ne!(credential.credential_store_ref, stale_ref);
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_200_000);
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));
    for output in [&stdout, &stderr] {
        assert!(!output.contains(&credential.credential_store_ref));
    }
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        1
    );
}

#[test]
fn plan_refresh_snapshots_auto_refreshes_expired_encrypted_bundle_credential_with_test_token_transport(
) {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-auto-refresh-encrypted-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-refresh-auto-refresh-encrypted.sqlite3");
    let bundle_path = temp_dir.join("encrypted-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let passphrase_env = format!(
        "BANGUMI_SYNC_TEST_PLAN_AUTO_REFRESH_PASSPHRASE_{}",
        std::process::id()
    );
    let passphrase = "plan auto refresh encrypted bundle passphrase secret";
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&bundle_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &token_response_path,
        r#"{
            "token_type": "Bearer",
            "expires_in": 200000,
            "access_token": "encrypted-plan-auto-refreshed-access-secret",
            "refresh_token": "encrypted-plan-auto-rotated-refresh-secret"
        }"#,
    )
    .expect("token response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    let stale_ref = seed_refresh_required_cli_encrypted_bundle_credential(
        &store,
        &bundle_path,
        passphrase,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
        "encrypted-plan-stale-bangumi-access-secret",
        "encrypted-plan-stale-bangumi-refresh-secret",
    );
    drop(store);

    std::env::set_var(&passphrase_env, passphrase);
    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--credential-bundle-passphrase-env",
        passphrase_env.as_str(),
        "--test-token-transport",
        "--token-response",
        &format!("bangumi:{}", token_response_path.display()),
        "--refresh-client-id",
        "bangumi:bangumi-client",
        "--refresh-client-secret",
        "bangumi:bangumi-secret",
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);
    std::env::remove_var(&passphrase_env);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["outcome"], "planned");
    assert_eq!(report["credential_source"], "file_credential_bundle");
    assert_eq!(
        report["credential_bundle_codec"],
        "passphrase-argon2id-xchacha20poly1305-v1"
    );
    assert_eq!(report["test_token_transport"], true);
    assert_eq!(report["token_request_count"], 1);
    assert_eq!(report["imports"][0]["outcome"], "imported");
    assert!(report["plan"]["actions"]
        .as_array()
        .expect("actions")
        .is_empty());
    for output in [&stdout, &stderr] {
        assert!(!output.contains("encrypted-plan-stale-bangumi-access-secret"));
        assert!(!output.contains("encrypted-plan-stale-bangumi-refresh-secret"));
        assert!(!output.contains("encrypted-plan-auto-refreshed-access-secret"));
        assert!(!output.contains("encrypted-plan-auto-rotated-refresh-secret"));
        assert!(!output.contains("bangumi-secret"));
        assert!(!output.contains(passphrase));
        assert!(!output.contains(&stale_ref));
        assert!(!output.contains(bundle_path.to_str().expect("utf8 bundle path")));
        assert!(!output.contains(passphrase_env.as_str()));
        assert!(!output.contains("credential_store_ref"));
        assert!(!output.contains("Authorization"));
        assert!(!output.contains("Bearer"));
        assert!(!output.contains("access_token"));
        assert!(!output.contains("refresh_token"));
    }

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let credential = store
        .provider_credential(sync_core::model::Provider::Bangumi, "fixture-account")
        .expect("credential lookup should work")
        .expect("credential metadata should exist");
    assert_ne!(credential.credential_store_ref, stale_ref);
    assert_eq!(credential.access_token_expires_at_epoch_secs, 1_700_200_000);
    assert_eq!(credential.last_refresh_at_epoch_secs, Some(1_700_000_000));
    for output in [&stdout, &stderr] {
        assert!(!output.contains(&credential.credential_store_ref));
    }
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        1
    );

    let mut bundle = sync_core::provider::FileCredentialBundleSecretStore::new(
        &bundle_path,
        sync_core::provider::PassphraseCredentialBundleCodec::new(passphrase),
    );
    let token = sync_core::provider::ProviderCredentialAccessTokenStore::get_provider_access_token(
        &mut bundle,
        sync_core::provider::ProviderCredentialSecretLookup {
            provider: sync_core::model::Provider::Bangumi,
            account_id: "fixture-account".to_owned(),
            credential_store_ref: credential.credential_store_ref,
        },
    )
    .expect("new access token should be readable through refreshed credential ref");
    assert_eq!(token.provider(), sync_core::model::Provider::Bangumi);
    assert!(!format!("{token:?}").contains("encrypted-plan-auto-refreshed-access-secret"));
}

#[test]
fn plan_refresh_snapshots_test_token_transport_requires_matching_refresh_client_id_before_opening_database(
) {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-missing-client-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("missing-sync-v2.sqlite3");
    let bundle_path = temp_dir.join("fixture-credential-bundle.json");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let token_response_path = temp_dir.join("refresh-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &token_response_path,
        r#"{"access_token":"missing-client-access-secret"}"#,
    )
    .expect("token response should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--test-credential-bundle",
        bundle_path.to_str().expect("utf8 bundle path"),
        "--test-token-transport",
        "--token-response",
        &format!("bangumi:{}", token_response_path.display()),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("--token-response requires matching --refresh-client-id"));
    assert!(!stderr.contains("missing-client-access-secret"));
    assert!(
        !db_path.exists(),
        "argument validation should fail before opening db"
    );
}

#[test]
fn plan_refresh_snapshots_reports_credentials_required_without_importing_fixture_responses() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-credentials-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-refresh-credentials.sqlite3");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_path,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["outcome"], "credentials_required");
    assert!(report["plan"].is_null());
    let imports = report["imports"].as_array().expect("imports array");
    assert_eq!(imports.len(), 2);
    assert!(imports
        .iter()
        .all(|summary| summary["outcome"] == "reauthorize"));

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert_eq!(
        store
            .collection_entry_count("fixture-account")
            .expect("collection count should load"),
        0
    );
}

#[test]
fn plan_refresh_snapshots_does_not_create_missing_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-missing-db-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("missing-sync-v2.sqlite3");
    let bangumi_path = temp_dir.join("bangumi-response.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(&bangumi_path, r#"{"data":[]}"#).expect("bangumi response should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[]}}}"#,
    )
    .expect("anilist response should write");

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_path.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
    ]);

    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        !db_path.exists(),
        "refresh-snapshots must not create a missing database"
    );
    assert!(stderr.contains("failed to open db"));
}

#[test]
fn plan_refresh_snapshots_accepts_multiple_fixture_pages_for_paginated_providers() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-refresh-pages-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-refresh-pages.sqlite3");
    let bangumi_page_one = temp_dir.join("bangumi-page-1.json");
    let bangumi_page_two = temp_dir.join("bangumi-page-2.json");
    let anilist_path = temp_dir.join("anilist-response.json");
    let _ = std::fs::remove_file(&db_path);
    std::fs::write(
        &bangumi_page_one,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    )
    .expect("bangumi page one should write");
    std::fs::write(&bangumi_page_two, r#"{"data":[]}"#).expect("bangumi page two should write");
    std::fs::write(
        &anilist_path,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[{"mediaId":1,"media":{"type":"ANIME"},"status":"COMPLETED","score":0,"progress":26,"progressVolumes":null}]}]}}}"#,
    )
    .expect("anilist response should write");

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("manual relation import should work");
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::Bangumi,
        "fixture-account",
    );
    seed_valid_cli_credential(
        &store,
        sync_core::model::Provider::AniList,
        "fixture-account",
    );
    drop(store);

    let (code, stdout, stderr) = run_cli([
        "sync-v2",
        "plan",
        "--dry-run",
        "--refresh-snapshots",
        "--db",
        db_path.to_str().expect("utf8 db path"),
        "--account",
        "fixture-account",
        "--kind",
        "anime",
        "--providers",
        "bangumi,anilist",
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_page_one.display()),
        "--fixture-response",
        &format!("bangumi:anime:{}", bangumi_page_two.display()),
        "--fixture-response",
        &format!("anilist:anime:{}", anilist_path.display()),
        "--limit",
        "1",
        "--now",
        "1700000000",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("sync dry-run output should be json");
    assert_eq!(report["outcome"], "planned");
    assert_eq!(report["imports"][0]["imported"], 1);
}

#[test]
fn plan_refresh_snapshots_rejects_provider_write_secret_and_browser_flags() {
    for flag in [
        "--apply",
        "--write",
        "--delete",
        "--refresh",
        "--reauthorize",
        "--open-browser",
        "--exchange-token",
        "--client-secret",
        "--authorization-code",
        "--access-token",
        "--refresh-token",
        "--csrf-token",
        "--cookie",
        "--session",
        "--browser-profile",
        "--remote-debugging-port",
    ] {
        let db_path = std::env::temp_dir().join(format!(
            "bangumi-sync-plan-refresh-rejects-{}-{}.sqlite3",
            std::process::id(),
            flag.trim_start_matches("--")
        ));
        let _ = std::fs::remove_file(&db_path);

        let (code, stdout, stderr) = run_cli([
            "sync-v2",
            "plan",
            "--dry-run",
            "--refresh-snapshots",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "anime",
            "--providers",
            "bangumi,anilist",
            flag,
            "secret-or-path",
        ]);

        assert_eq!(code, 2, "{flag} should be rejected");
        assert!(stdout.is_empty(), "{flag} should not write stdout");
        assert!(
            !db_path.exists(),
            "{flag} should be rejected before creating a db"
        );
        assert!(stderr.contains("disabled for plan refresh-snapshots"));
        assert!(stderr.contains(flag));
    }
}

#[test]
fn plan_dry_run_kind_all_reports_anime_and_manga_actions_for_three_providers() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-plan-all-kind-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-plan-all-kind.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Anime,
        "[[253, 1]]",
    )
    .expect("anime manual relation import should work");
    sync_core::identity::import_legacy_manual_relations(
        &store,
        sync_core::model::MediaKind::Manga,
        "[[9001, 2]]",
    )
    .expect("manga manual relation import should work");

    let anime_work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("anime work lookup should work")
        .expect("anime work should exist");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: anime_work_id,
            provider: sync_core::model::Provider::MyAnimeList,
            media_kind: sync_core::model::MediaKind::Anime,
            external_id: "5114".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("anime mal edge should insert");
    let manga_work_id = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Manga,
            "9001",
        )
        .expect("manga work lookup should work")
        .expect("manga work should exist");
    store
        .upsert_external_id_edge(sync_core::store::ExternalIdEdgeInput {
            work_id: manga_work_id,
            provider: sync_core::model::Provider::MyAnimeList,
            media_kind: sync_core::model::MediaKind::Manga,
            external_id: "300".to_owned(),
            source: "fixture".to_owned(),
            confidence: 1000,
            match_method: "manual".to_owned(),
            dataset_version: Some("fixture-v1".to_owned()),
        })
        .expect("manga mal edge should insert");

    let anime_bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
        sync_core::model::MediaKind::Anime,
    )
    .expect("anime bangumi fixture should parse");
    let manga_bangumi = sync_core::provider::parse_bangumi_collection_fixture(
        r#"{"data":[{"subject_id":9001,"subject_type":"book","collection_type":"do","rate":8,"ep_status":12,"vol_status":3}]}"#,
        sync_core::model::MediaKind::Manga,
    )
    .expect("manga bangumi fixture should parse");
    store
        .upsert_collection_snapshot("fixture-account", &anime_bangumi)
        .expect("anime snapshot should import");
    store
        .upsert_collection_snapshot("fixture-account", &manga_bangumi)
        .expect("manga snapshot should import");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "plan",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--account",
            "fixture-account",
            "--kind",
            "all",
            "--providers",
            "bangumi,anilist,myanimelist",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&stdout).expect("json report should parse");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["unmatched"], 0);
    assert!(report["conflicts"].as_array().expect("array").is_empty());

    let actions = report["actions"]
        .as_array()
        .expect("actions should be array");
    assert_eq!(actions.len(), 4);
    assert!(actions.iter().any(|action| {
        action["media_kind"] == "anime"
            && action["target_provider"] == "anilist"
            && action["target_provider_entry_id"] == "1"
    }));
    assert!(actions.iter().any(|action| {
        action["media_kind"] == "anime"
            && action["target_provider"] == "myanimelist"
            && action["target_provider_entry_id"] == "5114"
    }));
    assert!(actions.iter().any(|action| {
        action["media_kind"] == "manga"
            && action["target_provider"] == "anilist"
            && action["target_provider_entry_id"] == "2"
    }));
    assert!(actions.iter().any(|action| {
        action["media_kind"] == "manga"
            && action["target_provider"] == "myanimelist"
            && action["target_provider_entry_id"] == "300"
    }));
}

#[test]
fn inspect_match_reports_local_candidates_and_auto_match_decision() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-inspect-match-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-inspect-match.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_item(sync_core::store::ProviderItemInput {
            provider: sync_core::model::Provider::Bangumi,
            media_kind: sync_core::model::MediaKind::Anime,
            external_id: "975".to_owned(),
            canonical_title: "Cowboy Bebop".to_owned(),
            aliases: vec!["Kauboi Bibappu".to_owned()],
            format: Some("TV".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:975".to_owned(),
        })
        .expect("provider item should insert");
    store
        .upsert_provider_item(sync_core::store::ProviderItemInput {
            provider: sync_core::model::Provider::Bangumi,
            media_kind: sync_core::model::MediaKind::Anime,
            external_id: "976".to_owned(),
            canonical_title: "Cowboy Bebop Recap".to_owned(),
            aliases: Vec::new(),
            format: Some("SPECIAL".to_owned()),
            release_year: None,
            source_payload_hash: "sha256:976".to_owned(),
        })
        .expect("provider item should insert");
    store
        .upsert_manual_mapping(sync_core::store::ManualMappingInput {
            work_id: None,
            provider: sync_core::model::Provider::Bangumi,
            media_kind: sync_core::model::MediaKind::Anime,
            external_id: "976".to_owned(),
            decision: sync_core::store::ManualMappingDecision::Ignore,
            note: Some("known false positive".to_owned()),
        })
        .expect("ignore should insert");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "inspect-match",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--kind",
            "anime",
            "--query",
            "Cowboy Bebop",
            "--limit",
            "10",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let output = String::from_utf8(stdout).expect("output should be utf8");
    assert!(output.contains("sync-v2 inspect-match"));
    assert!(output.contains("dry-run"));
    assert!(output.contains("read-only"));
    assert!(output.contains("candidates=1"));
    assert!(output.contains("decision=accepted"));
    assert!(output.contains("bangumi anime 975"));
    assert!(output.contains("confidence=900"));
    assert!(output.contains("method=fts-title-exact"));
    assert!(output.contains("title=Cowboy Bebop"));
    assert!(!output.contains("976"));
    assert!(!output.contains("Recap"));
}

#[test]
fn inspect_match_reports_json_needs_review_for_ambiguous_candidates() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-inspect-match-json-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-inspect-match-json.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    for (provider, external_id) in [
        (sync_core::model::Provider::Bangumi, "975"),
        (sync_core::model::Provider::AniList, "1"),
    ] {
        store
            .upsert_provider_item(sync_core::store::ProviderItemInput {
                provider,
                media_kind: sync_core::model::MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: "Cowboy Bebop".to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year: None,
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "inspect-match",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--kind",
            "anime",
            "--query",
            "Cowboy Bebop",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&stdout).expect("json report should parse");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["query"], "Cowboy Bebop");
    assert_eq!(report["media_kind"], "anime");
    assert_eq!(report["decision"]["kind"], "needs_review");
    assert_eq!(report["decision"]["reason"], "ambiguous-top-confidence");
    let candidates = report["candidates"]
        .as_array()
        .expect("candidates should be array");
    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|candidate| candidate["confidence"] == 900));
    assert!(candidates
        .iter()
        .all(|candidate| candidate["match_method"] == "fts-title-exact"));
}

#[test]
fn inspect_match_uses_release_year_metadata_for_alias_exact_review() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-inspect-match-year-json-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-inspect-match-year-json.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    store
        .upsert_provider_item(sync_core::store::ProviderItemInput {
            provider: sync_core::model::Provider::Bangumi,
            media_kind: sync_core::model::MediaKind::Anime,
            external_id: "remake".to_owned(),
            canonical_title: "Cowboy Bebop Remake".to_owned(),
            aliases: vec!["Cowboy Bebop".to_owned()],
            format: Some("TV".to_owned()),
            release_year: Some(2021),
            source_payload_hash: "sha256:remake".to_owned(),
        })
        .expect("provider item should insert");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "inspect-match",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--kind",
            "anime",
            "--query",
            "Cowboy Bebop",
            "--release-year",
            "1998",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&stdout).expect("json report should parse");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["read_only"], true);
    assert_eq!(report["query"], "Cowboy Bebop");
    assert_eq!(report["query_release_year"], 1998);
    assert_eq!(report["decision"]["kind"], "needs_review");
    assert_eq!(report["decision"]["reason"], "low-confidence");
    let candidates = report["candidates"]
        .as_array()
        .expect("candidates should be array");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["release_year"], 2021);
    assert_eq!(candidates[0]["confidence"], 800);
    assert_eq!(
        candidates[0]["match_method"],
        "fts-alias-exact-year-mismatch"
    );
}

#[test]
fn inspect_match_rejects_invalid_release_year_without_opening_database() {
    let missing_db_path = std::env::temp_dir().join(format!(
        "bangumi-sync-inspect-match-invalid-year-{}.sqlite3",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&missing_db_path);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "inspect-match",
            "--dry-run",
            "--db",
            missing_db_path.to_str().expect("utf8 db path"),
            "--kind",
            "anime",
            "--query",
            "Cowboy Bebop",
            "--release-year",
            "98",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("stderr should be utf8");
    assert!(error.contains("invalid --release-year: 98"));
    assert!(
        !missing_db_path.exists(),
        "invalid release year should fail before opening db"
    );
}

#[test]
fn auto_link_dry_run_reports_planned_edges_without_modifying_database() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auto-link-dry-run-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auto-link-dry-run.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    for (provider, external_id) in [
        (sync_core::model::Provider::Bangumi, "253"),
        (sync_core::model::Provider::AniList, "1"),
    ] {
        store
            .upsert_provider_item(sync_core::store::ProviderItemInput {
                provider,
                media_kind: sync_core::model::MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: "Cowboy Bebop".to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year: Some(1998),
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }
    drop(store);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "auto-link",
            "--dry-run",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--provider",
            "bangumi",
            "--kind",
            "anime",
            "--id",
            "253",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&stdout).expect("json report should parse");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["read_only"], true);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["apply_local"], false);
    assert_eq!(report["provider"], "bangumi");
    assert_eq!(report["media_kind"], "anime");
    assert_eq!(report["external_id"], "253");
    assert_eq!(report["review_reason"], serde_json::Value::Null);
    let planned_edges = report["linked_edges"]
        .as_array()
        .expect("linked edges should be array");
    assert_eq!(planned_edges.len(), 2);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    assert_eq!(
        store
            .find_work_by_external_id(
                sync_core::model::Provider::Bangumi,
                sync_core::model::MediaKind::Anime,
                "253"
            )
            .expect("lookup should work"),
        None,
        "dry-run must not create identity edges"
    );
}

#[test]
fn auto_link_apply_local_writes_only_local_identity_edges() {
    let temp_dir = std::env::temp_dir().join(format!(
        "bangumi-sync-auto-link-apply-local-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
    let db_path = temp_dir.join("sync-v2-auto-link-apply-local.sqlite3");
    let _ = std::fs::remove_file(&db_path);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should open");
    for (provider, external_id) in [
        (sync_core::model::Provider::Bangumi, "253"),
        (sync_core::model::Provider::AniList, "1"),
    ] {
        store
            .upsert_provider_item(sync_core::store::ProviderItemInput {
                provider,
                media_kind: sync_core::model::MediaKind::Anime,
                external_id: external_id.to_owned(),
                canonical_title: "Cowboy Bebop".to_owned(),
                aliases: Vec::new(),
                format: Some("TV".to_owned()),
                release_year: Some(1998),
                source_payload_hash: format!("sha256:{external_id}"),
            })
            .expect("provider item should insert");
    }
    drop(store);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "auto-link",
            "--apply-local",
            "--db",
            db_path.to_str().expect("utf8 db path"),
            "--provider",
            "bangumi",
            "--kind",
            "anime",
            "--id",
            "253",
            "--format",
            "json",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&stderr));
    assert!(stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&stdout).expect("json report should parse");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["read_only"], false);
    assert_eq!(report["local_only"], true);
    assert_eq!(report["apply_local"], true);
    let linked_edges = report["linked_edges"]
        .as_array()
        .expect("linked edges should be array");
    assert_eq!(linked_edges.len(), 2);

    let store = sync_core::store::SqliteStore::open(&db_path).expect("db should reopen");
    let bangumi_work = store
        .find_work_by_external_id(
            sync_core::model::Provider::Bangumi,
            sync_core::model::MediaKind::Anime,
            "253",
        )
        .expect("lookup should work")
        .expect("bangumi edge should exist");
    let anilist_work = store
        .find_work_by_external_id(
            sync_core::model::Provider::AniList,
            sync_core::model::MediaKind::Anime,
            "1",
        )
        .expect("lookup should work")
        .expect("anilist edge should exist");
    assert_eq!(bangumi_work, anilist_work);
}

#[test]
fn auto_link_rejects_provider_write_flags_before_opening_database() {
    let missing_db_path = std::env::temp_dir().join(format!(
        "bangumi-sync-auto-link-write-flag-test-{}.sqlite3",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&missing_db_path);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = sync_cli::run(
        [
            "sync-v2",
            "auto-link",
            "--write",
            "--db",
            missing_db_path.to_str().expect("utf8 db path"),
            "--provider",
            "bangumi",
            "--kind",
            "anime",
            "--id",
            "253",
        ],
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let error = String::from_utf8(stderr).expect("stderr should be utf8");
    assert!(error.contains("provider writes and network calls are disabled for auto-link"));
    assert!(
        !missing_db_path.exists(),
        "rejected provider write flag should fail before opening db"
    );
}

fn seed_cli_provider_item(
    store: &sync_core::store::SqliteStore,
    provider: sync_core::model::Provider,
    media_kind: sync_core::model::MediaKind,
    external_id: &str,
    canonical_title: &str,
    format: Option<&str>,
    release_year: Option<u16>,
) {
    store
        .upsert_provider_item(sync_core::store::ProviderItemInput {
            provider,
            media_kind,
            external_id: external_id.to_owned(),
            canonical_title: canonical_title.to_owned(),
            aliases: Vec::new(),
            format: format.map(str::to_owned),
            release_year,
            source_payload_hash: format!(
                "fixture:{}:{}:{external_id}",
                provider.as_str(),
                media_kind.as_str()
            ),
        })
        .expect("provider item should insert");
}

fn seed_valid_cli_credential(
    store: &sync_core::store::SqliteStore,
    provider: sync_core::model::Provider,
    account_id: &str,
) {
    let expires_at = match provider {
        sync_core::model::Provider::AniList => 1_735_000_000,
        sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
            1_700_259_200
        }
    };

    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider,
            account_id: account_id.to_owned(),
            auth_flow: match provider {
                sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCode
                }
            },
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "fixture-secret-ref".to_owned(),
            access_token_expires_at_epoch_secs: expires_at,
            refresh_token: match provider {
                sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderRefreshTokenState::absent()
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry()
                }
            },
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
}

fn seed_refresh_required_cli_credential(
    store: &sync_core::store::SqliteStore,
    provider: sync_core::model::Provider,
    account_id: &str,
) {
    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider,
            account_id: account_id.to_owned(),
            auth_flow: match provider {
                sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCode
                }
            },
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: "fixture-secret-ref".to_owned(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: match provider {
                sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderRefreshTokenState::absent()
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry()
                }
            },
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");
}

fn seed_refresh_required_cli_bundle_credential(
    store: &sync_core::store::SqliteStore,
    bundle_path: &std::path::Path,
    provider: sync_core::model::Provider,
    account_id: &str,
    access_token: &str,
    refresh_token: &str,
) -> String {
    let mut secret_store = sync_core::provider::FileCredentialBundleSecretStore::new(
        bundle_path,
        CliTestCredentialBundleCodec,
    );
    let credential_store_ref =
        sync_core::provider::ProviderCredentialSecretStore::put_provider_tokens(
            &mut secret_store,
            sync_core::provider::ProviderCredentialSecretStoreInput {
                provider,
                account_id: account_id.to_owned(),
                token_type: "Bearer".to_owned(),
                access_token: access_token.to_owned(),
                refresh_token: Some(refresh_token.to_owned()),
            },
        )
        .expect("credential bundle token should store");

    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider,
            account_id: account_id.to_owned(),
            auth_flow: match provider {
                sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCode
                }
            },
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: credential_store_ref.clone(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: match provider {
                sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderRefreshTokenState::absent()
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry()
                }
            },
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    credential_store_ref
}

fn seed_refresh_required_cli_encrypted_bundle_credential(
    store: &sync_core::store::SqliteStore,
    bundle_path: &std::path::Path,
    passphrase: &str,
    provider: sync_core::model::Provider,
    account_id: &str,
    access_token: &str,
    refresh_token: &str,
) -> String {
    let mut secret_store = sync_core::provider::FileCredentialBundleSecretStore::new(
        bundle_path,
        sync_core::provider::PassphraseCredentialBundleCodec::new(passphrase),
    );
    let credential_store_ref =
        sync_core::provider::ProviderCredentialSecretStore::put_provider_tokens(
            &mut secret_store,
            sync_core::provider::ProviderCredentialSecretStoreInput {
                provider,
                account_id: account_id.to_owned(),
                token_type: "Bearer".to_owned(),
                access_token: access_token.to_owned(),
                refresh_token: Some(refresh_token.to_owned()),
            },
        )
        .expect("encrypted credential bundle token should store");

    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider,
            account_id: account_id.to_owned(),
            auth_flow: match provider {
                sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCode
                }
            },
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: credential_store_ref.clone(),
            access_token_expires_at_epoch_secs: 1_700_000_300,
            refresh_token: match provider {
                sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderRefreshTokenState::absent()
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry()
                }
            },
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    credential_store_ref
}

fn seed_valid_cli_bundle_credential(
    store: &sync_core::store::SqliteStore,
    bundle_path: &std::path::Path,
    provider: sync_core::model::Provider,
    account_id: &str,
    access_token: &str,
) -> String {
    let mut secret_store = sync_core::provider::FileCredentialBundleSecretStore::new(
        bundle_path,
        CliTestCredentialBundleCodec,
    );
    let credential_store_ref =
        sync_core::provider::ProviderCredentialSecretStore::put_provider_tokens(
            &mut secret_store,
            sync_core::provider::ProviderCredentialSecretStoreInput {
                provider,
                account_id: account_id.to_owned(),
                token_type: "Bearer".to_owned(),
                access_token: access_token.to_owned(),
                refresh_token: match provider {
                    sync_core::model::Provider::AniList => None,
                    sync_core::model::Provider::Bangumi
                    | sync_core::model::Provider::MyAnimeList => {
                        Some(format!("refresh-{access_token}"))
                    }
                },
            },
        )
        .expect("credential bundle token should store");

    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider,
            account_id: account_id.to_owned(),
            auth_flow: match provider {
                sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCode
                }
            },
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: credential_store_ref.clone(),
            access_token_expires_at_epoch_secs: match provider {
                sync_core::model::Provider::AniList => 1_735_000_000,
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
                    1_700_259_200
                }
            },
            refresh_token: match provider {
                sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderRefreshTokenState::absent()
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry()
                }
            },
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    credential_store_ref
}

fn seed_valid_cli_encrypted_bundle_credential(
    store: &sync_core::store::SqliteStore,
    bundle_path: &std::path::Path,
    passphrase: &str,
    provider: sync_core::model::Provider,
    account_id: &str,
    access_token: &str,
) -> String {
    let mut secret_store = sync_core::provider::FileCredentialBundleSecretStore::new(
        bundle_path,
        sync_core::provider::PassphraseCredentialBundleCodec::new(passphrase),
    );
    let credential_store_ref =
        sync_core::provider::ProviderCredentialSecretStore::put_provider_tokens(
            &mut secret_store,
            sync_core::provider::ProviderCredentialSecretStoreInput {
                provider,
                account_id: account_id.to_owned(),
                token_type: "Bearer".to_owned(),
                access_token: access_token.to_owned(),
                refresh_token: match provider {
                    sync_core::model::Provider::AniList => None,
                    sync_core::model::Provider::Bangumi
                    | sync_core::model::Provider::MyAnimeList => {
                        Some(format!("refresh-{access_token}"))
                    }
                },
            },
        )
        .expect("encrypted credential bundle token should store");

    store
        .upsert_provider_credential(sync_core::store::ProviderCredentialInput {
            provider,
            account_id: account_id.to_owned(),
            auth_flow: match provider {
                sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCodePkcePlain
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderAuthFlow::AuthorizationCode
                }
            },
            bootstrap_mode: sync_core::provider::ProviderAuthBootstrapMode::AuthBroker,
            credential_store_ref: credential_store_ref.clone(),
            access_token_expires_at_epoch_secs: match provider {
                sync_core::model::Provider::AniList => 1_735_000_000,
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
                    1_700_259_200
                }
            },
            refresh_token: match provider {
                sync_core::model::Provider::AniList => {
                    sync_core::provider::ProviderRefreshTokenState::absent()
                }
                sync_core::model::Provider::Bangumi | sync_core::model::Provider::MyAnimeList => {
                    sync_core::provider::ProviderRefreshTokenState::present_with_unknown_expiry()
                }
            },
            last_refresh_at_epoch_secs: Some(1_699_900_000),
            last_reauth_request_at_epoch_secs: None,
        })
        .expect("credential metadata should insert");

    credential_store_ref
}

#[derive(Debug, Clone)]
struct CliTestCredentialBundleCodec;

impl sync_core::provider::CredentialBundleSealCodec for CliTestCredentialBundleCodec {
    fn open_plaintext_bundle(
        &mut self,
        envelope: &sync_core::provider::CredentialBundleEnvelope,
    ) -> Result<String, sync_core::provider::CredentialBundleError> {
        if envelope.codec != "cli-fixture-reversing-v1" {
            return Err(sync_core::provider::CredentialBundleError::OpenFailed);
        }
        Ok(envelope.sealed_payload.chars().rev().collect())
    }

    fn seal_plaintext_bundle(
        &mut self,
        plaintext_json: &str,
    ) -> Result<
        sync_core::provider::CredentialBundleEnvelope,
        sync_core::provider::CredentialBundleError,
    > {
        Ok(sync_core::provider::CredentialBundleEnvelope {
            format_version: 1,
            codec: "cli-fixture-reversing-v1".to_owned(),
            sealed_payload: plaintext_json.chars().rev().collect(),
        })
    }
}
