use std::collections::HashSet;
use std::fmt;

use serde_json::{json, Map, Value};

use crate::model::{CollectionStatus, MediaKind, Provider, SyncField};
use crate::sync::{PlannedAction, PlannedActionKind};

use super::auth::{AuthorizedProviderWriteRequest, ProviderHttpHeader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderWriteRequestMethod {
    Post,
    Patch,
    Put,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderWriteAuth {
    RequiredBearer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWriteRequest {
    pub provider: Provider,
    pub method: ProviderWriteRequestMethod,
    pub url: String,
    pub auth: ProviderWriteAuth,
    pub content_type: &'static str,
    pub body: String,
}

pub trait AuthorizedProviderWriteTransport {
    fn send_authorized_provider_write_request(
        &mut self,
        request: AuthorizedProviderWriteRequest,
    ) -> Result<String, String>;
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderHttpWriteRequest {
    pub provider: Provider,
    pub method: ProviderWriteRequestMethod,
    pub url: String,
    pub headers: Vec<ProviderHttpHeader>,
    pub body: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderHttpWriteResponse {
    pub status: u16,
    pub body: String,
}

pub trait ProviderWriteHttpClient {
    fn send_provider_write_http_request(
        &mut self,
        request: ProviderHttpWriteRequest,
    ) -> Result<ProviderHttpWriteResponse, String>;
}

pub struct ProviderHttpWriteTransport<C> {
    client: C,
}

impl<C> ProviderHttpWriteTransport<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub fn into_inner(self) -> C {
        self.client
    }
}

impl<C> AuthorizedProviderWriteTransport for ProviderHttpWriteTransport<C>
where
    C: ProviderWriteHttpClient,
{
    fn send_authorized_provider_write_request(
        &mut self,
        request: AuthorizedProviderWriteRequest,
    ) -> Result<String, String> {
        let provider = request.request.provider;
        let http_request = ProviderHttpWriteRequest {
            provider,
            method: request.request.method,
            url: request.request.url,
            headers: request.headers,
            body: request.request.body,
        };
        let response = self
            .client
            .send_provider_write_http_request(http_request)
            .map_err(|_| {
                format!(
                    "HTTP write transport failed for provider={}",
                    provider.as_str()
                )
            })?;

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

impl fmt::Debug for ProviderHttpWriteRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderHttpWriteRequest")
            .field("provider", &self.provider)
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("body", &"<present>")
            .finish()
    }
}

impl fmt::Debug for ProviderHttpWriteResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderHttpWriteResponse")
            .field("status", &self.status)
            .field("body", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderWriteRequestError {
    InvalidProviderEntryId {
        provider: Provider,
        value: String,
    },
    InvalidResolvedEpisodeId {
        provider: Provider,
        value: u64,
    },
    DuplicateResolvedEpisodeId {
        provider: Provider,
        episode_id: u64,
    },
    MissingFieldValue {
        provider: Provider,
        field: SyncField,
    },
    UnsupportedField {
        provider: Provider,
        media_kind: MediaKind,
        field: SyncField,
    },
    ResolvedEpisodeCountMismatch {
        provider: Provider,
        requested_progress: u32,
        resolved_episode_count: usize,
    },
}

pub fn build_provider_write_request(
    action: &PlannedAction,
) -> Result<ProviderWriteRequest, ProviderWriteRequestError> {
    match action.target_provider {
        Provider::Bangumi => build_bangumi_write_request(action),
        Provider::AniList => build_anilist_write_request(action),
        Provider::MyAnimeList => build_myanimelist_write_request(action),
    }
}

pub fn build_bangumi_episode_progress_write_request(
    subject_id: &str,
    requested_progress_episodes: u32,
    episode_ids: &[u64],
) -> Result<ProviderWriteRequest, ProviderWriteRequestError> {
    let subject_id = subject_id.trim();
    let parsed_subject_id = subject_id.parse::<u64>().map_err(|_| {
        ProviderWriteRequestError::InvalidProviderEntryId {
            provider: Provider::Bangumi,
            value: subject_id.to_owned(),
        }
    })?;
    if parsed_subject_id == 0 {
        return Err(ProviderWriteRequestError::InvalidProviderEntryId {
            provider: Provider::Bangumi,
            value: subject_id.to_owned(),
        });
    }
    if episode_ids.is_empty() {
        return Err(ProviderWriteRequestError::MissingFieldValue {
            provider: Provider::Bangumi,
            field: SyncField::ProgressEpisodes,
        });
    }
    if requested_progress_episodes as usize != episode_ids.len() {
        return Err(ProviderWriteRequestError::ResolvedEpisodeCountMismatch {
            provider: Provider::Bangumi,
            requested_progress: requested_progress_episodes,
            resolved_episode_count: episode_ids.len(),
        });
    }
    let mut unique_episode_ids = HashSet::new();
    for episode_id in episode_ids {
        if *episode_id == 0 {
            return Err(ProviderWriteRequestError::InvalidResolvedEpisodeId {
                provider: Provider::Bangumi,
                value: *episode_id,
            });
        }
        if !unique_episode_ids.insert(*episode_id) {
            return Err(ProviderWriteRequestError::DuplicateResolvedEpisodeId {
                provider: Provider::Bangumi,
                episode_id: *episode_id,
            });
        }
    }

    Ok(ProviderWriteRequest {
        provider: Provider::Bangumi,
        method: ProviderWriteRequestMethod::Patch,
        url: format!("https://api.bgm.tv/v0/users/-/collections/{subject_id}/episodes"),
        auth: ProviderWriteAuth::RequiredBearer,
        content_type: "application/json",
        body: json!({
            "episode_id": episode_ids,
            "type": 2,
        })
        .to_string(),
    })
}

pub fn build_bangumi_episode_progress_write_request_for_action(
    action: &PlannedAction,
    episode_ids: &[u64],
) -> Result<ProviderWriteRequest, ProviderWriteRequestError> {
    if action.target_provider != Provider::Bangumi
        || action.media_kind != MediaKind::Anime
        || action.kind != PlannedActionKind::UpdateEntry
    {
        return Err(unsupported(action, SyncField::ProgressEpisodes));
    }
    if action.field_updates != [SyncField::ProgressEpisodes] {
        let unsupported_field = action
            .field_updates
            .iter()
            .copied()
            .find(|field| *field != SyncField::ProgressEpisodes)
            .unwrap_or(SyncField::ProgressEpisodes);
        return Err(unsupported(action, unsupported_field));
    }
    let requested_progress = action.required_progress_episodes(Provider::Bangumi)?;

    build_bangumi_episode_progress_write_request(
        &action.target_provider_entry_id,
        requested_progress,
        episode_ids,
    )
}

fn build_anilist_write_request(
    action: &PlannedAction,
) -> Result<ProviderWriteRequest, ProviderWriteRequestError> {
    let media_id = action.provider_entry_id_as_i64()?;
    let mut variables = Map::new();
    variables.insert("mediaId".to_owned(), json!(media_id));

    for field in &action.field_updates {
        match field {
            SyncField::Status => {
                variables.insert("status".to_owned(), json!(anilist_status(action.status)));
            }
            SyncField::Score => {
                variables.insert(
                    "scoreRaw".to_owned(),
                    json!(action.required_score_hundred(Provider::AniList)?),
                );
            }
            SyncField::ProgressEpisodes => {
                if action.media_kind != MediaKind::Anime {
                    return Err(unsupported(action, *field));
                }
                variables.insert(
                    "progress".to_owned(),
                    json!(action.required_progress_episodes(Provider::AniList)?),
                );
            }
            SyncField::ProgressChapters => {
                if action.media_kind != MediaKind::Manga {
                    return Err(unsupported(action, *field));
                }
                variables.insert(
                    "progress".to_owned(),
                    json!(action.required_progress_chapters(Provider::AniList)?),
                );
            }
            SyncField::ProgressVolumes => {
                if action.media_kind != MediaKind::Manga {
                    return Err(unsupported(action, *field));
                }
                variables.insert(
                    "progressVolumes".to_owned(),
                    json!(action.required_progress_volumes(Provider::AniList)?),
                );
            }
            other => return Err(unsupported(action, *other)),
        }
    }

    let query = "mutation ($mediaId: Int, $status: MediaListStatus, $scoreRaw: Int, $progress: Int, $progressVolumes: Int) { SaveMediaListEntry(mediaId: $mediaId, status: $status, scoreRaw: $scoreRaw, progress: $progress, progressVolumes: $progressVolumes) { id status score progress progressVolumes } }";

    Ok(ProviderWriteRequest {
        provider: Provider::AniList,
        method: ProviderWriteRequestMethod::Post,
        url: "https://graphql.anilist.co".to_owned(),
        auth: ProviderWriteAuth::RequiredBearer,
        content_type: "application/json",
        body: json!({
            "query": query,
            "variables": Value::Object(variables),
        })
        .to_string(),
    })
}

fn build_bangumi_write_request(
    action: &PlannedAction,
) -> Result<ProviderWriteRequest, ProviderWriteRequestError> {
    let mut payload = Map::new();

    for field in &action.field_updates {
        match field {
            SyncField::Status => {
                payload.insert(
                    "type".to_owned(),
                    json!(bangumi_collection_type(action.status)),
                );
            }
            SyncField::Score => {
                payload.insert(
                    "rate".to_owned(),
                    json!(score_hundred_to_ten_point(
                        action.required_score_hundred(Provider::Bangumi)?
                    )),
                );
            }
            SyncField::ProgressChapters => {
                if action.media_kind != MediaKind::Manga {
                    return Err(unsupported(action, *field));
                }
                payload.insert(
                    "ep_status".to_owned(),
                    json!(action.required_progress_chapters(Provider::Bangumi)?),
                );
            }
            SyncField::ProgressVolumes => {
                if action.media_kind != MediaKind::Manga {
                    return Err(unsupported(action, *field));
                }
                payload.insert(
                    "vol_status".to_owned(),
                    json!(action.required_progress_volumes(Provider::Bangumi)?),
                );
            }
            other => return Err(unsupported(action, *other)),
        }
    }

    let method = match action.kind {
        PlannedActionKind::AddEntry => ProviderWriteRequestMethod::Post,
        PlannedActionKind::UpdateEntry => ProviderWriteRequestMethod::Patch,
    };

    Ok(ProviderWriteRequest {
        provider: Provider::Bangumi,
        method,
        url: format!(
            "https://api.bgm.tv/v0/users/-/collections/{}",
            action.target_provider_entry_id
        ),
        auth: ProviderWriteAuth::RequiredBearer,
        content_type: "application/json",
        body: Value::Object(payload).to_string(),
    })
}

fn build_myanimelist_write_request(
    action: &PlannedAction,
) -> Result<ProviderWriteRequest, ProviderWriteRequestError> {
    let media_path = match action.media_kind {
        MediaKind::Anime => "anime",
        MediaKind::Manga => "manga",
    };
    let mut form = Vec::new();

    for field in &action.field_updates {
        match field {
            SyncField::Status => {
                form.push((
                    "status",
                    myanimelist_status(action.media_kind, action.status).to_owned(),
                ));
            }
            SyncField::Score => {
                form.push((
                    "score",
                    score_hundred_to_ten_point(
                        action.required_score_hundred(Provider::MyAnimeList)?,
                    )
                    .to_string(),
                ));
            }
            SyncField::ProgressEpisodes => {
                if action.media_kind != MediaKind::Anime {
                    return Err(unsupported(action, *field));
                }
                form.push((
                    "num_watched_episodes",
                    action
                        .required_progress_episodes(Provider::MyAnimeList)?
                        .to_string(),
                ));
            }
            SyncField::ProgressChapters => {
                if action.media_kind != MediaKind::Manga {
                    return Err(unsupported(action, *field));
                }
                form.push((
                    "num_chapters_read",
                    action
                        .required_progress_chapters(Provider::MyAnimeList)?
                        .to_string(),
                ));
            }
            SyncField::ProgressVolumes => {
                if action.media_kind != MediaKind::Manga {
                    return Err(unsupported(action, *field));
                }
                form.push((
                    "num_volumes_read",
                    action
                        .required_progress_volumes(Provider::MyAnimeList)?
                        .to_string(),
                ));
            }
            other => return Err(unsupported(action, *other)),
        }
    }

    Ok(ProviderWriteRequest {
        provider: Provider::MyAnimeList,
        method: ProviderWriteRequestMethod::Put,
        url: format!(
            "https://api.myanimelist.net/v2/{}/{}/my_list_status",
            media_path, action.target_provider_entry_id
        ),
        auth: ProviderWriteAuth::RequiredBearer,
        content_type: "application/x-www-form-urlencoded",
        body: form_urlencoded(form),
    })
}

trait PlannedActionWriteExt {
    fn provider_entry_id_as_i64(&self) -> Result<i64, ProviderWriteRequestError>;
    fn required_score_hundred(&self, provider: Provider) -> Result<u8, ProviderWriteRequestError>;
    fn required_progress_episodes(
        &self,
        provider: Provider,
    ) -> Result<u32, ProviderWriteRequestError>;
    fn required_progress_chapters(
        &self,
        provider: Provider,
    ) -> Result<u32, ProviderWriteRequestError>;
    fn required_progress_volumes(
        &self,
        provider: Provider,
    ) -> Result<u32, ProviderWriteRequestError>;
}

impl PlannedActionWriteExt for PlannedAction {
    fn provider_entry_id_as_i64(&self) -> Result<i64, ProviderWriteRequestError> {
        self.target_provider_entry_id.parse::<i64>().map_err(|_| {
            ProviderWriteRequestError::InvalidProviderEntryId {
                provider: self.target_provider,
                value: self.target_provider_entry_id.clone(),
            }
        })
    }

    fn required_score_hundred(&self, provider: Provider) -> Result<u8, ProviderWriteRequestError> {
        self.score_hundred
            .ok_or(ProviderWriteRequestError::MissingFieldValue {
                provider,
                field: SyncField::Score,
            })
    }

    fn required_progress_episodes(
        &self,
        provider: Provider,
    ) -> Result<u32, ProviderWriteRequestError> {
        self.progress_episodes
            .ok_or(ProviderWriteRequestError::MissingFieldValue {
                provider,
                field: SyncField::ProgressEpisodes,
            })
    }

    fn required_progress_chapters(
        &self,
        provider: Provider,
    ) -> Result<u32, ProviderWriteRequestError> {
        self.progress_chapters
            .ok_or(ProviderWriteRequestError::MissingFieldValue {
                provider,
                field: SyncField::ProgressChapters,
            })
    }

    fn required_progress_volumes(
        &self,
        provider: Provider,
    ) -> Result<u32, ProviderWriteRequestError> {
        self.progress_volumes
            .ok_or(ProviderWriteRequestError::MissingFieldValue {
                provider,
                field: SyncField::ProgressVolumes,
            })
    }
}

fn unsupported(action: &PlannedAction, field: SyncField) -> ProviderWriteRequestError {
    ProviderWriteRequestError::UnsupportedField {
        provider: action.target_provider,
        media_kind: action.media_kind,
        field,
    }
}

fn anilist_status(status: CollectionStatus) -> &'static str {
    match status {
        CollectionStatus::InProgress => "CURRENT",
        CollectionStatus::Completed => "COMPLETED",
        CollectionStatus::Paused => "PAUSED",
        CollectionStatus::Dropped => "DROPPED",
        CollectionStatus::Planned => "PLANNING",
    }
}

fn bangumi_collection_type(status: CollectionStatus) -> u8 {
    match status {
        CollectionStatus::Planned => 1,
        CollectionStatus::Completed => 2,
        CollectionStatus::InProgress => 3,
        CollectionStatus::Paused => 4,
        CollectionStatus::Dropped => 5,
    }
}

fn myanimelist_status(media_kind: MediaKind, status: CollectionStatus) -> &'static str {
    match (media_kind, status) {
        (MediaKind::Anime, CollectionStatus::InProgress) => "watching",
        (MediaKind::Manga, CollectionStatus::InProgress) => "reading",
        (_, CollectionStatus::Completed) => "completed",
        (_, CollectionStatus::Paused) => "on_hold",
        (_, CollectionStatus::Dropped) => "dropped",
        (MediaKind::Anime, CollectionStatus::Planned) => "plan_to_watch",
        (MediaKind::Manga, CollectionStatus::Planned) => "plan_to_read",
    }
}

fn score_hundred_to_ten_point(score_hundred: u8) -> u8 {
    ((u16::from(score_hundred) + 5) / 10).min(10) as u8
}

fn form_urlencoded(items: Vec<(&str, String)>) -> String {
    items
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}
