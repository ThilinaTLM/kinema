use super::*;
use serde_json::json;
use std::result::Result;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
    delay: Duration,
}
fn reply(status: u16, body: Value) -> Reply {
    Reply {
        status,
        headers: vec![("Content-Type", "application/json".into())],
        body: body.to_string().into_bytes(),
        delay: Duration::ZERO,
    }
}
fn raw(status: u16, body: Vec<u8>) -> Reply {
    Reply {
        status,
        headers: Vec::new(),
        body,
        delay: Duration::ZERO,
    }
}

/// Serves one reply per connection and returns the raw requests it saw.
fn serve(replies: Vec<Reply>) -> Result<(Url, thread::JoinHandle<Vec<String>>), std::io::Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let url = Url::parse(&format!("http://{}/api/v1/", listener.local_addr()?))
        .map_err(std::io::Error::other)?;
    listener.set_nonblocking(true)?;
    let handle = thread::spawn(move || {
        let mut seen = Vec::new();
        for reply in replies {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                    Err(_) => return seen,
                }
            };
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut data = Vec::new();
            let mut buffer = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&data).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if data.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            seen.push(String::from_utf8_lossy(&data).to_string());
            thread::sleep(reply.delay);
            let mut head = format!(
                "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                reply.status,
                reply.body.len()
            );
            for (k, v) in &reply.headers {
                head.push_str(&format!("{k}: {v}\r\n"));
            }
            head.push_str("\r\n");
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&reply.body);
        }
        seen
    });
    Ok((url, handle))
}

fn client(api: Url) -> Result<OpenSubtitlesClient, OpenSubtitlesError> {
    OpenSubtitlesClient::for_test(api.clone(), api)
}
fn idle() -> AtomicBool {
    AtomicBool::new(false)
}
fn movie_query() -> SubtitleQuery {
    SubtitleQuery {
        target: Some(SubtitleTarget::Movie { imdb: 133093 }),
        languages: vec!["fr".into(), "en".into()],
        ..SubtitleQuery::default()
    }
}
fn session(host: &str) -> Session {
    Session {
        token: Zeroizing::new("tok-123".into()),
        base_host: host.into(),
        allowed_downloads: Some(10),
        remaining_downloads: Some(9),
    }
}
fn item(language: &str, file_id: u64, extra: Value) -> Value {
    let mut attrs = json!({
        "language": language, "release": "Rel.1080p", "download_count": 5,
        "files": [{"file_id": file_id, "file_name": "Movie.en.SRT"}]
    });
    if let (Some(a), Some(e)) = (attrs.as_object_mut(), extra.as_object()) {
        a.extend(e.clone());
    }
    json!({"id": "1", "type": "subtitle", "attributes": attrs})
}
fn request_line(request: &str) -> &str {
    request.lines().next().unwrap_or_default()
}
fn header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.eq_ignore_ascii_case(name).then_some(v.trim())
    })
}

// ---- pure ----

#[test]
fn canonical_movie_query_is_sorted_lowercase_without_defaults() -> TestResult {
    let query = SubtitleQuery {
        target: Some(SubtitleTarget::Movie { imdb: 133093 }),
        text: Some("  The  MATRIX Ré ".into()),
        languages: vec!["FR".into(), "en".into(), "en".into(), "x1".into()],
        moviehash: Some(0xAB),
    };
    assert_eq!(
        canonical_query(&query)?,
        "imdb_id=133093&languages=en,fr&moviehash=00000000000000ab&query=the+matrix+r%C3%A9"
    );
    Ok(())
}

#[test]
fn canonical_episode_query_uses_parent_fields() -> TestResult {
    let query = SubtitleQuery {
        target: Some(SubtitleTarget::Episode {
            series_imdb: 903747,
            season: 2,
            episode: 5,
        }),
        languages: vec!["en".into()],
        ..SubtitleQuery::default()
    };
    assert_eq!(
        canonical_query(&query)?,
        "episode_number=5&languages=en&parent_imdb_id=903747&season_number=2"
    );
    Ok(())
}

#[test]
fn query_without_any_criteria_is_a_configuration_error() {
    assert_eq!(
        canonical_query(&SubtitleQuery::default()),
        Err(OpenSubtitlesError::Configuration)
    );
}

#[test]
fn status_mapping_is_distinct() {
    use OpenSubtitlesError as E;
    let m =
        |s: u16, e, session| map_status(StatusCode::from_u16(s).unwrap_or_default(), e, session);
    assert_eq!(m(401, Endpoint::Login, false), E::LoginRejected);
    assert_eq!(m(401, Endpoint::Download, false), E::SignInRequired);
    assert_eq!(m(401, Endpoint::Download, true), E::SessionExpired);
    assert_eq!(m(401, Endpoint::Search, false), E::ApiKeyRejected);
    assert_eq!(m(403, Endpoint::Search, true), E::ApiKeyRejected);
    assert_eq!(m(403, Endpoint::Login, false), E::ApiKeyRejected);
    assert_eq!(m(429, Endpoint::Search, false), E::RateLimited);
    assert_eq!(m(500, Endpoint::Search, false), E::ServiceFailure);
    assert_eq!(m(302, Endpoint::Search, false), E::ServiceFailure);
    assert!(matches!(
        m(406, Endpoint::Download, true),
        E::QuotaExhausted { .. }
    ));
}

#[test]
fn api_hosts_are_allowlisted() {
    assert_eq!(
        allowed_api_host("vip-api.opensubtitles.com").as_deref(),
        Some(VIP_HOST)
    );
    assert_eq!(
        allowed_api_host("https://api.opensubtitles.com/").as_deref(),
        Some(API_HOST)
    );
    assert_eq!(allowed_api_host("evil.example"), None);
    assert_eq!(allowed_api_host("api.opensubtitles.com.evil.example"), None);
}

#[test]
fn production_link_validation_is_strict() -> TestResult {
    let mut c = client(Url::parse("http://127.0.0.1:1/api/v1/")?)?;
    c.loopback_links = false;
    assert!(
        c.validate_link("https://www.opensubtitles.com/download/x")
            .is_ok()
    );
    for bad in [
        "http://www.opensubtitles.com/x",
        "https://evil.example/x",
        "https://api.opensubtitles.com/x",
        "https://user@www.opensubtitles.com/x",
        "http://127.0.0.1:1/x",
        "not a url",
    ] {
        assert_eq!(
            c.validate_link(bad).err(),
            Some(OpenSubtitlesError::InvalidResponse),
            "{bad}"
        );
    }
    Ok(())
}

#[test]
fn debug_output_hides_token() {
    let text = format!("{:?}", session(API_HOST));
    assert!(!text.contains("tok-123"));
}

// ---- parsing ----

#[test]
fn parse_maps_files_formats_and_flags() -> TestResult {
    let body = json!({"data": [
        item("EN", 7, json!({"ai_translated": true, "from_trusted": true, "moviehash_match": true,
             "hearing_impaired": true, "foreign_parts_only": true, "machine_translated": false})),
        {"attributes": {"language": "fr", "format": "SRT", "files": [
            {"file_id": 8}, {"file_name": "no id"}, {"file_id": 9, "file_name": "noext"}]}},
    ]});
    let results = parse_results(&body)?;
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].file_id, 7);
    assert_eq!(results[0].language, "en");
    assert_eq!(results[0].format.as_deref(), Some("srt"));
    assert_eq!(results[0].file_name.as_deref(), Some("Movie.en.SRT"));
    assert!(results[0].ai_translated && results[0].from_trusted && results[0].moviehash_match);
    assert!(results[0].hearing_impaired && results[0].foreign_parts_only);
    assert_eq!(
        (results[1].file_id, results[1].format.as_deref()),
        (8, Some("srt"))
    );
    assert_eq!(results[2].format.as_deref(), Some("srt"));
    Ok(())
}

#[test]
fn parse_skips_malformed_items_but_rejects_all_malformed() -> TestResult {
    let mixed = json!({"data": [{"attributes": {"language": ""}}, item("en", 1, json!({})), 5]});
    assert_eq!(parse_results(&mixed)?.len(), 1);
    let all_bad = json!({"data": [{"attributes": {}}, {"id": 1}]});
    assert_eq!(
        parse_results(&all_bad),
        Err(OpenSubtitlesError::InvalidResponse)
    );
    assert_eq!(parse_results(&json!({"data": []})), Ok(vec![]));
    assert_eq!(
        parse_results(&json!({"nodata": []})),
        Err(OpenSubtitlesError::InvalidResponse)
    );
    Ok(())
}

// ---- loopback ----

#[test]
fn search_sends_canonical_request_and_headers() -> TestResult {
    let (url, server) = serve(vec![reply(
        200,
        json!({"data": [item("en", 1, json!({}))]}),
    )])?;
    let results = client(url)?.search(&movie_query(), Some(&session(API_HOST)), &idle())?;
    assert_eq!(results.len(), 1);
    let seen = server.join().map_err(|_| "server panicked")?;
    assert_eq!(
        request_line(&seen[0]),
        "GET /api/v1/subtitles?imdb_id=133093&languages=en,fr HTTP/1.1"
    );
    assert_eq!(header(&seen[0], "api-key"), Some("test-key"));
    assert_eq!(header(&seen[0], "user-agent"), Some("Kinema vtest"));
    assert_eq!(header(&seen[0], "accept"), Some("application/json"));
    assert_eq!(header(&seen[0], "authorization"), Some("Bearer tok-123"));
    Ok(())
}

#[test]
fn anonymous_search_sends_no_authorization() -> TestResult {
    let (url, server) = serve(vec![reply(200, json!({"data": []}))])?;
    client(url)?.search(&movie_query(), None, &idle())?;
    let seen = server.join().map_err(|_| "server panicked")?;
    assert_eq!(header(&seen[0], "authorization"), None);
    Ok(())
}

#[test]
fn search_ranks_by_preferred_language() -> TestResult {
    let body = json!({"data": [item("en", 1, json!({})), item("fr", 2, json!({})),
        item("es", 3, json!({"machine_translated": true}))]});
    let (url, _server) = serve(vec![reply(200, body)])?;
    let results = client(url)?.search(&movie_query(), None, &idle())?;
    assert_eq!(
        results.iter().map(|r| r.file_id).collect::<Vec<_>>(),
        vec![2, 1]
    );
    Ok(())
}

#[test]
fn empty_results_succeed_while_failures_do_not() -> TestResult {
    let (url, _s) = serve(vec![reply(200, json!({"data": []}))])?;
    assert_eq!(
        client(url)?.search(&movie_query(), None, &idle()),
        Ok(vec![])
    );
    let (url, _s) = serve(vec![reply(200, json!({"data": [{"x": 1}]}))])?;
    assert_eq!(
        client(url)?.search(&movie_query(), None, &idle()),
        Err(OpenSubtitlesError::InvalidResponse)
    );
    let (url, _s) = serve(vec![raw(200, b"not json".to_vec())])?;
    assert_eq!(
        client(url)?.search(&movie_query(), None, &idle()),
        Err(OpenSubtitlesError::InvalidResponse)
    );
    Ok(())
}

#[test]
fn search_error_statuses_map() -> TestResult {
    for (status, expected) in [
        (401, OpenSubtitlesError::ApiKeyRejected),
        (403, OpenSubtitlesError::ApiKeyRejected),
        (429, OpenSubtitlesError::RateLimited),
        (500, OpenSubtitlesError::ServiceFailure),
        (503, OpenSubtitlesError::ServiceFailure),
        (404, OpenSubtitlesError::ServiceFailure),
    ] {
        let (url, _s) = serve(vec![reply(status, json!({"message": "secret detail"}))])?;
        assert_eq!(
            client(url)?.search(&movie_query(), None, &idle()),
            Err(expected),
            "{status}"
        );
    }
    Ok(())
}

#[test]
fn redirect_is_a_service_failure_and_not_followed() -> TestResult {
    let mut redirect = raw(302, Vec::new());
    redirect
        .headers
        .push(("Location", "http://127.0.0.1:1/elsewhere".into()));
    let (url, server) = serve(vec![redirect])?;
    assert_eq!(
        client(url)?.search(&movie_query(), None, &idle()),
        Err(OpenSubtitlesError::ServiceFailure)
    );
    assert_eq!(server.join().map_err(|_| "panic")?.len(), 1);
    Ok(())
}

#[test]
fn oversize_json_is_rejected() -> TestResult {
    let (url, _s) = serve(vec![raw(200, vec![b' '; (MAX_JSON + 1) as usize])])?;
    assert_eq!(
        client(url)?.search(&movie_query(), None, &idle()),
        Err(OpenSubtitlesError::InvalidResponse)
    );
    Ok(())
}

#[test]
fn transport_failure_is_network() -> TestResult {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let url = Url::parse(&format!("http://{}/api/v1/", listener.local_addr()?))?;
    drop(listener);
    assert_eq!(
        client(url)?.search(&movie_query(), None, &idle()),
        Err(OpenSubtitlesError::Network)
    );
    Ok(())
}

#[test]
fn login_success_parses_session_and_vip_host() -> TestResult {
    let body = json!({"token": "abc", "base_url": "vip-api.opensubtitles.com",
        "user": {"allowed_downloads": 1000, "remaining_downloads": 990}, "status": 200});
    let (url, server) = serve(vec![reply(200, body)])?;
    let s = client(url)?.login("alice", &SecretString::new("pw".into()), &idle())?;
    assert_eq!(s.base_host, VIP_HOST);
    assert_eq!(
        (s.allowed_downloads, s.remaining_downloads),
        (Some(1000), Some(990))
    );
    let seen = server.join().map_err(|_| "panic")?;
    assert_eq!(request_line(&seen[0]), "POST /api/v1/login HTTP/1.1");
    assert!(seen[0].contains(r#""username":"alice""#));
    Ok(())
}

#[test]
fn login_rejects_foreign_base_host_and_missing_token() -> TestResult {
    let (url, _s) = serve(vec![reply(
        200,
        json!({"token": "abc", "base_url": "evil.example"}),
    )])?;
    assert_eq!(
        client(url)?
            .login("a", &SecretString::new("b".into()), &idle())
            .err(),
        Some(OpenSubtitlesError::InvalidResponse)
    );
    let (url, _s) = serve(vec![reply(200, json!({"user": {}}))])?;
    assert_eq!(
        client(url)?
            .login("a", &SecretString::new("b".into()), &idle())
            .err(),
        Some(OpenSubtitlesError::InvalidResponse)
    );
    Ok(())
}

#[test]
fn rejected_login_is_never_retried_with_same_credentials() -> TestResult {
    let (url, server) = serve(vec![
        reply(401, json!({"message": "bad"})),
        reply(200, json!({"token": "t"})),
    ])?;
    let c = client(url)?;
    let pw = SecretString::new("pw".into());
    assert_eq!(
        c.login("u", &pw, &idle()).err(),
        Some(OpenSubtitlesError::LoginRejected)
    );
    // Same credentials: refused locally, no second request.
    assert_eq!(
        c.login("u", &pw, &idle()).err(),
        Some(OpenSubtitlesError::LoginRejected)
    );
    // Changed credentials are tried.
    let session = c.login("u", &SecretString::new("pw2".into()), &idle())?;
    assert_eq!(session.base_host, API_HOST);
    assert_eq!(server.join().map_err(|_| "panic")?.len(), 2);
    Ok(())
}

#[test]
fn vip_session_routes_requests_to_vip_base_with_bearer() -> TestResult {
    let (api, api_server) = serve(vec![])?;
    let (vip, vip_server) = serve(vec![reply(200, json!({"data": []}))])?;
    let c = OpenSubtitlesClient::for_test(api, vip)?;
    c.search(&movie_query(), Some(&session(VIP_HOST)), &idle())?;
    let seen = vip_server.join().map_err(|_| "panic")?;
    assert_eq!(header(&seen[0], "authorization"), Some("Bearer tok-123"));
    assert!(api_server.join().map_err(|_| "panic")?.is_empty());
    Ok(())
}

#[test]
fn user_info_parses_allowance() -> TestResult {
    let body = json!({"data": {"allowed_downloads": 10, "remaining_downloads": 4, "vip": true}});
    let (url, server) = serve(vec![reply(200, body)])?;
    let info = client(url)?.user_info(&session(API_HOST), &idle())?;
    assert_eq!(
        info,
        UserInfo {
            allowed_downloads: Some(10),
            remaining_downloads: Some(4),
            vip: true
        }
    );
    let seen = server.join().map_err(|_| "panic")?;
    assert_eq!(request_line(&seen[0]), "GET /api/v1/infos/user HTTP/1.1");
    Ok(())
}

type Served = thread::JoinHandle<Vec<String>>;

fn download_pair(
    link_reply: Reply,
    remaining: Value,
) -> Result<(Url, Served, Served), std::io::Error> {
    let (link_url, link_server) = serve(vec![link_reply])?;
    let link = link_url.join("/file/abc").map_err(std::io::Error::other)?;
    let (api, api_server) = serve(vec![reply(
        200,
        json!({"link": link.as_str(), "file_name": "Movie.en.srt", "remaining": remaining,
               "reset_time_utc": "2026-11-08T00:00:00.000Z"}),
    )])?;
    Ok((api, api_server, link_server))
}

#[test]
fn download_posts_then_fetches_link_without_credentials() -> TestResult {
    let (api, api_server, link_server) = download_pair(raw(200, b"1\n00:00".to_vec()), json!(4))?;
    let d = client(api)?.download(42, Some(&session(API_HOST)), &idle())?;
    assert_eq!(d.bytes, b"1\n00:00");
    assert_eq!(d.format.as_deref(), Some("srt"));
    assert_eq!(d.file_name.as_deref(), Some("Movie.en.srt"));
    assert_eq!(d.remaining, Some(4));
    assert_eq!(
        d.reset_time_utc.as_deref(),
        Some("2026-11-08T00:00:00.000Z")
    );
    let api_seen = api_server.join().map_err(|_| "panic")?;
    assert_eq!(request_line(&api_seen[0]), "POST /api/v1/download HTTP/1.1");
    assert!(api_seen[0].contains(r#""file_id":42"#));
    assert_eq!(
        header(&api_seen[0], "authorization"),
        Some("Bearer tok-123")
    );
    let link_seen = link_server.join().map_err(|_| "panic")?;
    assert_eq!(header(&link_seen[0], "authorization"), None);
    assert_eq!(header(&link_seen[0], "api-key"), None);
    Ok(())
}

#[test]
fn download_rejects_empty_and_oversize_files() -> TestResult {
    let (api, _a, _l) = download_pair(raw(200, Vec::new()), json!(1))?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::InvalidResponse)
    );
    let (api, _a, _l) = download_pair(
        raw(200, vec![b'a'; MAX_SUBTITLE_BYTES as usize + 1]),
        json!(1),
    )?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::InvalidResponse)
    );
    Ok(())
}

#[test]
fn download_link_failures_map() -> TestResult {
    let (api, _a, _l) = download_pair(raw(500, Vec::new()), json!(1))?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::ServiceFailure)
    );
    let (api, _a, _l) = download_pair(raw(429, Vec::new()), json!(1))?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::RateLimited)
    );
    Ok(())
}

#[test]
fn download_rejects_disallowed_link_host() -> TestResult {
    let (api, server) = serve(vec![reply(
        200,
        json!({"link": "https://evil.example/f", "file_name": "a.srt"}),
    )])?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::InvalidResponse)
    );
    assert_eq!(server.join().map_err(|_| "panic")?.len(), 1);
    Ok(())
}

#[test]
fn download_status_mappings() -> TestResult {
    let (api, _s) = serve(vec![reply(401, json!({}))])?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::SignInRequired)
    );
    let (api, _s) = serve(vec![reply(401, json!({}))])?;
    assert_eq!(
        client(api)?
            .download(1, Some(&session(API_HOST)), &idle())
            .err(),
        Some(OpenSubtitlesError::SessionExpired)
    );
    let quota = json!({"message": "You have downloaded your allowed 5 subtitles", "remaining": 0,
        "reset_time_utc": "2026-11-08T00:00:00.000Z"});
    let (api, _s) = serve(vec![reply(406, quota)])?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::QuotaExhausted {
            reset_time_utc: Some("2026-11-08T00:00:00.000Z".into())
        })
    );
    let (api, _s) = serve(vec![raw(406, b"garbage".to_vec())])?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::QuotaExhausted {
            reset_time_utc: None
        })
    );
    let (api, _s) = serve(vec![reply(429, json!({}))])?;
    assert_eq!(
        client(api)?.download(1, None, &idle()).err(),
        Some(OpenSubtitlesError::RateLimited)
    );
    Ok(())
}

#[test]
fn cancellation_before_and_during_a_request() -> TestResult {
    let cancelled = AtomicBool::new(true);
    let c = client(Url::parse("http://127.0.0.1:1/api/v1/")?)?;
    assert_eq!(
        c.search(&movie_query(), None, &cancelled),
        Err(OpenSubtitlesError::Cancelled)
    );
    assert_eq!(
        c.download(1, None, &cancelled).err(),
        Some(OpenSubtitlesError::Cancelled)
    );
    assert_eq!(
        c.login("u", &SecretString::new("p".into()), &cancelled)
            .err(),
        Some(OpenSubtitlesError::Cancelled)
    );

    let mut slow = reply(200, json!({"data": []}));
    slow.delay = Duration::from_millis(400);
    let (url, _s) = serve(vec![slow])?;
    let flag = Arc::new(AtomicBool::new(false));
    let setter = Arc::clone(&flag);
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        setter.store(true, Ordering::Relaxed);
    });
    assert_eq!(
        client(url)?.search(&movie_query(), None, &flag),
        Err(OpenSubtitlesError::Cancelled)
    );
    canceller.join().map_err(|_| "panic")?;
    Ok(())
}

#[test]
fn blank_api_key_is_rejected_locally() {
    assert_eq!(
        OpenSubtitlesClient::new(SecretString::new("  ".into()), default_user_agent()).err(),
        Some(OpenSubtitlesError::ApiKeyRejected)
    );
    assert!(default_user_agent().starts_with("Kinema v"));
}

#[test]
fn spacing_enforces_minimum_interval() -> TestResult {
    let last = Mutex::new(None);
    let cancel = idle();
    let start = Instant::now();
    space(&last, Duration::from_millis(120), &cancel)?;
    space(&last, Duration::from_millis(120), &cancel)?;
    assert!(start.elapsed() >= Duration::from_millis(115));
    Ok(())
}
