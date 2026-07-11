mod anilist;
mod auth;
mod bangumi;
mod credential_bundle;
mod fixture;
mod myanimelist;
mod rate_limit;
mod read_request;
mod write_request;

pub use anilist::parse_anilist_collection_fixture;
pub(crate) use auth::validate_oauth_parameter;
pub use auth::{
    authorize_provider_read_request, authorize_provider_write_request,
    build_provider_authorization_request, build_provider_refresh_token_request,
    build_provider_token_exchange_request, exchange_provider_oauth_token,
    plan_provider_credential_action, provider_credential_capability,
    refresh_provider_credential_if_needed, refresh_provider_oauth_token,
    AuthorizedProviderReadRequest, AuthorizedProviderWriteRequest, ProviderAccessToken,
    ProviderAuthBootstrapMode, ProviderAuthError, ProviderAuthFlow,
    ProviderCredentialAccessTokenStore, ProviderCredentialAction, ProviderCredentialCapability,
    ProviderCredentialExchangeResult, ProviderCredentialRefreshInput,
    ProviderCredentialRefreshOutcome, ProviderCredentialRefreshPlanInput,
    ProviderCredentialSecretLookup, ProviderCredentialSecretStore,
    ProviderCredentialSecretStoreInput, ProviderCredentialState, ProviderHttpHeader,
    ProviderOAuthAuthorizationInput, ProviderOAuthAuthorizationRequest,
    ProviderOAuthEndpointSource, ProviderOAuthFormField, ProviderOAuthHttpClient,
    ProviderOAuthHttpRequest, ProviderOAuthHttpRequestMethod, ProviderOAuthHttpResponse,
    ProviderOAuthHttpTokenTransport, ProviderOAuthTokenExchangeInput,
    ProviderOAuthTokenHttpResponse, ProviderOAuthTokenRefreshInput, ProviderOAuthTokenRequest,
    ProviderOAuthTokenTransport, ProviderRefreshPolicy, ProviderRefreshTokenState,
};
pub use bangumi::parse_bangumi_collection_fixture;
pub use credential_bundle::{
    CredentialBundleEnvelope, CredentialBundleError, CredentialBundleSealCodec,
    FileCredentialBundleSecretStore, PassphraseCredentialBundleCodec,
};
pub use fixture::{ProviderCollectionSnapshot, ProviderFixtureError, ProviderSnapshotIdentityLink};
pub use myanimelist::parse_myanimelist_collection_fixture;
pub use rate_limit::{parse_provider_rate_limit, ProviderRateLimit};
pub use read_request::{
    build_bangumi_episode_collection_read_request, build_collection_read_request,
    fetch_authorized_bangumi_episode_collection_pages_with_transport,
    fetch_authorized_collection_snapshot_pages_with_transport,
    fetch_authorized_collection_snapshot_with_transport,
    fetch_collection_snapshot_pages_with_transport, fetch_collection_snapshot_with_transport,
    parse_bangumi_episode_collection_fixture, AuthorizedProviderReadTransport,
    BangumiEpisodeCollectionEntry, BangumiEpisodeCollectionSnapshot, ProviderHttpClient,
    ProviderHttpReadTransport, ProviderHttpRequest, ProviderHttpResponse, ProviderReadAuth,
    ProviderReadError, ProviderReadRequest, ProviderReadRequestError, ProviderReadRequestMethod,
    ProviderReadTransport,
};
pub use write_request::{
    build_bangumi_episode_progress_write_request,
    build_bangumi_episode_progress_write_request_for_action, build_provider_write_request,
    AuthorizedProviderWriteTransport, ProviderHttpWriteRequest, ProviderHttpWriteResponse,
    ProviderHttpWriteTransport, ProviderWriteAuth, ProviderWriteHttpClient, ProviderWriteRequest,
    ProviderWriteRequestError, ProviderWriteRequestMethod,
};
