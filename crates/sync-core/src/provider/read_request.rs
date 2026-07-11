use std::fmt;

use serde::Deserialize;
use serde_json::json;

use crate::model::{MediaKind, Provider};

use super::auth::{
    authorize_provider_read_request, AuthorizedProviderReadRequest, ProviderAccessToken,
    ProviderAuthError, ProviderHttpHeader,
};
use super::{
    parse_anilist_collection_fixture, parse_bangumi_collection_fixture,
    parse_myanimelist_collection_fixture, ProviderCollectionSnapshot, ProviderFixtureError,
};

const BANGUMI_EPISODE_COLLECTION_TYPE_DONE: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderReadRequestMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderReadAuth {
    RequiredBearer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderReadRequest {
    pub provider: Provider,
    pub method: ProviderReadRequestMethod,
    pub url: String,
    pub auth: ProviderReadAuth,
    pub content_type: Option<&'static str>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderReadRequestError {
    MissingAccountId { provider: Provider },
    MissingSubjectId { provider: Provider },
    InvalidPageLimit { provider: Provider, limit: u16 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BangumiEpisodeCollectionSnapshot {
    subject_id: String,
    entries: Vec<BangumiEpisodeCollectionEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BangumiEpisodeCollectionEntry {
    episode_id: u64,
    sort: f64,
    collection_type: u8,
    updated_at_epoch_secs: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct BangumiEpisodeCollectionPage {
    data: Vec<BangumiEpisodeCollectionPageItem>,
}

#[derive(Debug, Deserialize)]
struct BangumiEpisodeCollectionPageItem {
    episode: BangumiEpisodeCollectionEpisode,
    #[serde(rename = "type")]
    collection_type: u8,
    #[serde(default)]
    updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct BangumiEpisodeCollectionEpisode {
    id: u64,
    sort: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderReadError {
    Request(ProviderReadRequestError),
    Auth(ProviderAuthError),
    Transport { provider: Provider, message: String },
    Parse(ProviderFixtureError),
}

impl BangumiEpisodeCollectionSnapshot {
    fn new(subject_id: &str, mut entries: Vec<BangumiEpisodeCollectionEntry>) -> Self {
        entries.sort_by(|left, right| {
            left.sort
                .total_cmp(&right.sort)
                .then(left.episode_id.cmp(&right.episode_id))
        });

        Self {
            subject_id: subject_id.to_owned(),
            entries,
        }
    }

    pub fn subject_id(&self) -> &str {
        &self.subject_id
    }

    pub fn entries(&self) -> &[BangumiEpisodeCollectionEntry] {
        &self.entries
    }

    pub fn episode_ids_for_done_prefix(&self, count: usize) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|entry| entry.collection_type == BANGUMI_EPISODE_COLLECTION_TYPE_DONE)
            .take(count)
            .map(|entry| entry.episode_id)
            .collect()
    }
}

impl BangumiEpisodeCollectionEntry {
    fn new(
        episode_id: u64,
        sort: f64,
        collection_type: u8,
        updated_at_epoch_secs: Option<i64>,
    ) -> Self {
        Self {
            episode_id,
            sort,
            collection_type,
            updated_at_epoch_secs,
        }
    }

    pub fn episode_id(&self) -> u64 {
        self.episode_id
    }

    pub fn sort(&self) -> f64 {
        self.sort
    }

    pub fn collection_type(&self) -> u8 {
        self.collection_type
    }

    pub fn updated_at_epoch_secs(&self) -> Option<i64> {
        self.updated_at_epoch_secs
    }
}

pub trait ProviderReadTransport {
    fn send_provider_read_request(
        &mut self,
        request: ProviderReadRequest,
    ) -> Result<String, String>;
}

pub trait AuthorizedProviderReadTransport {
    fn send_authorized_provider_read_request(
        &mut self,
        request: AuthorizedProviderReadRequest,
    ) -> Result<String, String>;
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderHttpRequest {
    pub provider: Provider,
    pub method: ProviderReadRequestMethod,
    pub url: String,
    pub headers: Vec<ProviderHttpHeader>,
    pub body: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderHttpResponse {
    pub status: u16,
    pub body: String,
}

pub trait ProviderHttpClient {
    fn send_provider_http_request(
        &mut self,
        request: ProviderHttpRequest,
    ) -> Result<ProviderHttpResponse, String>;
}

pub struct ProviderHttpReadTransport<C> {
    client: C,
}

impl<C> ProviderHttpReadTransport<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub fn into_inner(self) -> C {
        self.client
    }
}

impl<C> AuthorizedProviderReadTransport for ProviderHttpReadTransport<C>
where
    C: ProviderHttpClient,
{
    fn send_authorized_provider_read_request(
        &mut self,
        request: AuthorizedProviderReadRequest,
    ) -> Result<String, String> {
        let provider = request.request.provider;
        let http_request = ProviderHttpRequest {
            provider,
            method: request.request.method,
            url: request.request.url,
            headers: request.headers,
            body: request.request.body,
        };
        let response = self
            .client
            .send_provider_http_request(http_request)
            .map_err(|_| format!("HTTP transport failed for provider={}", provider.as_str()))?;

        if !(200..=299).contains(&response.status) {
            return Err(format!(
                "provider={} HTTP {}",
                provider.as_str(),
                response.status
            ));
        }

        Ok(response.body)
    }
}

impl fmt::Debug for ProviderHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderHttpRequest")
            .field("provider", &self.provider)
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("body", &self.body.as_ref().map(|_| "<present>"))
            .finish()
    }
}

impl fmt::Debug for ProviderHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderHttpResponse")
            .field("status", &self.status)
            .field("body", &"<redacted>")
            .finish()
    }
}

pub fn build_collection_read_request(
    provider: Provider,
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
    offset: u32,
) -> Result<ProviderReadRequest, ProviderReadRequestError> {
    let account_id = account_id.trim();
    if account_id.is_empty() {
        return Err(ProviderReadRequestError::MissingAccountId { provider });
    }
    if limit == 0 {
        return Err(ProviderReadRequestError::InvalidPageLimit { provider, limit });
    }

    match provider {
        Provider::Bangumi => Ok(build_bangumi_read_request(
            media_kind, account_id, limit, offset,
        )),
        Provider::AniList => Ok(build_anilist_read_request(media_kind, account_id)),
        Provider::MyAnimeList => Ok(build_myanimelist_read_request(
            media_kind, account_id, limit, offset,
        )),
    }
}

pub fn build_bangumi_episode_collection_read_request(
    subject_id: &str,
    limit: u16,
    offset: u32,
) -> Result<ProviderReadRequest, ProviderReadRequestError> {
    let subject_id = subject_id.trim();
    if subject_id.is_empty() {
        return Err(ProviderReadRequestError::MissingSubjectId {
            provider: Provider::Bangumi,
        });
    }
    if limit == 0 {
        return Err(ProviderReadRequestError::InvalidPageLimit {
            provider: Provider::Bangumi,
            limit,
        });
    }

    Ok(ProviderReadRequest {
        provider: Provider::Bangumi,
        method: ProviderReadRequestMethod::Get,
        url: format!(
            "https://api.bgm.tv/v0/users/-/collections/{}/episodes?episode_type=0&limit={limit}&offset={offset}",
            url_encode_component(subject_id)
        ),
        auth: ProviderReadAuth::RequiredBearer,
        content_type: None,
        body: None,
    })
}

pub fn fetch_collection_snapshot_with_transport<T>(
    transport: &mut T,
    provider: Provider,
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
    offset: u32,
) -> Result<ProviderCollectionSnapshot, ProviderReadError>
where
    T: ProviderReadTransport,
{
    let request = build_collection_read_request(provider, media_kind, account_id, limit, offset)
        .map_err(ProviderReadError::Request)?;
    let response = transport
        .send_provider_read_request(request)
        .map_err(|message| ProviderReadError::Transport { provider, message })?;

    parse_collection_response(provider, media_kind, &response).map_err(ProviderReadError::Parse)
}

pub fn fetch_authorized_collection_snapshot_with_transport<T>(
    transport: &mut T,
    token: &ProviderAccessToken,
    provider: Provider,
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
    offset: u32,
) -> Result<ProviderCollectionSnapshot, ProviderReadError>
where
    T: AuthorizedProviderReadTransport,
{
    let request = build_collection_read_request(provider, media_kind, account_id, limit, offset)
        .map_err(ProviderReadError::Request)?;
    let authorized =
        authorize_provider_read_request(&request, token).map_err(ProviderReadError::Auth)?;
    let response = transport
        .send_authorized_provider_read_request(authorized)
        .map_err(|message| ProviderReadError::Transport { provider, message })?;

    parse_collection_response(provider, media_kind, &response).map_err(ProviderReadError::Parse)
}

pub fn fetch_authorized_collection_snapshot_pages_with_transport<T>(
    transport: &mut T,
    token: &ProviderAccessToken,
    provider: Provider,
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
) -> Result<ProviderCollectionSnapshot, ProviderReadError>
where
    T: AuthorizedProviderReadTransport,
{
    if provider == Provider::AniList {
        return fetch_authorized_collection_snapshot_with_transport(
            transport, token, provider, media_kind, account_id, limit, 0,
        );
    }

    let mut offset = 0_u32;
    let mut raw_pages = Vec::new();
    let mut entries = Vec::new();

    loop {
        let request =
            build_collection_read_request(provider, media_kind, account_id, limit, offset)
                .map_err(ProviderReadError::Request)?;
        let authorized =
            authorize_provider_read_request(&request, token).map_err(ProviderReadError::Auth)?;
        let response = transport
            .send_authorized_provider_read_request(authorized)
            .map_err(|message| ProviderReadError::Transport { provider, message })?;
        let page = parse_collection_response(provider, media_kind, &response)
            .map_err(ProviderReadError::Parse)?;
        let page_count = page.entries().len();

        raw_pages.push(response);
        entries.extend(page.entries().iter().cloned());

        if page_count < usize::from(limit) {
            break;
        }
        offset = offset.saturating_add(u32::from(limit));
    }

    Ok(ProviderCollectionSnapshot::new(
        provider,
        media_kind,
        &raw_pages.join("\n"),
        entries,
    ))
}

pub fn fetch_authorized_bangumi_episode_collection_pages_with_transport<T>(
    transport: &mut T,
    token: &ProviderAccessToken,
    subject_id: &str,
    limit: u16,
) -> Result<BangumiEpisodeCollectionSnapshot, ProviderReadError>
where
    T: AuthorizedProviderReadTransport,
{
    let mut offset = 0_u32;
    let mut entries = Vec::new();

    loop {
        let request = build_bangumi_episode_collection_read_request(subject_id, limit, offset)
            .map_err(ProviderReadError::Request)?;
        let authorized =
            authorize_provider_read_request(&request, token).map_err(ProviderReadError::Auth)?;
        let response = transport
            .send_authorized_provider_read_request(authorized)
            .map_err(|message| ProviderReadError::Transport {
                provider: Provider::Bangumi,
                message,
            })?;
        let page = parse_bangumi_episode_collection_page(subject_id, &response)
            .map_err(ProviderReadError::Parse)?;
        let page_count = page.entries.len();

        entries.extend(page.entries);

        if page_count < usize::from(limit) {
            break;
        }
        offset = offset.saturating_add(u32::from(limit));
    }

    Ok(BangumiEpisodeCollectionSnapshot::new(subject_id, entries))
}

pub fn parse_bangumi_episode_collection_fixture(
    subject_id: &str,
    json: &str,
) -> Result<BangumiEpisodeCollectionSnapshot, ProviderFixtureError> {
    parse_bangumi_episode_collection_page(subject_id, json)
}

pub fn fetch_collection_snapshot_pages_with_transport<T>(
    transport: &mut T,
    provider: Provider,
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
) -> Result<ProviderCollectionSnapshot, ProviderReadError>
where
    T: ProviderReadTransport,
{
    if provider == Provider::AniList {
        return fetch_collection_snapshot_with_transport(
            transport, provider, media_kind, account_id, limit, 0,
        );
    }

    let mut offset = 0_u32;
    let mut raw_pages = Vec::new();
    let mut entries = Vec::new();

    loop {
        let request =
            build_collection_read_request(provider, media_kind, account_id, limit, offset)
                .map_err(ProviderReadError::Request)?;
        let response = transport
            .send_provider_read_request(request)
            .map_err(|message| ProviderReadError::Transport { provider, message })?;
        let page = parse_collection_response(provider, media_kind, &response)
            .map_err(ProviderReadError::Parse)?;
        let page_count = page.entries().len();

        raw_pages.push(response);
        entries.extend(page.entries().iter().cloned());

        if page_count < usize::from(limit) {
            break;
        }
        offset = offset.saturating_add(u32::from(limit));
    }

    Ok(ProviderCollectionSnapshot::new(
        provider,
        media_kind,
        &raw_pages.join("\n"),
        entries,
    ))
}

fn build_bangumi_read_request(
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
    offset: u32,
) -> ProviderReadRequest {
    let subject_type = match media_kind {
        MediaKind::Anime => 2,
        MediaKind::Manga => 1,
    };

    ProviderReadRequest {
        provider: Provider::Bangumi,
        method: ProviderReadRequestMethod::Get,
        url: format!(
            "https://api.bgm.tv/v0/users/{}/collections?subject_type={subject_type}&limit={limit}&offset={offset}",
            url_encode_component(account_id)
        ),
        auth: ProviderReadAuth::RequiredBearer,
        content_type: None,
        body: None,
    }
}

fn build_anilist_read_request(media_kind: MediaKind, account_id: &str) -> ProviderReadRequest {
    let media_type = match media_kind {
        MediaKind::Anime => "ANIME",
        MediaKind::Manga => "MANGA",
    };
    let query = "query ($userName: String, $type: MediaType) { MediaListCollection(userName: $userName, type: $type) { lists { entries { mediaId media { id type idMal } status score(format: POINT_100) progress progressVolumes updatedAt } } } }";

    ProviderReadRequest {
        provider: Provider::AniList,
        method: ProviderReadRequestMethod::Post,
        url: "https://graphql.anilist.co".to_owned(),
        auth: ProviderReadAuth::RequiredBearer,
        content_type: Some("application/json"),
        body: Some(
            json!({
                "query": query,
                "variables": {
                    "userName": account_id,
                    "type": media_type,
                },
            })
            .to_string(),
        ),
    }
}

fn build_myanimelist_read_request(
    media_kind: MediaKind,
    account_id: &str,
    limit: u16,
    offset: u32,
) -> ProviderReadRequest {
    let list_path = match media_kind {
        MediaKind::Anime => "animelist",
        MediaKind::Manga => "mangalist",
    };
    let fields = "media_type,list_status{status,score,num_episodes_watched,num_chapters_read,num_volumes_read,updated_at}";

    ProviderReadRequest {
        provider: Provider::MyAnimeList,
        method: ProviderReadRequestMethod::Get,
        url: format!(
            "https://api.myanimelist.net/v2/users/{}/{list_path}?fields={}&limit={limit}&offset={offset}",
            url_encode_component(account_id),
            url_encode_component(fields)
        ),
        auth: ProviderReadAuth::RequiredBearer,
        content_type: None,
        body: None,
    }
}

fn parse_collection_response(
    provider: Provider,
    media_kind: MediaKind,
    response: &str,
) -> Result<ProviderCollectionSnapshot, ProviderFixtureError> {
    match provider {
        Provider::Bangumi => parse_bangumi_collection_fixture(response, media_kind),
        Provider::AniList => parse_anilist_collection_fixture(response, media_kind),
        Provider::MyAnimeList => parse_myanimelist_collection_fixture(response, media_kind),
    }
}

fn parse_bangumi_episode_collection_page(
    subject_id: &str,
    response: &str,
) -> Result<BangumiEpisodeCollectionSnapshot, ProviderFixtureError> {
    let page: BangumiEpisodeCollectionPage =
        serde_json::from_str(response).map_err(|error| ProviderFixtureError::InvalidJson {
            provider: Provider::Bangumi,
            message: error.to_string(),
        })?;
    let entries = page
        .data
        .into_iter()
        .map(|item| {
            BangumiEpisodeCollectionEntry::new(
                item.episode.id,
                item.episode.sort,
                item.collection_type,
                item.updated_at,
            )
        })
        .collect::<Vec<_>>();

    Ok(BangumiEpisodeCollectionSnapshot::new(subject_id, entries))
}

fn url_encode_component(value: &str) -> String {
    let mut encoded = String::new();

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
}
