use serde_json::Value;
use sync_core::model::{CollectionStatus, MediaKind, Provider, SyncField};
use sync_core::provider::{
    authorize_provider_write_request, build_bangumi_episode_progress_write_request,
    build_bangumi_episode_progress_write_request_for_action, build_provider_write_request,
    AuthorizedProviderWriteTransport, ProviderAccessToken, ProviderHttpWriteRequest,
    ProviderHttpWriteResponse, ProviderHttpWriteTransport, ProviderWriteAuth,
    ProviderWriteHttpClient, ProviderWriteRequestError, ProviderWriteRequestMethod,
};
use sync_core::sync::{PlannedAction, PlannedActionKind, PlannedFieldChange};

#[test]
fn anilist_write_request_uses_save_media_list_entry_variables() {
    let action = planned_action(
        Provider::Bangumi,
        Provider::AniList,
        MediaKind::Anime,
        "1",
        CollectionStatus::Completed,
        Some(90),
        Some(26),
        None,
        None,
        vec![
            SyncField::Status,
            SyncField::Score,
            SyncField::ProgressEpisodes,
        ],
    );

    let request = build_provider_write_request(&action).expect("request should build");
    let body: Value = serde_json::from_str(&request.body).expect("body should be json");

    assert_eq!(request.provider, Provider::AniList);
    assert_eq!(request.method, ProviderWriteRequestMethod::Post);
    assert_eq!(request.auth, ProviderWriteAuth::RequiredBearer);
    assert_eq!(request.url, "https://graphql.anilist.co");
    assert_eq!(request.content_type, "application/json");
    assert!(body["query"]
        .as_str()
        .expect("query string")
        .contains("SaveMediaListEntry"));
    assert_eq!(body["variables"]["mediaId"], 1);
    assert_eq!(body["variables"]["status"], "COMPLETED");
    assert_eq!(body["variables"]["scoreRaw"], 90);
    assert_eq!(body["variables"]["progress"], 26);
    assert!(body["variables"].get("progressVolumes").is_none());
}

#[test]
fn anilist_score_write_keeps_hundred_point_precision() {
    let action = planned_action(
        Provider::Bangumi,
        Provider::AniList,
        MediaKind::Anime,
        "1",
        CollectionStatus::Completed,
        Some(85),
        None,
        None,
        None,
        vec![SyncField::Score],
    );

    let request = build_provider_write_request(&action).expect("request should build");
    let body: Value = serde_json::from_str(&request.body).expect("body should be json");

    assert_eq!(body["variables"]["scoreRaw"], 85);
}

#[test]
fn bangumi_score_write_rounds_hundred_point_to_ten_point_boundaries() {
    for (score_hundred, expected_rate) in [(84, 8), (85, 9), (95, 10)] {
        let action = planned_action(
            Provider::AniList,
            Provider::Bangumi,
            MediaKind::Anime,
            "253",
            CollectionStatus::Completed,
            Some(score_hundred),
            None,
            None,
            None,
            vec![SyncField::Score],
        );

        let request = build_provider_write_request(&action).expect("request should build");
        let body: Value = serde_json::from_str(&request.body).expect("body should be json");

        assert_eq!(body["rate"], expected_rate, "score_hundred={score_hundred}");
    }
}

#[test]
fn myanimelist_score_write_rounds_hundred_point_to_ten_point_boundaries() {
    for (score_hundred, expected_score) in [(84, "score=8"), (85, "score=9"), (95, "score=10")] {
        let action = planned_action(
            Provider::AniList,
            Provider::MyAnimeList,
            MediaKind::Anime,
            "5",
            CollectionStatus::Completed,
            Some(score_hundred),
            None,
            None,
            None,
            vec![SyncField::Score],
        );

        let request = build_provider_write_request(&action).expect("request should build");

        assert_eq!(
            request.body, expected_score,
            "score_hundred={score_hundred}"
        );
    }
}

#[test]
fn bangumi_manga_write_request_uses_collection_patch_payload() {
    let action = planned_action(
        Provider::AniList,
        Provider::Bangumi,
        MediaKind::Manga,
        "9001",
        CollectionStatus::InProgress,
        Some(80),
        None,
        Some(12),
        Some(3),
        vec![
            SyncField::Status,
            SyncField::Score,
            SyncField::ProgressChapters,
            SyncField::ProgressVolumes,
        ],
    );

    let request = build_provider_write_request(&action).expect("request should build");
    let body: Value = serde_json::from_str(&request.body).expect("body should be json");

    assert_eq!(request.provider, Provider::Bangumi);
    assert_eq!(request.method, ProviderWriteRequestMethod::Patch);
    assert_eq!(request.auth, ProviderWriteAuth::RequiredBearer);
    assert_eq!(
        request.url,
        "https://api.bgm.tv/v0/users/-/collections/9001"
    );
    assert_eq!(request.content_type, "application/json");
    assert_eq!(body["type"], 3);
    assert_eq!(body["rate"], 8);
    assert_eq!(body["ep_status"], 12);
    assert_eq!(body["vol_status"], 3);
    assert!(body.get("comment").is_none());
    assert!(body.get("tags").is_none());
}

#[test]
fn myanimelist_manga_write_request_uses_form_status_and_progress_fields() {
    let action = planned_action(
        Provider::Bangumi,
        Provider::MyAnimeList,
        MediaKind::Manga,
        "30013",
        CollectionStatus::InProgress,
        Some(80),
        None,
        Some(12),
        Some(3),
        vec![
            SyncField::Status,
            SyncField::Score,
            SyncField::ProgressChapters,
            SyncField::ProgressVolumes,
        ],
    );

    let request = build_provider_write_request(&action).expect("request should build");

    assert_eq!(request.provider, Provider::MyAnimeList);
    assert_eq!(request.method, ProviderWriteRequestMethod::Put);
    assert_eq!(request.auth, ProviderWriteAuth::RequiredBearer);
    assert_eq!(
        request.url,
        "https://api.myanimelist.net/v2/manga/30013/my_list_status"
    );
    assert_eq!(request.content_type, "application/x-www-form-urlencoded");
    assert_eq!(
        request.body,
        "status=reading&score=8&num_chapters_read=12&num_volumes_read=3"
    );
}

#[test]
fn bangumi_anime_episode_progress_is_not_written_through_collection_endpoint() {
    let action = planned_action(
        Provider::AniList,
        Provider::Bangumi,
        MediaKind::Anime,
        "253",
        CollectionStatus::InProgress,
        None,
        Some(12),
        None,
        None,
        vec![SyncField::ProgressEpisodes],
    );

    let error = build_provider_write_request(&action).expect_err("request should be rejected");

    assert_eq!(
        error,
        ProviderWriteRequestError::UnsupportedField {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            field: SyncField::ProgressEpisodes,
        }
    );
}

#[test]
fn bangumi_episode_progress_write_request_requires_resolved_episode_ids() {
    let request = build_bangumi_episode_progress_write_request("253", 2, &[1001, 1002])
        .expect("episode progress request should build");
    let body: Value = serde_json::from_str(&request.body).expect("body should be json");

    assert_eq!(request.provider, Provider::Bangumi);
    assert_eq!(request.method, ProviderWriteRequestMethod::Patch);
    assert_eq!(request.auth, ProviderWriteAuth::RequiredBearer);
    assert_eq!(
        request.url,
        "https://api.bgm.tv/v0/users/-/collections/253/episodes"
    );
    assert_eq!(request.content_type, "application/json");
    assert_eq!(body["episode_id"], serde_json::json!([1001, 1002]));
    assert_eq!(body["type"], 2);
}

#[test]
fn bangumi_episode_progress_write_request_rejects_empty_resolved_episode_ids() {
    let error = build_bangumi_episode_progress_write_request("253", 1, &[])
        .expect_err("empty episode ids should be rejected");

    assert_eq!(
        error,
        ProviderWriteRequestError::MissingFieldValue {
            provider: Provider::Bangumi,
            field: SyncField::ProgressEpisodes,
        }
    );
}

#[test]
fn bangumi_episode_progress_write_request_rejects_invalid_subject_id() {
    let error = build_bangumi_episode_progress_write_request("not-a-subject", 1, &[1001])
        .expect_err("non-numeric subject id should be rejected");

    assert_eq!(
        error,
        ProviderWriteRequestError::InvalidProviderEntryId {
            provider: Provider::Bangumi,
            value: "not-a-subject".to_owned(),
        }
    );
}

#[test]
fn bangumi_episode_progress_write_request_rejects_invalid_resolved_episode_ids() {
    let zero_error = build_bangumi_episode_progress_write_request("253", 1, &[0])
        .expect_err("zero episode id should be rejected");
    assert_eq!(
        zero_error,
        ProviderWriteRequestError::InvalidResolvedEpisodeId {
            provider: Provider::Bangumi,
            value: 0,
        }
    );

    let duplicate_error = build_bangumi_episode_progress_write_request("253", 2, &[1001, 1001])
        .expect_err("duplicate episode id should be rejected");
    assert_eq!(
        duplicate_error,
        ProviderWriteRequestError::DuplicateResolvedEpisodeId {
            provider: Provider::Bangumi,
            episode_id: 1001,
        }
    );
}

#[test]
fn bangumi_episode_progress_write_request_for_action_requires_count_match() {
    let action = planned_action(
        Provider::AniList,
        Provider::Bangumi,
        MediaKind::Anime,
        "253",
        CollectionStatus::InProgress,
        None,
        Some(2),
        None,
        None,
        vec![SyncField::ProgressEpisodes],
    );

    let request = build_bangumi_episode_progress_write_request_for_action(&action, &[1001, 1002])
        .expect("episode progress request should build");
    let body: Value = serde_json::from_str(&request.body).expect("body should be json");

    assert_eq!(body["episode_id"], serde_json::json!([1001, 1002]));
    assert_eq!(body["type"], 2);
}

#[test]
fn bangumi_episode_progress_write_request_for_action_rejects_count_mismatch() {
    let action = planned_action(
        Provider::AniList,
        Provider::Bangumi,
        MediaKind::Anime,
        "253",
        CollectionStatus::InProgress,
        None,
        Some(3),
        None,
        None,
        vec![SyncField::ProgressEpisodes],
    );

    let error = build_bangumi_episode_progress_write_request_for_action(&action, &[1001, 1002])
        .expect_err("mismatched episode id count should be rejected");

    assert_eq!(
        error,
        ProviderWriteRequestError::ResolvedEpisodeCountMismatch {
            provider: Provider::Bangumi,
            requested_progress: 3,
            resolved_episode_count: 2,
        }
    );
}

#[test]
fn bangumi_episode_progress_write_request_for_action_rejects_mixed_fields() {
    let action = planned_action(
        Provider::AniList,
        Provider::Bangumi,
        MediaKind::Anime,
        "253",
        CollectionStatus::InProgress,
        None,
        Some(2),
        None,
        None,
        vec![SyncField::Status, SyncField::ProgressEpisodes],
    );

    let error = build_bangumi_episode_progress_write_request_for_action(&action, &[1001, 1002])
        .expect_err("mixed field action should be rejected");

    assert_eq!(
        error,
        ProviderWriteRequestError::UnsupportedField {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            field: SyncField::Status,
        }
    );
}

#[test]
fn bangumi_episode_progress_write_request_for_action_rejects_add_entry() {
    let mut action = planned_action(
        Provider::AniList,
        Provider::Bangumi,
        MediaKind::Anime,
        "253",
        CollectionStatus::InProgress,
        None,
        Some(2),
        None,
        None,
        vec![SyncField::ProgressEpisodes],
    );
    action.kind = PlannedActionKind::AddEntry;

    let error = build_bangumi_episode_progress_write_request_for_action(&action, &[1001, 1002])
        .expect_err("add entry episode progress action should be rejected");

    assert_eq!(
        error,
        ProviderWriteRequestError::UnsupportedField {
            provider: Provider::Bangumi,
            media_kind: MediaKind::Anime,
            field: SyncField::ProgressEpisodes,
        }
    );
}

#[test]
fn authorize_write_request_adds_sensitive_bearer_header_without_mutating_request() {
    let action = planned_action(
        Provider::Bangumi,
        Provider::AniList,
        MediaKind::Anime,
        "1",
        CollectionStatus::Completed,
        Some(90),
        Some(26),
        None,
        None,
        vec![
            SyncField::Status,
            SyncField::Score,
            SyncField::ProgressEpisodes,
        ],
    );
    let request = build_provider_write_request(&action).expect("request should build");
    let token = ProviderAccessToken::new(Provider::AniList, "write-access-token-secret")
        .expect("token should be valid");

    let authorized =
        authorize_provider_write_request(&request, &token).expect("request should authorize");

    assert_eq!(authorized.request, request);
    assert!(authorized.headers.iter().any(|header| {
        header.name == "Authorization" && header.sensitive && header.value.ends_with("-secret")
    }));
    assert!(authorized.headers.iter().any(|header| {
        header.name == "Content-Type" && !header.sensitive && header.value == "application/json"
    }));
    let debug = format!("{authorized:?}");
    assert!(!debug.contains("write-access-token-secret"));
}

#[test]
fn authorize_write_request_rejects_wrong_provider_token() {
    let action = planned_action(
        Provider::Bangumi,
        Provider::AniList,
        MediaKind::Anime,
        "1",
        CollectionStatus::Completed,
        Some(90),
        None,
        None,
        None,
        vec![SyncField::Score],
    );
    let request = build_provider_write_request(&action).expect("request should build");
    let token = ProviderAccessToken::new(Provider::Bangumi, "bangumi-token-secret")
        .expect("token should be valid");

    let error = authorize_provider_write_request(&request, &token)
        .expect_err("wrong provider token should fail");

    let debug = format!("{error:?}");
    assert!(debug.contains("ProviderMismatch"));
    assert!(!debug.contains("bangumi-token-secret"));
}

#[test]
fn http_write_transport_sends_authorized_request_without_leaking_token() {
    let action = planned_action(
        Provider::Bangumi,
        Provider::AniList,
        MediaKind::Anime,
        "1",
        CollectionStatus::Completed,
        Some(90),
        None,
        None,
        None,
        vec![SyncField::Score],
    );
    let request = build_provider_write_request(&action).expect("request should build");
    let token = ProviderAccessToken::new(Provider::AniList, "write-access-token-secret")
        .expect("token should be valid");
    let authorized =
        authorize_provider_write_request(&request, &token).expect("request should authorize");
    let mut transport = ProviderHttpWriteTransport::new(RecordingWriteHttpClient::ok(
        201,
        r#"{"data":{"SaveMediaListEntry":{"id":1}}}"#,
    ));

    let response = transport
        .send_authorized_provider_write_request(authorized)
        .expect("authorized write should succeed");
    let client = transport.into_inner();

    assert!(response.contains("SaveMediaListEntry"));
    assert_eq!(client.requests.len(), 1);
    assert_eq!(client.requests[0].provider, Provider::AniList);
    assert_eq!(client.requests[0].method, ProviderWriteRequestMethod::Post);
    assert_eq!(client.requests[0].url, "https://graphql.anilist.co");
    assert!(client.requests[0]
        .headers
        .iter()
        .any(|header| header.name == "Authorization" && header.sensitive));
    let debug = format!("{:?}", client.requests[0]);
    assert!(debug.contains("<present>"));
    assert!(!debug.contains("write-access-token-secret"));
    assert!(!debug.contains("SaveMediaListEntry"));
}

#[test]
fn http_write_transport_sends_myanimelist_form_body_and_content_type() {
    let action = planned_action(
        Provider::Bangumi,
        Provider::MyAnimeList,
        MediaKind::Manga,
        "30013",
        CollectionStatus::InProgress,
        Some(80),
        None,
        Some(12),
        Some(3),
        vec![
            SyncField::Status,
            SyncField::Score,
            SyncField::ProgressChapters,
            SyncField::ProgressVolumes,
        ],
    );
    let request = build_provider_write_request(&action).expect("request should build");
    let token = ProviderAccessToken::new(Provider::MyAnimeList, "mal-write-token-secret")
        .expect("token should be valid");
    let authorized =
        authorize_provider_write_request(&request, &token).expect("request should authorize");
    let mut transport = ProviderHttpWriteTransport::new(RecordingWriteHttpClient::ok(204, ""));

    let response = transport
        .send_authorized_provider_write_request(authorized)
        .expect("authorized write should succeed");
    let client = transport.into_inner();

    assert_eq!(response, "");
    assert_eq!(client.requests.len(), 1);
    assert_eq!(client.requests[0].provider, Provider::MyAnimeList);
    assert_eq!(client.requests[0].method, ProviderWriteRequestMethod::Put);
    assert_eq!(
        client.requests[0].url,
        "https://api.myanimelist.net/v2/manga/30013/my_list_status"
    );
    assert_eq!(
        client.requests[0].body,
        "status=reading&score=8&num_chapters_read=12&num_volumes_read=3"
    );
    assert!(client.requests[0].headers.iter().any(|header| {
        header.name == "Content-Type"
            && header.value == "application/x-www-form-urlencoded"
            && !header.sensitive
    }));
    let debug = format!("{:?}", client.requests[0]);
    assert!(debug.contains("<present>"));
    assert!(!debug.contains("mal-write-token-secret"));
    assert!(!debug.contains("num_chapters_read"));
}

#[test]
fn http_write_transport_sanitizes_client_and_status_errors() {
    let action = planned_action(
        Provider::Bangumi,
        Provider::AniList,
        MediaKind::Anime,
        "1",
        CollectionStatus::Completed,
        Some(90),
        None,
        None,
        None,
        vec![SyncField::Score],
    );
    let request = build_provider_write_request(&action).expect("request should build");
    let token = ProviderAccessToken::new(Provider::AniList, "write-access-token-secret")
        .expect("token should be valid");
    let authorized =
        authorize_provider_write_request(&request, &token).expect("request should authorize");
    let mut status_transport = ProviderHttpWriteTransport::new(RecordingWriteHttpClient::ok(
        500,
        "body with write-access-token-secret",
    ));

    let status_error = status_transport
        .send_authorized_provider_write_request(authorized.clone())
        .expect_err("500 should fail");
    assert_eq!(status_error, "provider=anilist HTTP 500");
    assert!(!status_error.contains("write-access-token-secret"));

    let mut client_error_transport = ProviderHttpWriteTransport::new(
        RecordingWriteHttpClient::fail("client error with write-access-token-secret"),
    );
    let client_error = client_error_transport
        .send_authorized_provider_write_request(authorized)
        .expect_err("client error should fail");
    assert_eq!(
        client_error,
        "HTTP write transport failed for provider=anilist"
    );
    assert!(!client_error.contains("write-access-token-secret"));
}

struct RecordingWriteHttpClient {
    requests: Vec<ProviderHttpWriteRequest>,
    response: Result<ProviderHttpWriteResponse, String>,
}

impl RecordingWriteHttpClient {
    fn ok(status: u16, body: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Ok(ProviderHttpWriteResponse {
                status,
                body: body.to_owned(),
            }),
        }
    }

    fn fail(message: &str) -> Self {
        Self {
            requests: Vec::new(),
            response: Err(message.to_owned()),
        }
    }
}

impl ProviderWriteHttpClient for RecordingWriteHttpClient {
    fn send_provider_write_http_request(
        &mut self,
        request: ProviderHttpWriteRequest,
    ) -> Result<ProviderHttpWriteResponse, String> {
        self.requests.push(request);
        self.response.clone()
    }
}

fn planned_action(
    source_provider: Provider,
    target_provider: Provider,
    media_kind: MediaKind,
    target_provider_entry_id: &str,
    status: CollectionStatus,
    score_hundred: Option<u8>,
    progress_episodes: Option<u32>,
    progress_chapters: Option<u32>,
    progress_volumes: Option<u32>,
    field_updates: Vec<SyncField>,
) -> PlannedAction {
    let field_changes = field_updates
        .iter()
        .map(|field| PlannedFieldChange {
            field: *field,
            old_value: None,
            new_value: "test".to_owned(),
        })
        .collect();

    PlannedAction {
        kind: PlannedActionKind::UpdateEntry,
        work_id: 1,
        source_provider,
        target_provider,
        target_provider_entry_id: target_provider_entry_id.to_owned(),
        media_kind,
        status,
        score_hundred,
        progress_episodes,
        progress_chapters,
        progress_volumes,
        field_updates,
        field_changes,
        reason: "test request builder".to_owned(),
    }
}
