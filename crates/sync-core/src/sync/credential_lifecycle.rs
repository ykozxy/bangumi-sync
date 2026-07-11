use crate::model::Provider;
use crate::provider::{
    plan_provider_credential_action, refresh_provider_credential_if_needed,
    validate_oauth_parameter, ProviderAccessToken, ProviderAuthBootstrapMode, ProviderAuthError,
    ProviderCredentialAccessTokenStore, ProviderCredentialAction, ProviderCredentialRefreshOutcome,
    ProviderCredentialRefreshPlanInput, ProviderCredentialSecretLookup,
    ProviderCredentialSecretStore, ProviderOAuthTokenTransport,
};
use crate::store::{
    ProviderCredentialRefreshAttemptFinalizeStatus, ProviderCredentialRefreshAttemptInput,
    SqliteStore, StoreError,
};

#[derive(Debug)]
pub enum StoredCredentialRefreshError {
    Store(StoreError),
    Auth(ProviderAuthError),
    RefreshCommitConflict {
        provider: Provider,
        account_id: String,
        attempt_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredProviderAccessTokenOutcome {
    Authorized(ProviderAccessToken),
    RefreshRequired,
    Reauthorize {
        preferred_mode: ProviderAuthBootstrapMode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredProviderCredentialSecretPreflightOutcome {
    AccessTokenReadable,
    RefreshTokenReadable,
    Reauthorize {
        preferred_mode: ProviderAuthBootstrapMode,
    },
}

impl From<StoreError> for StoredCredentialRefreshError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ProviderAuthError> for StoredCredentialRefreshError {
    fn from(error: ProviderAuthError) -> Self {
        Self::Auth(error)
    }
}

pub fn acquire_stored_provider_access_token<S>(
    store: &SqliteStore,
    provider: Provider,
    account_id: &str,
    now_unix_seconds: i64,
    secret_store: &mut S,
) -> Result<StoredProviderAccessTokenOutcome, StoredCredentialRefreshError>
where
    S: ProviderCredentialAccessTokenStore,
{
    validate_oauth_parameter(provider, "account_id", account_id)?;

    let Some(credential) = store.provider_credential(provider, account_id)? else {
        return Ok(StoredProviderAccessTokenOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
        });
    };
    validate_oauth_parameter(
        credential.provider,
        "credential_store_ref",
        &credential.credential_store_ref,
    )?;

    match plan_provider_credential_action(credential.credential_state(), now_unix_seconds) {
        ProviderCredentialAction::UseStoredAccessToken => {
            let access_token =
                secret_store.get_provider_access_token(ProviderCredentialSecretLookup {
                    provider: credential.provider,
                    account_id: credential.account_id,
                    credential_store_ref: credential.credential_store_ref,
                })?;
            Ok(StoredProviderAccessTokenOutcome::Authorized(access_token))
        }
        ProviderCredentialAction::RefreshWithProvider => {
            Ok(StoredProviderAccessTokenOutcome::RefreshRequired)
        }
        ProviderCredentialAction::Reauthorize { preferred_mode } => {
            Ok(StoredProviderAccessTokenOutcome::Reauthorize { preferred_mode })
        }
    }
}

pub fn preflight_stored_provider_credential_secret<S>(
    store: &SqliteStore,
    provider: Provider,
    account_id: &str,
    now_unix_seconds: i64,
    secret_store: &mut S,
) -> Result<StoredProviderCredentialSecretPreflightOutcome, StoredCredentialRefreshError>
where
    S: ProviderCredentialAccessTokenStore + ProviderCredentialSecretStore,
{
    validate_oauth_parameter(provider, "account_id", account_id)?;

    let Some(credential) = store.provider_credential(provider, account_id)? else {
        return Ok(
            StoredProviderCredentialSecretPreflightOutcome::Reauthorize {
                preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
            },
        );
    };
    match plan_provider_credential_action(credential.credential_state(), now_unix_seconds) {
        ProviderCredentialAction::UseStoredAccessToken => {
            validate_oauth_parameter(
                credential.provider,
                "credential_store_ref",
                &credential.credential_store_ref,
            )?;
            let lookup = ProviderCredentialSecretLookup {
                provider: credential.provider,
                account_id: credential.account_id.clone(),
                credential_store_ref: credential.credential_store_ref.clone(),
            };
            let _ = secret_store.get_provider_access_token(lookup)?;
            Ok(StoredProviderCredentialSecretPreflightOutcome::AccessTokenReadable)
        }
        ProviderCredentialAction::RefreshWithProvider => {
            validate_oauth_parameter(
                credential.provider,
                "credential_store_ref",
                &credential.credential_store_ref,
            )?;
            let lookup = ProviderCredentialSecretLookup {
                provider: credential.provider,
                account_id: credential.account_id.clone(),
                credential_store_ref: credential.credential_store_ref.clone(),
            };
            let refresh_token = secret_store.get_provider_refresh_token(lookup)?;
            validate_oauth_parameter(credential.provider, "refresh_token", &refresh_token)?;
            Ok(StoredProviderCredentialSecretPreflightOutcome::RefreshTokenReadable)
        }
        ProviderCredentialAction::Reauthorize { preferred_mode } => {
            Ok(StoredProviderCredentialSecretPreflightOutcome::Reauthorize { preferred_mode })
        }
    }
}

// Keep the secret store and token transport explicit at this security boundary.
#[allow(clippy::too_many_arguments)]
pub fn refresh_stored_provider_credential_if_needed<T, S>(
    store: &SqliteStore,
    provider: Provider,
    account_id: &str,
    client_id: String,
    client_secret: Option<String>,
    now_unix_seconds: i64,
    transport: &mut T,
    secret_store: &mut S,
) -> Result<ProviderCredentialRefreshOutcome, StoredCredentialRefreshError>
where
    T: ProviderOAuthTokenTransport,
    S: ProviderCredentialSecretStore,
{
    validate_oauth_parameter(provider, "account_id", account_id)?;
    validate_oauth_parameter(provider, "client_id", &client_id)?;
    if let Some(client_secret) = client_secret.as_deref() {
        validate_oauth_parameter(provider, "client_secret", client_secret)?;
    }

    let Some(credential) = store.provider_credential(provider, account_id)? else {
        return Ok(ProviderCredentialRefreshOutcome::Reauthorize {
            preferred_mode: ProviderAuthBootstrapMode::AuthBroker,
        });
    };

    let plan_input = ProviderCredentialRefreshPlanInput {
        provider: credential.provider,
        account_id: credential.account_id.clone(),
        bootstrap_mode: credential.bootstrap_mode,
        credential_store_ref: credential.credential_store_ref.clone(),
        access_token_expires_at_epoch_secs: credential.access_token_expires_at_epoch_secs,
        refresh_token: credential.refresh_token,
        client_id,
        client_secret,
    };
    if plan_provider_credential_action(credential.credential_state(), now_unix_seconds)
        != ProviderCredentialAction::RefreshWithProvider
    {
        return Ok(refresh_provider_credential_if_needed(
            plan_input,
            now_unix_seconds,
            transport,
            secret_store,
        )?);
    }

    let attempt_id =
        store.begin_provider_credential_refresh_attempt(ProviderCredentialRefreshAttemptInput {
            provider: credential.provider,
            account_id: credential.account_id.clone(),
            previous_credential_store_ref: credential.credential_store_ref,
            started_at_epoch_secs: now_unix_seconds,
        })?;
    let outcome = match refresh_provider_credential_if_needed(
        plan_input,
        now_unix_seconds,
        transport,
        secret_store,
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = store.fail_provider_credential_refresh_attempt(
                attempt_id,
                "auth_or_transport",
                now_unix_seconds,
            );
            return Err(error.into());
        }
    };

    if let ProviderCredentialRefreshOutcome::Refreshed(result) = &outcome {
        match store.finalize_provider_credential_refresh_attempt(
            attempt_id,
            result.clone(),
            now_unix_seconds,
        )? {
            ProviderCredentialRefreshAttemptFinalizeStatus::Committed => {}
            ProviderCredentialRefreshAttemptFinalizeStatus::Conflict => {
                return Err(StoredCredentialRefreshError::RefreshCommitConflict {
                    provider: result.provider,
                    account_id: result.account_id.clone(),
                    attempt_id,
                });
            }
        }
    };

    Ok(outcome)
}
