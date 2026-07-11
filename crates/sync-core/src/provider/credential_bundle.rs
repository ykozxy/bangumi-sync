use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::model::Provider;

use super::auth::{
    validate_oauth_parameter, ProviderAccessToken, ProviderAuthError,
    ProviderCredentialAccessTokenStore, ProviderCredentialSecretLookup,
    ProviderCredentialSecretStore, ProviderCredentialSecretStoreInput,
};

static NEXT_CREDENTIAL_REF_NONCE: AtomicU64 = AtomicU64::new(1);

/// File envelope for a credential bundle.
///
/// `sync-core` does not provide encryption here. Confidentiality and integrity
/// depend entirely on the injected [`CredentialBundleSealCodec`].
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialBundleEnvelope {
    pub format_version: u16,
    pub codec: String,
    pub sealed_payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialBundleError {
    OpenFailed,
    SealFailed,
}

/// Opens and seals credential bundle plaintext.
///
/// Implementations are responsible for real encryption, authentication, and key
/// management. Test codecs may be reversible; production codecs must not be.
pub trait CredentialBundleSealCodec {
    fn open_plaintext_bundle(
        &mut self,
        envelope: &CredentialBundleEnvelope,
    ) -> Result<String, CredentialBundleError>;

    fn seal_plaintext_bundle(
        &mut self,
        plaintext_json: &str,
    ) -> Result<CredentialBundleEnvelope, CredentialBundleError>;
}

/// File-backed credential secret store for NAS/headless sync.
///
/// The file format and provider/account lookup semantics live here. Encryption
/// and tamper protection are delegated to the injected codec.
pub struct FileCredentialBundleSecretStore<C> {
    path: PathBuf,
    codec: C,
}

pub struct PassphraseCredentialBundleCodec {
    passphrase: Zeroizing<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CredentialBundlePlaintext {
    entries: Vec<CredentialBundleEntry>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PassphraseSealedPayload {
    kdf: String,
    memory_cost_kib: u32,
    time_cost: u32,
    parallelism: u32,
    salt_b64: String,
    nonce_b64: String,
    ciphertext_b64: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CredentialBundleEntry {
    credential_store_ref: String,
    provider: String,
    account_id: String,
    token_type: String,
    access_token: String,
    refresh_token: Option<String>,
}

impl<C> FileCredentialBundleSecretStore<C> {
    pub fn new(path: impl AsRef<Path>, codec: C) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            codec,
        }
    }

    pub fn into_inner(self) -> C {
        self.codec
    }
}

impl<C> fmt::Debug for FileCredentialBundleSecretStore<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileCredentialBundleSecretStore")
            .field("path", &self.path)
            .field("codec", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for CredentialBundleEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialBundleEnvelope")
            .field("format_version", &self.format_version)
            .field("codec", &self.codec)
            .field("sealed_payload", &"<redacted>")
            .finish()
    }
}

impl PassphraseCredentialBundleCodec {
    pub const CODEC_LABEL: &'static str = "passphrase-argon2id-xchacha20poly1305-v1";
    const KEY_LEN: usize = 32;
    const SALT_LEN: usize = 16;
    const NONCE_LEN: usize = 24;
    const MEMORY_COST_KIB: u32 = 19_456;
    const TIME_COST: u32 = 2;
    const PARALLELISM: u32 = 1;

    pub fn new(passphrase: impl Into<String>) -> Self {
        Self {
            passphrase: Zeroizing::new(passphrase.into()),
        }
    }
}

impl fmt::Debug for PassphraseCredentialBundleCodec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PassphraseCredentialBundleCodec")
            .field("passphrase", &"<redacted>")
            .finish()
    }
}

impl CredentialBundleSealCodec for PassphraseCredentialBundleCodec {
    fn open_plaintext_bundle(
        &mut self,
        envelope: &CredentialBundleEnvelope,
    ) -> Result<String, CredentialBundleError> {
        if envelope.codec != Self::CODEC_LABEL {
            return Err(CredentialBundleError::OpenFailed);
        }
        let sealed = serde_json::from_str::<PassphraseSealedPayload>(&envelope.sealed_payload)
            .map_err(|_| CredentialBundleError::OpenFailed)?;
        if sealed.kdf != "argon2id"
            || sealed.memory_cost_kib != Self::MEMORY_COST_KIB
            || sealed.time_cost != Self::TIME_COST
            || sealed.parallelism != Self::PARALLELISM
        {
            return Err(CredentialBundleError::OpenFailed);
        }
        let salt = decode_b64(&sealed.salt_b64).map_err(|_| CredentialBundleError::OpenFailed)?;
        if salt.len() != Self::SALT_LEN {
            return Err(CredentialBundleError::OpenFailed);
        }
        let nonce = decode_b64(&sealed.nonce_b64).map_err(|_| CredentialBundleError::OpenFailed)?;
        if nonce.len() != Self::NONCE_LEN {
            return Err(CredentialBundleError::OpenFailed);
        }
        let ciphertext =
            decode_b64(&sealed.ciphertext_b64).map_err(|_| CredentialBundleError::OpenFailed)?;
        let key = derive_passphrase_key(
            &self.passphrase,
            &salt,
            sealed.memory_cost_kib,
            sealed.time_cost,
            sealed.parallelism,
        )
        .map_err(|_| CredentialBundleError::OpenFailed)?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_slice())
            .map_err(|_| CredentialBundleError::OpenFailed)?;
        let plaintext = cipher
            .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| CredentialBundleError::OpenFailed)?;
        String::from_utf8(plaintext).map_err(|_| CredentialBundleError::OpenFailed)
    }

    fn seal_plaintext_bundle(
        &mut self,
        plaintext_json: &str,
    ) -> Result<CredentialBundleEnvelope, CredentialBundleError> {
        let mut salt = [0_u8; Self::SALT_LEN];
        let mut nonce = [0_u8; Self::NONCE_LEN];
        getrandom::getrandom(&mut salt).map_err(|_| CredentialBundleError::SealFailed)?;
        getrandom::getrandom(&mut nonce).map_err(|_| CredentialBundleError::SealFailed)?;
        let key = derive_passphrase_key(
            &self.passphrase,
            &salt,
            Self::MEMORY_COST_KIB,
            Self::TIME_COST,
            Self::PARALLELISM,
        )
        .map_err(|_| CredentialBundleError::SealFailed)?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_slice())
            .map_err(|_| CredentialBundleError::SealFailed)?;
        let ciphertext = cipher
            .encrypt(XNonce::from_slice(&nonce), plaintext_json.as_bytes())
            .map_err(|_| CredentialBundleError::SealFailed)?;
        let sealed = PassphraseSealedPayload {
            kdf: "argon2id".to_owned(),
            memory_cost_kib: Self::MEMORY_COST_KIB,
            time_cost: Self::TIME_COST,
            parallelism: Self::PARALLELISM,
            salt_b64: encode_b64(&salt),
            nonce_b64: encode_b64(&nonce),
            ciphertext_b64: encode_b64(&ciphertext),
        };
        Ok(CredentialBundleEnvelope {
            format_version: 1,
            codec: Self::CODEC_LABEL.to_owned(),
            sealed_payload: serde_json::to_string(&sealed)
                .map_err(|_| CredentialBundleError::SealFailed)?,
        })
    }
}

impl<C> ProviderCredentialAccessTokenStore for FileCredentialBundleSecretStore<C>
where
    C: CredentialBundleSealCodec,
{
    fn get_provider_access_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<ProviderAccessToken, ProviderAuthError> {
        let entry = self.find_entry(&lookup)?;
        ProviderAccessToken::new(lookup.provider, entry.access_token)
    }
}

impl<C> ProviderCredentialSecretStore for FileCredentialBundleSecretStore<C>
where
    C: CredentialBundleSealCodec,
{
    fn get_provider_refresh_token(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<String, ProviderAuthError> {
        let provider = lookup.provider;
        let entry = self.find_entry(&lookup)?;
        entry
            .refresh_token
            .ok_or(ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "refresh token is absent",
            })
    }

    fn put_provider_tokens(
        &mut self,
        input: ProviderCredentialSecretStoreInput,
    ) -> Result<String, ProviderAuthError> {
        validate_secret_store_input(&input)?;
        let mut plaintext = self.load_plaintext_for_write(input.provider)?;
        let next_ref = next_credential_ref(&plaintext, input.provider, &input.account_id);
        plaintext.entries.push(CredentialBundleEntry {
            credential_store_ref: next_ref.clone(),
            provider: input.provider.as_str().to_owned(),
            account_id: input.account_id,
            token_type: input.token_type,
            access_token: input.access_token,
            refresh_token: input.refresh_token,
        });
        self.save_plaintext(input.provider, &plaintext)?;
        Ok(next_ref)
    }
}

impl<C> FileCredentialBundleSecretStore<C>
where
    C: CredentialBundleSealCodec,
{
    pub fn delete_provider_tokens(
        &mut self,
        lookup: ProviderCredentialSecretLookup,
    ) -> Result<bool, ProviderAuthError> {
        validate_oauth_parameter(lookup.provider, "account_id", &lookup.account_id)?;
        validate_oauth_parameter(
            lookup.provider,
            "credential_store_ref",
            &lookup.credential_store_ref,
        )?;

        let mut plaintext = self.load_plaintext(lookup.provider)?;
        let original_len = plaintext.entries.len();
        plaintext.entries.retain(|entry| {
            !(entry.credential_store_ref == lookup.credential_store_ref
                && entry.provider == lookup.provider.as_str()
                && entry.account_id == lookup.account_id)
        });
        let deleted = plaintext.entries.len() != original_len;
        if deleted {
            self.save_plaintext(lookup.provider, &plaintext)?;
        }

        Ok(deleted)
    }

    fn find_entry(
        &mut self,
        lookup: &ProviderCredentialSecretLookup,
    ) -> Result<CredentialBundleEntry, ProviderAuthError> {
        let plaintext = self.load_plaintext(lookup.provider)?;
        plaintext
            .entries
            .into_iter()
            .find(|entry| {
                entry.credential_store_ref == lookup.credential_store_ref
                    && entry.provider == lookup.provider.as_str()
                    && entry.account_id == lookup.account_id
            })
            .ok_or(ProviderAuthError::CredentialSecretStore {
                provider: lookup.provider,
                reason: "credential ref not found",
            })
    }

    fn load_plaintext(
        &mut self,
        provider: Provider,
    ) -> Result<CredentialBundlePlaintext, ProviderAuthError> {
        let envelope_json = std::fs::read_to_string(&self.path).map_err(|_| {
            ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "credential bundle could not be read",
            }
        })?;
        let envelope =
            serde_json::from_str::<CredentialBundleEnvelope>(&envelope_json).map_err(|_| {
                ProviderAuthError::CredentialSecretStore {
                    provider,
                    reason: "credential bundle envelope is invalid",
                }
            })?;
        if envelope.format_version != 1 {
            return Err(ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "credential bundle version is unsupported",
            });
        }
        let plaintext_json = self.codec.open_plaintext_bundle(&envelope).map_err(|_| {
            ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "credential bundle could not be opened",
            }
        })?;
        serde_json::from_str::<CredentialBundlePlaintext>(&plaintext_json).map_err(|_| {
            ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "credential bundle plaintext is invalid",
            }
        })
    }

    fn load_plaintext_for_write(
        &mut self,
        provider: Provider,
    ) -> Result<CredentialBundlePlaintext, ProviderAuthError> {
        if !self.path.exists() {
            return Ok(CredentialBundlePlaintext {
                entries: Vec::new(),
            });
        }
        self.load_plaintext(provider)
    }

    fn save_plaintext(
        &mut self,
        provider: Provider,
        plaintext: &CredentialBundlePlaintext,
    ) -> Result<(), ProviderAuthError> {
        let plaintext_json = serde_json::to_string(plaintext).map_err(|_| {
            ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "credential bundle plaintext could not be encoded",
            }
        })?;
        let envelope = self
            .codec
            .seal_plaintext_bundle(&plaintext_json)
            .map_err(|_| ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "credential bundle could not be sealed",
            })?;
        let envelope_json = serde_json::to_string_pretty(&envelope).map_err(|_| {
            ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "credential bundle envelope could not be encoded",
            }
        })?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file =
            options
                .open(&self.path)
                .map_err(|_| ProviderAuthError::CredentialSecretStore {
                    provider,
                    reason: "credential bundle could not be written",
                })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| ProviderAuthError::CredentialSecretStore {
                    provider,
                    reason: "credential bundle could not be written",
                })?;
        }
        file.write_all(envelope_json.as_bytes()).map_err(|_| {
            ProviderAuthError::CredentialSecretStore {
                provider,
                reason: "credential bundle could not be written",
            }
        })
    }
}

fn validate_secret_store_input(
    input: &ProviderCredentialSecretStoreInput,
) -> Result<(), ProviderAuthError> {
    validate_oauth_parameter(input.provider, "account_id", &input.account_id)?;
    validate_oauth_parameter(input.provider, "token_type", &input.token_type)?;
    ProviderAccessToken::new(input.provider, &input.access_token)?;
    if let Some(refresh_token) = input.refresh_token.as_deref() {
        validate_oauth_parameter(input.provider, "refresh_token", refresh_token)?;
    }

    Ok(())
}

fn next_credential_ref(
    plaintext: &CredentialBundlePlaintext,
    provider: Provider,
    account_id: &str,
) -> String {
    let version = plaintext
        .entries
        .iter()
        .filter(|entry| entry.provider == provider.as_str() && entry.account_id == account_id)
        .count()
        + 1;
    let nonce = NEXT_CREDENTIAL_REF_NONCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "bundle:{}:{account_id}:v{version}-{nonce}",
        provider.as_str()
    )
}

fn derive_passphrase_key(
    passphrase: &str,
    salt: &[u8],
    memory_cost_kib: u32,
    time_cost: u32,
    parallelism: u32,
) -> Result<Zeroizing<[u8; PassphraseCredentialBundleCodec::KEY_LEN]>, CredentialBundleError> {
    let params = Params::new(
        memory_cost_kib,
        time_cost,
        parallelism,
        Some(PassphraseCredentialBundleCodec::KEY_LEN),
    )
    .map_err(|_| CredentialBundleError::OpenFailed)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; PassphraseCredentialBundleCodec::KEY_LEN]);
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut_slice())
        .map_err(|_| CredentialBundleError::OpenFailed)?;
    Ok(key)
}

fn encode_b64(bytes: &[u8]) -> String {
    STANDARD_NO_PAD.encode(bytes)
}

fn decode_b64(value: &str) -> Result<Vec<u8>, base64::DecodeError> {
    STANDARD_NO_PAD.decode(value)
}
