use std::path::PathBuf;

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sync_core::model::Provider;
use sync_core::provider::{
    CredentialBundleEnvelope, CredentialBundleSealCodec, FileCredentialBundleSecretStore,
    PassphraseCredentialBundleCodec, ProviderCredentialAccessTokenStore,
    ProviderCredentialSecretLookup, ProviderCredentialSecretStore,
    ProviderCredentialSecretStoreInput,
};

#[test]
fn credential_bundle_store_round_trips_tokens_without_debug_or_file_leaks() {
    let path = temp_bundle_path("round-trip");
    let _ = std::fs::remove_file(&path);
    let mut store = FileCredentialBundleSecretStore::new(&path, ReversingCodec);

    let credential_ref = store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "access-token-secret".to_owned(),
            refresh_token: Some("refresh-token-secret".to_owned()),
        })
        .expect("tokens should store");

    assert!(credential_ref.starts_with("bundle:bangumi:bangumi-user-1:v1-"));
    let raw_file = std::fs::read_to_string(&path).expect("bundle file should exist");
    assert!(!raw_file.contains("access-token-secret"));
    assert!(!raw_file.contains("refresh-token-secret"));
    assert!(!format!("{store:?}").contains("access-token-secret"));
    assert!(!format!("{store:?}").contains("refresh-token-secret"));

    let access_token = store
        .get_provider_access_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: credential_ref.clone(),
        })
        .expect("access token should load");
    let refresh_token = store
        .get_provider_refresh_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: credential_ref,
        })
        .expect("refresh token should load");

    assert_eq!(access_token.provider(), Provider::Bangumi);
    assert_eq!(refresh_token, "refresh-token-secret");
    assert!(!format!("{access_token:?}").contains("access-token-secret"));
}

#[test]
fn credential_bundle_store_scopes_lookup_by_ref_provider_and_account() {
    let path = temp_bundle_path("scope");
    let _ = std::fs::remove_file(&path);
    let mut store = FileCredentialBundleSecretStore::new(&path, ReversingCodec);
    let credential_ref = store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "mal-access-secret".to_owned(),
            refresh_token: Some("mal-refresh-secret".to_owned()),
        })
        .expect("tokens should store");

    let wrong_provider = store
        .get_provider_access_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "mal-user-1".to_owned(),
            credential_store_ref: credential_ref.clone(),
        })
        .expect_err("wrong provider should be rejected");
    let wrong_account = store
        .get_provider_refresh_token(ProviderCredentialSecretLookup {
            provider: Provider::MyAnimeList,
            account_id: "other-user".to_owned(),
            credential_store_ref: credential_ref,
        })
        .expect_err("wrong account should be rejected");

    let wrong_provider = format!("{wrong_provider:?}");
    let wrong_account = format!("{wrong_account:?}");
    assert!(wrong_provider.contains("CredentialSecretStore"));
    assert!(wrong_account.contains("CredentialSecretStore"));
    assert!(!wrong_provider.contains("mal-access-secret"));
    assert!(!wrong_provider.contains("mal-refresh-secret"));
    assert!(!wrong_account.contains("mal-access-secret"));
    assert!(!wrong_account.contains("mal-refresh-secret"));
}

#[test]
fn credential_bundle_store_rotates_references_on_put() {
    let path = temp_bundle_path("rotation");
    let _ = std::fs::remove_file(&path);
    let mut store = FileCredentialBundleSecretStore::new(&path, ReversingCodec);

    let first_ref = store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "access-token-v1".to_owned(),
            refresh_token: Some("refresh-token-v1".to_owned()),
        })
        .expect("first tokens should store");
    let second_ref = store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "access-token-v2".to_owned(),
            refresh_token: Some("refresh-token-v2".to_owned()),
        })
        .expect("second tokens should store");

    assert!(first_ref.starts_with("bundle:bangumi:bangumi-user-1:v1-"));
    assert!(second_ref.starts_with("bundle:bangumi:bangumi-user-1:v2-"));
    let latest_refresh = store
        .get_provider_refresh_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: second_ref,
        })
        .expect("latest refresh token should load");
    assert_eq!(latest_refresh, "refresh-token-v2");
}

#[test]
fn credential_bundle_store_deletes_only_matching_ref_provider_and_account() {
    let path = temp_bundle_path("delete-ref");
    let _ = std::fs::remove_file(&path);
    let mut store = FileCredentialBundleSecretStore::new(&path, ReversingCodec);

    let first_ref = store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "access-token-v1".to_owned(),
            refresh_token: Some("refresh-token-v1".to_owned()),
        })
        .expect("first tokens should store");
    let second_ref = store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "access-token-v2".to_owned(),
            refresh_token: Some("refresh-token-v2".to_owned()),
        })
        .expect("second tokens should store");

    let deleted = store
        .delete_provider_tokens(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: second_ref.clone(),
        })
        .expect("matching ref should delete");

    assert!(deleted);
    assert!(store
        .get_provider_access_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: second_ref,
        })
        .is_err());
    let first_refresh = store
        .get_provider_refresh_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: first_ref,
        })
        .expect("other refs should remain");
    assert_eq!(first_refresh, "refresh-token-v1");
}

#[test]
fn credential_bundle_store_rejects_invalid_tokens_before_codec_or_file_write() {
    let path = temp_bundle_path("invalid-input");
    let _ = std::fs::remove_file(&path);
    let mut store = FileCredentialBundleSecretStore::new(&path, CountingCodec::default());

    let error = store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bad\naccount".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "access-token-secret".to_owned(),
            refresh_token: Some("refresh-token-secret".to_owned()),
        })
        .expect_err("invalid account should fail before codec");
    assert!(format!("{error:?}").contains("InvalidOAuthParameter"));
    let codec = store.into_inner();
    assert_eq!(codec.open_calls, 0);
    assert_eq!(codec.seal_calls, 0);
    assert!(!path.exists());

    let mut store = FileCredentialBundleSecretStore::new(&path, CountingCodec::default());
    let error = store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "bad\naccess-token-secret".to_owned(),
            refresh_token: Some("refresh-token-secret".to_owned()),
        })
        .expect_err("invalid access token should fail before codec");
    assert!(format!("{error:?}").contains("InvalidBearerToken"));
    let codec = store.into_inner();
    assert_eq!(codec.open_calls, 0);
    assert_eq!(codec.seal_calls, 0);
    assert!(!path.exists());
}

#[test]
fn credential_bundle_store_redacts_corrupt_bundle_and_codec_failures() {
    let path = temp_bundle_path("corrupt");
    let _ = std::fs::remove_file(&path);
    std::fs::write(
        &path,
        r#"{"format_version":1,"codec":"test","sealed_payload":"access-token-secret"}"#,
    )
    .expect("corrupt bundle should write");
    let mut store = FileCredentialBundleSecretStore::new(&path, FailingOpenCodec);

    let error = store
        .get_provider_refresh_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: "bundle:bangumi:bangumi-user-1:v1".to_owned(),
        })
        .expect_err("codec failure should be reported");
    let error = format!("{error:?}");
    assert!(error.contains("CredentialSecretStore"));
    assert!(!error.contains("access-token-secret"));

    std::fs::write(&path, "{not-json refresh-token-secret").expect("invalid envelope should write");
    let mut store = FileCredentialBundleSecretStore::new(&path, ReversingCodec);
    let error = store
        .get_provider_access_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: "bundle:bangumi:bangumi-user-1:v1".to_owned(),
        })
        .expect_err("invalid envelope should be reported");
    let error = format!("{error:?}");
    assert!(error.contains("CredentialSecretStore"));
    assert!(!error.contains("refresh-token-secret"));
}

#[test]
fn credential_bundle_store_refs_do_not_collide_for_stale_writers() {
    let path = temp_bundle_path("stale-writers");
    let _ = std::fs::remove_file(&path);
    let mut first_writer = FileCredentialBundleSecretStore::new(&path, ReversingCodec);
    let mut second_writer = FileCredentialBundleSecretStore::new(&path, ReversingCodec);

    let first_ref = first_writer
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "access-token-v1".to_owned(),
            refresh_token: Some("refresh-token-v1".to_owned()),
        })
        .expect("first writer should store");
    let second_ref = second_writer
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "access-token-v2".to_owned(),
            refresh_token: Some("refresh-token-v2".to_owned()),
        })
        .expect("second writer should store");

    assert_ne!(first_ref, second_ref);
}

#[test]
fn credential_bundle_passphrase_codec_round_trips_without_plaintext_file_leaks() {
    let path = temp_bundle_path("passphrase-round-trip");
    let _ = std::fs::remove_file(&path);
    let mut writer = FileCredentialBundleSecretStore::new(
        &path,
        PassphraseCredentialBundleCodec::new("correct horse battery staple"),
    );

    let credential_ref = writer
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "encrypted-access-token-secret".to_owned(),
            refresh_token: Some("encrypted-refresh-token-secret".to_owned()),
        })
        .expect("tokens should store through encrypted codec");

    let raw_file = std::fs::read_to_string(&path).expect("bundle should exist");
    assert!(raw_file.contains(PassphraseCredentialBundleCodec::CODEC_LABEL));
    assert!(!raw_file.contains("encrypted-access-token-secret"));
    assert!(!raw_file.contains("encrypted-refresh-token-secret"));
    assert!(!raw_file.contains("correct horse battery staple"));
    assert!(!format!("{writer:?}").contains("correct horse battery staple"));

    let mut reader = FileCredentialBundleSecretStore::new(
        &path,
        PassphraseCredentialBundleCodec::new("correct horse battery staple"),
    );
    let access_token = reader
        .get_provider_access_token(ProviderCredentialSecretLookup {
            provider: Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            credential_store_ref: credential_ref.clone(),
        })
        .expect("access token should decrypt");
    let refresh_token = reader
        .get_provider_refresh_token(ProviderCredentialSecretLookup {
            provider: Provider::AniList,
            account_id: "anilist-user-1".to_owned(),
            credential_store_ref: credential_ref,
        })
        .expect("refresh token should decrypt");

    assert_eq!(access_token.provider(), Provider::AniList);
    assert_eq!(refresh_token, "encrypted-refresh-token-secret");
    assert!(!format!("{access_token:?}").contains("encrypted-access-token-secret"));
}

#[test]
fn credential_bundle_passphrase_codec_rejects_wrong_passphrase_without_secret_leaks() {
    let path = temp_bundle_path("passphrase-wrong");
    let _ = std::fs::remove_file(&path);
    let mut writer = FileCredentialBundleSecretStore::new(
        &path,
        PassphraseCredentialBundleCodec::new("correct passphrase secret"),
    );
    let credential_ref = writer
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "wrong-pass-access-token-secret".to_owned(),
            refresh_token: Some("wrong-pass-refresh-token-secret".to_owned()),
        })
        .expect("tokens should store through encrypted codec");

    let mut reader = FileCredentialBundleSecretStore::new(
        &path,
        PassphraseCredentialBundleCodec::new("wrong passphrase secret"),
    );
    let error = reader
        .get_provider_access_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: credential_ref,
        })
        .expect_err("wrong passphrase should not decrypt");
    let error = format!("{error:?}");

    assert!(error.contains("CredentialSecretStore"));
    assert!(!error.contains("wrong-pass-access-token-secret"));
    assert!(!error.contains("wrong-pass-refresh-token-secret"));
    assert!(!error.contains("correct passphrase secret"));
    assert!(!error.contains("wrong passphrase secret"));
}

#[test]
fn credential_bundle_passphrase_codec_rejects_tampered_kdf_parameters_without_secret_leaks() {
    let path = temp_bundle_path("passphrase-kdf-tamper");
    let _ = std::fs::remove_file(&path);
    let mut writer = FileCredentialBundleSecretStore::new(
        &path,
        PassphraseCredentialBundleCodec::new("kdf parameter passphrase secret"),
    );
    let credential_ref = writer
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "kdf-tamper-access-token-secret".to_owned(),
            refresh_token: Some("kdf-tamper-refresh-token-secret".to_owned()),
        })
        .expect("tokens should store through encrypted codec");

    let mut envelope: CredentialBundleEnvelope =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("bundle should exist"))
            .expect("envelope should parse");
    let mut sealed: serde_json::Value =
        serde_json::from_str(&envelope.sealed_payload).expect("sealed payload should parse");
    sealed["memory_cost_kib"] = serde_json::json!(1);
    envelope.sealed_payload =
        serde_json::to_string(&sealed).expect("tampered sealed payload should encode");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&envelope).expect("envelope should encode"),
    )
    .expect("tampered bundle should write");

    let mut reader = FileCredentialBundleSecretStore::new(
        &path,
        PassphraseCredentialBundleCodec::new("kdf parameter passphrase secret"),
    );
    let error = reader
        .get_provider_refresh_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: "bangumi-user-1".to_owned(),
            credential_store_ref: credential_ref,
        })
        .expect_err("tampered kdf parameters should be rejected before derive");
    let error = format!("{error:?}");

    assert!(error.contains("CredentialSecretStore"));
    assert!(!error.contains("kdf-tamper-access-token-secret"));
    assert!(!error.contains("kdf-tamper-refresh-token-secret"));
    assert!(!error.contains("kdf parameter passphrase secret"));
}

#[test]
fn credential_bundle_passphrase_codec_rejects_tampered_salt_and_nonce_lengths_without_secret_leaks()
{
    for (label, field, bytes) in [
        ("short-salt", "salt_b64", vec![1_u8; 15]),
        ("long-salt", "salt_b64", vec![1_u8; 17]),
        ("short-nonce", "nonce_b64", vec![2_u8; 23]),
        ("long-nonce", "nonce_b64", vec![2_u8; 25]),
    ] {
        let path = temp_bundle_path(label);
        let _ = std::fs::remove_file(&path);
        let mut writer = FileCredentialBundleSecretStore::new(
            &path,
            PassphraseCredentialBundleCodec::new("salt nonce passphrase secret"),
        );
        let credential_ref = writer
            .put_provider_tokens(ProviderCredentialSecretStoreInput {
                provider: Provider::Bangumi,
                account_id: "bangumi-user-1".to_owned(),
                token_type: "Bearer".to_owned(),
                access_token: "salt-nonce-access-token-secret".to_owned(),
                refresh_token: Some("salt-nonce-refresh-token-secret".to_owned()),
            })
            .expect("tokens should store through encrypted codec");

        let mut envelope: CredentialBundleEnvelope =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("bundle should exist"))
                .expect("envelope should parse");
        let mut sealed: serde_json::Value =
            serde_json::from_str(&envelope.sealed_payload).expect("sealed payload should parse");
        sealed[field] = serde_json::json!(STANDARD_NO_PAD.encode(bytes));
        envelope.sealed_payload =
            serde_json::to_string(&sealed).expect("tampered sealed payload should encode");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&envelope).expect("envelope should encode"),
        )
        .expect("tampered bundle should write");

        let mut reader = FileCredentialBundleSecretStore::new(
            &path,
            PassphraseCredentialBundleCodec::new("salt nonce passphrase secret"),
        );
        let error = reader
            .get_provider_refresh_token(ProviderCredentialSecretLookup {
                provider: Provider::Bangumi,
                account_id: "bangumi-user-1".to_owned(),
                credential_store_ref: credential_ref,
            })
            .expect_err("tampered salt or nonce length should be rejected");
        let error = format!("{error:?}");

        assert!(error.contains("CredentialSecretStore"));
        assert!(!error.contains("salt-nonce-access-token-secret"));
        assert!(!error.contains("salt-nonce-refresh-token-secret"));
        assert!(!error.contains("salt nonce passphrase secret"));
    }
}

#[test]
fn credential_bundle_passphrase_codec_rejects_valid_payload_with_weak_kdf_parameters() {
    let path = temp_bundle_path("passphrase-weak-kdf");
    let _ = std::fs::remove_file(&path);
    let account_id = "bangumi-user-weak-kdf";
    let credential_ref = "bundle:bangumi:bangumi-user-weak-kdf:v1-weak";
    let plaintext = format!(
        r#"{{"entries":[{{"credential_store_ref":"{credential_ref}","provider":"bangumi","account_id":"{account_id}","token_type":"Bearer","access_token":"weak-kdf-access-token-secret","refresh_token":"weak-kdf-refresh-token-secret"}}]}}"#
    );
    let salt = [7_u8; 16];
    let nonce = [9_u8; 24];
    let mut key = [0_u8; 32];
    let weak_params = Params::new(8, 1, 1, Some(32)).expect("weak params should construct");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, weak_params)
        .hash_password_into(b"weak kdf passphrase secret", &salt, &mut key)
        .expect("weak key should derive");
    let ciphertext = XChaCha20Poly1305::new_from_slice(&key)
        .expect("cipher should initialize")
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_bytes())
        .expect("weak payload should encrypt");
    let sealed_payload = serde_json::json!({
        "kdf": "argon2id",
        "memory_cost_kib": 8,
        "time_cost": 1,
        "parallelism": 1,
        "salt_b64": STANDARD_NO_PAD.encode(salt),
        "nonce_b64": STANDARD_NO_PAD.encode(nonce),
        "ciphertext_b64": STANDARD_NO_PAD.encode(ciphertext),
    });
    let envelope = CredentialBundleEnvelope {
        format_version: 1,
        codec: PassphraseCredentialBundleCodec::CODEC_LABEL.to_owned(),
        sealed_payload: serde_json::to_string(&sealed_payload)
            .expect("sealed payload should encode"),
    };
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&envelope).expect("envelope should encode"),
    )
    .expect("weak kdf bundle should write");

    let mut reader = FileCredentialBundleSecretStore::new(
        &path,
        PassphraseCredentialBundleCodec::new("weak kdf passphrase secret"),
    );
    let error = reader
        .get_provider_access_token(ProviderCredentialSecretLookup {
            provider: Provider::Bangumi,
            account_id: account_id.to_owned(),
            credential_store_ref: credential_ref.to_owned(),
        })
        .expect_err("weak kdf parameters should be rejected even when ciphertext is valid");
    let error = format!("{error:?}");

    assert!(error.contains("CredentialSecretStore"));
    assert!(!error.contains("weak-kdf-access-token-secret"));
    assert!(!error.contains("weak-kdf-refresh-token-secret"));
    assert!(!error.contains("weak kdf passphrase secret"));
}

#[cfg(unix)]
#[test]
fn credential_bundle_store_writes_owner_only_file_permissions_for_passphrase_codec() {
    let path = temp_bundle_path("passphrase-permissions");
    let _ = std::fs::remove_file(&path);
    let mut store = FileCredentialBundleSecretStore::new(
        &path,
        PassphraseCredentialBundleCodec::new("owner only passphrase"),
    );

    store
        .put_provider_tokens(ProviderCredentialSecretStoreInput {
            provider: Provider::MyAnimeList,
            account_id: "mal-user-1".to_owned(),
            token_type: "Bearer".to_owned(),
            access_token: "owner-only-access-token-secret".to_owned(),
            refresh_token: Some("owner-only-refresh-token-secret".to_owned()),
        })
        .expect("tokens should store");

    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(&path)
            .expect("bundle metadata should exist")
            .permissions(),
    ) & 0o777;
    assert_eq!(mode, 0o600);
}

#[derive(Debug, Clone)]
struct ReversingCodec;

impl CredentialBundleSealCodec for ReversingCodec {
    fn open_plaintext_bundle(
        &mut self,
        envelope: &CredentialBundleEnvelope,
    ) -> Result<String, sync_core::provider::CredentialBundleError> {
        Ok(envelope.sealed_payload.chars().rev().collect())
    }

    fn seal_plaintext_bundle(
        &mut self,
        plaintext_json: &str,
    ) -> Result<CredentialBundleEnvelope, sync_core::provider::CredentialBundleError> {
        Ok(CredentialBundleEnvelope {
            format_version: 1,
            codec: "test-reversing-codec".to_owned(),
            sealed_payload: plaintext_json.chars().rev().collect(),
        })
    }
}

#[derive(Debug, Clone, Default)]
struct CountingCodec {
    open_calls: usize,
    seal_calls: usize,
}

impl CredentialBundleSealCodec for CountingCodec {
    fn open_plaintext_bundle(
        &mut self,
        _envelope: &CredentialBundleEnvelope,
    ) -> Result<String, sync_core::provider::CredentialBundleError> {
        self.open_calls += 1;
        Ok(r#"{"entries":[]}"#.to_owned())
    }

    fn seal_plaintext_bundle(
        &mut self,
        _plaintext_json: &str,
    ) -> Result<CredentialBundleEnvelope, sync_core::provider::CredentialBundleError> {
        self.seal_calls += 1;
        Ok(CredentialBundleEnvelope {
            format_version: 1,
            codec: "counting-codec".to_owned(),
            sealed_payload: "sealed".to_owned(),
        })
    }
}

#[derive(Debug, Clone)]
struct FailingOpenCodec;

impl CredentialBundleSealCodec for FailingOpenCodec {
    fn open_plaintext_bundle(
        &mut self,
        _envelope: &CredentialBundleEnvelope,
    ) -> Result<String, sync_core::provider::CredentialBundleError> {
        Err(sync_core::provider::CredentialBundleError::OpenFailed)
    }

    fn seal_plaintext_bundle(
        &mut self,
        _plaintext_json: &str,
    ) -> Result<CredentialBundleEnvelope, sync_core::provider::CredentialBundleError> {
        Err(sync_core::provider::CredentialBundleError::SealFailed)
    }
}

fn temp_bundle_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "bangumi-sync-credential-bundle-{label}-{}.json",
        std::process::id()
    ))
}
