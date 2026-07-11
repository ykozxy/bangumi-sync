use serde_json::Value;
use sync_core::model::{CollectionStatus, MediaKind, Provider};
use sync_core::provider::{
    build_bangumi_episode_collection_read_request, build_collection_read_request,
    fetch_authorized_bangumi_episode_collection_pages_with_transport,
    fetch_authorized_collection_snapshot_pages_with_transport,
    fetch_authorized_collection_snapshot_with_transport,
    fetch_collection_snapshot_pages_with_transport, fetch_collection_snapshot_with_transport,
    AuthorizedProviderReadRequest, AuthorizedProviderReadTransport, ProviderAccessToken,
    ProviderHttpClient, ProviderHttpReadTransport, ProviderHttpRequest, ProviderHttpResponse,
    ProviderReadAuth, ProviderReadRequest, ProviderReadRequestError, ProviderReadRequestMethod,
    ProviderReadTransport,
};

#[test]
fn bangumi_read_request_targets_user_collection_by_subject_type() {
    let request =
        build_collection_read_request(Provider::Bangumi, MediaKind::Anime, "alice", 50, 100)
            .expect("request should build");

    assert_eq!(request.method, ProviderReadRequestMethod::Get);
    assert_eq!(request.provider, Provider::Bangumi);
    assert_eq!(request.auth, ProviderReadAuth::RequiredBearer);
    assert_eq!(
        request.url,
        "https://api.bgm.tv/v0/users/alice/collections?subject_type=2&limit=50&offset=100"
    );
    assert_eq!(request.content_type, None);
    assert_eq!(request.body, None);
}

#[test]
fn bangumi_episode_collection_read_request_targets_main_episodes_for_subject() {
    let request = build_bangumi_episode_collection_read_request("253", 100, 200)
        .expect("request should build");

    assert_eq!(request.method, ProviderReadRequestMethod::Get);
    assert_eq!(request.provider, Provider::Bangumi);
    assert_eq!(request.auth, ProviderReadAuth::RequiredBearer);
    assert_eq!(
        request.url,
        "https://api.bgm.tv/v0/users/-/collections/253/episodes?episode_type=0&limit=100&offset=200"
    );
    assert_eq!(request.content_type, None);
    assert_eq!(request.body, None);
}

#[test]
fn anilist_read_request_uses_media_list_collection_query() {
    let request =
        build_collection_read_request(Provider::AniList, MediaKind::Manga, "alice", 500, 0)
            .expect("request should build");
    let body: Value = serde_json::from_str(request.body.as_deref().expect("json body"))
        .expect("body should be json");

    assert_eq!(request.method, ProviderReadRequestMethod::Post);
    assert_eq!(request.provider, Provider::AniList);
    assert_eq!(request.auth, ProviderReadAuth::RequiredBearer);
    assert_eq!(request.url, "https://graphql.anilist.co");
    assert_eq!(request.content_type, Some("application/json"));
    assert!(body["query"]
        .as_str()
        .expect("query string")
        .contains("MediaListCollection"));
    assert_eq!(body["variables"]["userName"], "alice");
    assert_eq!(body["variables"]["type"], "MANGA");
}

#[test]
fn myanimelist_read_request_selects_list_endpoint_and_fields() {
    let request =
        build_collection_read_request(Provider::MyAnimeList, MediaKind::Manga, "@me", 1000, 2000)
            .expect("request should build");

    assert_eq!(request.method, ProviderReadRequestMethod::Get);
    assert_eq!(request.provider, Provider::MyAnimeList);
    assert_eq!(request.auth, ProviderReadAuth::RequiredBearer);
    assert!(request
        .url
        .starts_with("https://api.myanimelist.net/v2/users/%40me/mangalist?"));
    assert!(request.url.contains("limit=1000"));
    assert!(request.url.contains("offset=2000"));
    assert!(request.url.contains("fields="));
    assert!(request.url.contains("media_type"));
    assert!(request.url.contains("list_status%7B"));
    assert_eq!(request.content_type, None);
    assert_eq!(request.body, None);
}

#[test]
fn read_requests_do_not_embed_access_tokens() {
    let request =
        build_collection_read_request(Provider::AniList, MediaKind::Anime, "alice", 500, 0)
            .expect("request should build");

    assert_eq!(request.auth, ProviderReadAuth::RequiredBearer);
    assert!(!request.url.contains("token"));
    assert!(!request
        .body
        .as_deref()
        .unwrap_or_default()
        .contains("token"));
}

#[test]
fn read_request_rejects_empty_account_and_zero_limit() {
    assert_eq!(
        build_collection_read_request(Provider::Bangumi, MediaKind::Anime, " ", 50, 0),
        Err(ProviderReadRequestError::MissingAccountId {
            provider: Provider::Bangumi,
        })
    );
    assert_eq!(
        build_collection_read_request(Provider::AniList, MediaKind::Anime, "alice", 0, 0),
        Err(ProviderReadRequestError::InvalidPageLimit {
            provider: Provider::AniList,
            limit: 0,
        })
    );
}

#[test]
fn fetch_collection_snapshot_uses_transport_and_normalizes_response() {
    let mut transport = RecordingReadTransport::ok(
        r#"{
        "data": [
            {
                "node": { "id": 5114, "media_type": "anime" },
                "list_status": {
                    "status": "completed",
                    "score": 10,
                    "num_episodes_watched": 64,
                    "updated_at": "2026-06-01T00:00:00Z"
                }
            }
        ]
    }"#,
    );

    let snapshot = fetch_collection_snapshot_with_transport(
        &mut transport,
        Provider::MyAnimeList,
        MediaKind::Anime,
        "@me",
        1000,
        0,
    )
    .expect("snapshot should parse");

    assert_eq!(transport.requests.len(), 1);
    assert_eq!(transport.requests[0].method, ProviderReadRequestMethod::Get);
    assert_eq!(snapshot.provider(), Provider::MyAnimeList);
    assert_eq!(snapshot.media_kind(), MediaKind::Anime);
    assert_eq!(snapshot.entries().len(), 1);
    let entry = &snapshot.entries()[0];
    assert_eq!(entry.provider_entry_id(), "5114");
    assert_eq!(entry.status(), CollectionStatus::Completed);
    assert_eq!(entry.progress().episodes(), Some(64));
}

#[test]
fn fetch_authorized_collection_snapshot_sends_sensitive_headers_to_transport() {
    let mut transport = RecordingAuthorizedReadTransport::ok(
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME", "idMal": 5114 },
                                    "status": "CURRENT",
                                    "score": 70,
                                    "progress": 12,
                                    "progressVolumes": null,
                                    "updatedAt": 1764560000
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
    );
    let token = ProviderAccessToken::new(Provider::AniList, "valid-access-token-secret")
        .expect("token should be valid");

    let snapshot = fetch_authorized_collection_snapshot_with_transport(
        &mut transport,
        &token,
        Provider::AniList,
        MediaKind::Anime,
        "alice",
        500,
        0,
    )
    .expect("authorized snapshot should parse");

    assert_eq!(transport.requests.len(), 1);
    let authorized = &transport.requests[0];
    assert_eq!(authorized.request.provider, Provider::AniList);
    assert_eq!(authorized.request.method, ProviderReadRequestMethod::Post);
    assert_eq!(authorized.request.content_type, Some("application/json"));
    assert!(!authorized.request.url.contains("valid-access-token-secret"));
    assert!(!authorized
        .request
        .body
        .as_deref()
        .unwrap_or_default()
        .contains("valid-access-token-secret"));
    assert!(authorized.headers.iter().any(|header| {
        header.name == "Authorization"
            && header.value == "Bearer valid-access-token-secret"
            && header.sensitive
    }));
    assert!(authorized.headers.iter().any(|header| {
        header.name == "Content-Type" && header.value == "application/json" && !header.sensitive
    }));

    assert_eq!(snapshot.provider(), Provider::AniList);
    assert_eq!(snapshot.media_kind(), MediaKind::Anime);
    assert_eq!(snapshot.entries().len(), 1);
    assert_eq!(snapshot.entries()[0].provider_entry_id(), "1");
    assert_eq!(snapshot.entries()[0].status(), CollectionStatus::InProgress);
    assert_eq!(snapshot.entries()[0].progress().episodes(), Some(12));
    assert!(!format!("{authorized:?}").contains("valid-access-token-secret"));
}

#[test]
fn fetch_paginated_collection_snapshot_reads_until_short_page() {
    let mut transport = RecordingReadTransport::sequence(vec![
        r#"{
            "data": [
                {
                    "subject_id": 253,
                    "subject_type": "anime",
                    "collection_type": "collect",
                    "rate": 10,
                    "ep_status": 26,
                    "vol_status": 0
                },
                {
                    "subject_id": 998,
                    "subject_type": "anime",
                    "collection_type": "do",
                    "rate": 8,
                    "ep_status": 3,
                    "vol_status": 0
                }
            ]
        }"#,
        r#"{
            "data": [
                {
                    "subject_id": 999,
                    "subject_type": "anime",
                    "collection_type": "wish",
                    "rate": 0,
                    "ep_status": 0,
                    "vol_status": 0
                }
            ]
        }"#,
    ]);

    let snapshot = fetch_collection_snapshot_pages_with_transport(
        &mut transport,
        Provider::Bangumi,
        MediaKind::Anime,
        "alice",
        2,
    )
    .expect("paginated snapshot should parse");

    assert_eq!(transport.requests.len(), 2);
    assert!(transport.requests[0].url.contains("offset=0"));
    assert!(transport.requests[1].url.contains("offset=2"));
    assert_eq!(snapshot.provider(), Provider::Bangumi);
    assert_eq!(snapshot.media_kind(), MediaKind::Anime);
    assert_eq!(snapshot.entries().len(), 3);
    assert_eq!(snapshot.entries()[2].provider_entry_id(), "999");
}

#[test]
fn fetch_authorized_bangumi_episode_collection_pages_collects_main_episode_ids() {
    let mut transport = RecordingAuthorizedReadTransport::sequence(vec![
        r#"{
            "data": [
                {
                    "episode": { "id": 1000, "type": 0, "sort": 0, "ep": 0 },
                    "type": 1,
                    "updated_at": 1699999999
                },
                {
                    "episode": { "id": 1002, "type": 0, "sort": 2, "ep": 2 },
                    "type": 2,
                    "updated_at": 1700000000
                }
            ]
        }"#,
        r#"{
            "data": [
                {
                    "episode": { "id": 1001, "type": 0, "sort": 1, "ep": 1 },
                    "type": 2,
                    "updated_at": 1700000001
                }
            ]
        }"#,
    ]);
    let token = ProviderAccessToken::new(Provider::Bangumi, "valid-access-token-secret")
        .expect("token should be valid");

    let snapshot = fetch_authorized_bangumi_episode_collection_pages_with_transport(
        &mut transport,
        &token,
        "253",
        2,
    )
    .expect("episode collection should parse");

    assert_eq!(transport.requests.len(), 2);
    assert!(transport.requests[0]
        .request
        .url
        .contains("episode_type=0&limit=2&offset=0"));
    assert!(transport.requests[1]
        .request
        .url
        .contains("episode_type=0&limit=2&offset=2"));
    assert!(transport.requests.iter().all(|authorized| {
        authorized.headers.iter().any(|header| {
            header.name == "Authorization"
                && header.value == "Bearer valid-access-token-secret"
                && header.sensitive
        })
    }));

    assert_eq!(snapshot.subject_id(), "253");
    assert_eq!(snapshot.episode_ids_for_done_prefix(2), vec![1001, 1002]);
    assert_eq!(snapshot.entries().len(), 3);
    assert_eq!(snapshot.entries()[0].episode_id(), 1000);
    assert_eq!(snapshot.entries()[0].sort(), 0.0);
    assert_eq!(snapshot.entries()[0].collection_type(), 1);
    assert!(!format!("{:?}", transport.requests[0]).contains("valid-access-token-secret"));
}

#[test]
fn fetch_authorized_paginated_collection_snapshot_reads_until_short_page() {
    let mut transport = RecordingAuthorizedReadTransport::sequence(vec![
        r#"{
            "data": [
                {
                    "subject_id": 253,
                    "subject_type": "anime",
                    "collection_type": "collect",
                    "rate": 10,
                    "ep_status": 26,
                    "vol_status": 0
                },
                {
                    "subject_id": 998,
                    "subject_type": "anime",
                    "collection_type": "do",
                    "rate": 8,
                    "ep_status": 3,
                    "vol_status": 0
                }
            ]
        }"#,
        r#"{
            "data": [
                {
                    "subject_id": 999,
                    "subject_type": "anime",
                    "collection_type": "wish",
                    "rate": 0,
                    "ep_status": 0,
                    "vol_status": 0
                }
            ]
        }"#,
    ]);
    let token = ProviderAccessToken::new(Provider::Bangumi, "valid-access-token-secret")
        .expect("token should be valid");

    let snapshot = fetch_authorized_collection_snapshot_pages_with_transport(
        &mut transport,
        &token,
        Provider::Bangumi,
        MediaKind::Anime,
        "alice",
        2,
    )
    .expect("paginated authorized snapshot should parse");

    assert_eq!(transport.requests.len(), 2);
    assert!(transport.requests[0].request.url.contains("offset=0"));
    assert!(transport.requests[1].request.url.contains("offset=2"));
    for authorized in &transport.requests {
        assert!(!authorized.request.url.contains("valid-access-token-secret"));
        assert!(!authorized
            .request
            .body
            .as_deref()
            .unwrap_or_default()
            .contains("valid-access-token-secret"));
        assert!(authorized.headers.iter().any(|header| {
            header.name == "Authorization"
                && header.value == "Bearer valid-access-token-secret"
                && header.sensitive
        }));
        assert!(!format!("{authorized:?}").contains("valid-access-token-secret"));
    }
    assert_eq!(snapshot.provider(), Provider::Bangumi);
    assert_eq!(snapshot.media_kind(), MediaKind::Anime);
    assert_eq!(snapshot.entries().len(), 3);
    assert_eq!(snapshot.entries()[2].provider_entry_id(), "999");
}

#[test]
fn fetch_authorized_paginated_collection_snapshot_uses_single_request_for_anilist() {
    let mut transport = RecordingAuthorizedReadTransport::sequence(vec![
        r#"{
            "data": {
                "MediaListCollection": {
                    "lists": [
                        {
                            "entries": [
                                {
                                    "mediaId": 1,
                                    "media": { "type": "ANIME", "idMal": 5114 },
                                    "status": "CURRENT",
                                    "score": 70,
                                    "progress": 12,
                                    "progressVolumes": null,
                                    "updatedAt": 1764560000
                                },
                                {
                                    "mediaId": 2,
                                    "media": { "type": "ANIME", "idMal": 32281 },
                                    "status": "COMPLETED",
                                    "score": 90,
                                    "progress": 24,
                                    "progressVolumes": null,
                                    "updatedAt": 1764560500
                                }
                            ]
                        }
                    ]
                }
            }
        }"#,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[]}]}}}"#,
    ]);
    let token = ProviderAccessToken::new(Provider::AniList, "valid-access-token-secret")
        .expect("token should be valid");

    let snapshot = fetch_authorized_collection_snapshot_pages_with_transport(
        &mut transport,
        &token,
        Provider::AniList,
        MediaKind::Anime,
        "alice",
        2,
    )
    .expect("anilist authorized snapshot should parse");

    assert_eq!(transport.requests.len(), 1);
    assert_eq!(transport.requests[0].request.provider, Provider::AniList);
    assert_eq!(
        transport.requests[0].request.method,
        ProviderReadRequestMethod::Post
    );
    assert!(transport.requests[0].headers.iter().any(|header| {
        header.name == "Authorization"
            && header.value == "Bearer valid-access-token-secret"
            && header.sensitive
    }));
    assert_eq!(snapshot.provider(), Provider::AniList);
    assert_eq!(snapshot.entries().len(), 2);
}

#[test]
fn http_read_transport_sends_authorized_provider_request_without_leaking_token_in_debug() {
    let client = RecordingHttpClient::ok(
        200,
        r#"{"data":{"MediaListCollection":{"lists":[{"entries":[]}]}}}"#,
    );
    let mut transport = ProviderHttpReadTransport::new(client);
    let token = ProviderAccessToken::new(Provider::AniList, "valid-access-token-secret")
        .expect("token should be valid");

    let snapshot = fetch_authorized_collection_snapshot_with_transport(
        &mut transport,
        &token,
        Provider::AniList,
        MediaKind::Anime,
        "alice",
        500,
        0,
    )
    .expect("http-backed authorized snapshot should parse");

    assert_eq!(snapshot.provider(), Provider::AniList);
    let client = transport.into_inner();
    assert_eq!(client.requests.len(), 1);
    let request = &client.requests[0];
    assert_eq!(request.method, ProviderReadRequestMethod::Post);
    assert_eq!(request.url, "https://graphql.anilist.co");
    assert!(request
        .body
        .as_deref()
        .unwrap_or_default()
        .contains("alice"));
    assert!(request.headers.iter().any(|header| {
        header.name == "Authorization"
            && header.value == "Bearer valid-access-token-secret"
            && header.sensitive
    }));
    assert!(request.headers.iter().any(|header| {
        header.name == "Content-Type" && header.value == "application/json" && !header.sensitive
    }));
    assert!(!format!("{request:?}").contains("valid-access-token-secret"));
}

#[test]
fn http_read_transport_sends_authorized_get_without_body_or_content_type() {
    let client = RecordingHttpClient::ok(
        200,
        r#"{"data":[{"subject_id":253,"subject_type":"anime","collection_type":"collect","rate":10,"ep_status":26,"vol_status":0}]}"#,
    );
    let mut transport = ProviderHttpReadTransport::new(client);
    let token = ProviderAccessToken::new(Provider::Bangumi, "valid-access-token-secret")
        .expect("token should be valid");

    let snapshot = fetch_authorized_collection_snapshot_with_transport(
        &mut transport,
        &token,
        Provider::Bangumi,
        MediaKind::Anime,
        "alice",
        50,
        0,
    )
    .expect("http-backed authorized snapshot should parse");

    assert_eq!(snapshot.entries().len(), 1);
    let client = transport.into_inner();
    let request = &client.requests[0];
    assert_eq!(request.method, ProviderReadRequestMethod::Get);
    assert!(request.url.contains("/users/alice/collections"));
    assert_eq!(request.body, None);
    assert!(request
        .headers
        .iter()
        .all(|header| header.name != "Content-Type"));
    assert!(request.headers.iter().any(|header| {
        header.name == "Authorization"
            && header.value == "Bearer valid-access-token-secret"
            && header.sensitive
    }));
}

#[test]
fn http_read_transport_reports_non_success_status_without_response_body_or_token() {
    let client = RecordingHttpClient::ok(401, "unauthorized valid-access-token-secret");
    let mut transport = ProviderHttpReadTransport::new(client);
    let token = ProviderAccessToken::new(Provider::Bangumi, "valid-access-token-secret")
        .expect("token should be valid");

    let error = fetch_authorized_collection_snapshot_with_transport(
        &mut transport,
        &token,
        Provider::Bangumi,
        MediaKind::Anime,
        "alice",
        50,
        0,
    )
    .expect_err("non-success HTTP status should fail");

    let error = format!("{error:?}");
    assert!(error.contains("HTTP 401"));
    assert!(!error.contains("unauthorized valid-access-token-secret"));
    assert!(!error.contains("valid-access-token-secret"));
}

#[test]
fn http_read_transport_sanitizes_injected_client_errors() {
    let client = RecordingHttpClient::err("socket failed valid-access-token-secret");
    let mut transport = ProviderHttpReadTransport::new(client);
    let token = ProviderAccessToken::new(Provider::MyAnimeList, "valid-access-token-secret")
        .expect("token should be valid");

    let error = fetch_authorized_collection_snapshot_with_transport(
        &mut transport,
        &token,
        Provider::MyAnimeList,
        MediaKind::Anime,
        "@me",
        50,
        0,
    )
    .expect_err("client transport error should fail");

    let error = format!("{error:?}");
    assert!(error.contains("HTTP transport failed"));
    assert!(error.contains("myanimelist"));
    assert!(!error.contains("socket failed valid-access-token-secret"));
    assert!(!error.contains("valid-access-token-secret"));
}

struct RecordingReadTransport {
    requests: Vec<ProviderReadRequest>,
    responses: Vec<Result<String, String>>,
}

struct RecordingHttpClient {
    requests: Vec<ProviderHttpRequest>,
    responses: Vec<Result<ProviderHttpResponse, String>>,
}

struct RecordingAuthorizedReadTransport {
    requests: Vec<AuthorizedProviderReadRequest>,
    responses: Vec<Result<String, String>>,
}

impl RecordingHttpClient {
    fn ok(status: u16, body: &str) -> Self {
        Self {
            requests: Vec::new(),
            responses: vec![Ok(ProviderHttpResponse {
                status,
                body: body.to_owned(),
            })],
        }
    }

    fn err(message: &str) -> Self {
        Self {
            requests: Vec::new(),
            responses: vec![Err(message.to_owned())],
        }
    }
}

impl ProviderHttpClient for RecordingHttpClient {
    fn send_provider_http_request(
        &mut self,
        request: ProviderHttpRequest,
    ) -> Result<ProviderHttpResponse, String> {
        self.requests.push(request);
        self.responses
            .pop()
            .unwrap_or_else(|| Err("missing fake HTTP response".to_owned()))
    }
}

impl RecordingAuthorizedReadTransport {
    fn ok(response_body: &str) -> Self {
        Self {
            requests: Vec::new(),
            responses: vec![Ok(response_body.to_owned())],
        }
    }

    fn sequence(response_bodies: Vec<&str>) -> Self {
        Self {
            requests: Vec::new(),
            responses: response_bodies
                .into_iter()
                .map(|body| Ok(body.to_owned()))
                .collect(),
        }
    }
}

impl AuthorizedProviderReadTransport for RecordingAuthorizedReadTransport {
    fn send_authorized_provider_read_request(
        &mut self,
        request: AuthorizedProviderReadRequest,
    ) -> Result<String, String> {
        self.requests.push(request);
        if self.responses.len() > 1 {
            self.responses.remove(0)
        } else {
            self.responses
                .last()
                .cloned()
                .unwrap_or_else(|| Err("missing fake response".to_owned()))
        }
    }
}

impl RecordingReadTransport {
    fn ok(response_body: &str) -> Self {
        Self {
            requests: Vec::new(),
            responses: vec![Ok(response_body.to_owned())],
        }
    }

    fn sequence(response_bodies: Vec<&str>) -> Self {
        Self {
            requests: Vec::new(),
            responses: response_bodies
                .into_iter()
                .map(|body| Ok(body.to_owned()))
                .collect(),
        }
    }
}

impl ProviderReadTransport for RecordingReadTransport {
    fn send_provider_read_request(
        &mut self,
        request: ProviderReadRequest,
    ) -> Result<String, String> {
        self.requests.push(request);
        if self.responses.len() > 1 {
            self.responses.remove(0)
        } else {
            self.responses
                .last()
                .cloned()
                .unwrap_or_else(|| Err("missing fake response".to_owned()))
        }
    }
}
