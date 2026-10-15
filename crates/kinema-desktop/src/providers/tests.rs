use super::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;

pub(crate) fn fixture(
    replies: Vec<(u16, Value)>,
) -> Result<(Url, thread::JoinHandle<Vec<String>>), std::io::Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, json) in replies {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "fixture request timeout");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => panic!("fixture accept failed"),
                }
            };
            assert!(
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .is_ok()
            );
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = match stream.read(&mut buffer) {
                    Ok(n) => n,
                    Err(_) => panic!("fixture read failed"),
                };
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let size = headers
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + size {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8_lossy(&bytes).into_owned());
            let body = if status == 204 {
                String::new()
            } else {
                json.to_string()
            };
            let reply = format!(
                "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            assert!(stream.write_all(reply.as_bytes()).is_ok());
        }
        requests
    });
    let url = Url::parse(&format!("http://{address}/")).map_err(std::io::Error::other)?;
    Ok((url, handle))
}
fn source() -> Result<TorrentSource, Box<dyn std::error::Error>> {
    Ok(TorrentSource {
        hash: kinema_core::sources::InfoHash::new("0123456789abcdef0123456789abcdef01234567")?,
        file_index: Some(1),
        filename: None,
    })
}
fn client(kind: DebridKind, base: Url) -> Result<DebridClient, DebridError> {
    let mut c = DebridClient::new(kind, SecretString::new("private-token".to_owned()))?;
    c.base = base;
    c.polls = 1;
    Ok(c)
}
fn rd_info(status: &str) -> Value {
    json!({"status":status,"files":[{"id":1,"path":"/extra.txt","bytes":20,"selected":0},{"id":7,"path":"/season/episode.mkv","bytes":123,"selected":1},{"id":9,"path":"/other.mkv","bytes":456,"selected":1}],"links":["https://provider.invalid/episode","https://provider.invalid/other"]})
}
#[test]
fn rd_multifile_correct_link_and_renewal_identity() -> TestResult {
    let mut downloaded = rd_info("downloaded");
    downloaded["files"][2]["selected"] = json!(0);
    downloaded["links"] = json!(["https://provider.invalid/episode"]);
    let (base, server) = fixture(vec![
        (201, json!({"id":"job"})),
        (200, rd_info("waiting_files_selection")),
        (200, rd_info("waiting_files_selection")),
        (204, Value::Null),
        (200, downloaded.clone()),
        (
            200,
            json!({"download":"https://cdn.invalid/media?token=first","filesize":123}),
        ),
        (200, downloaded),
        (
            200,
            json!({"download":"https://cdn.invalid/media?token=second","filesize":123}),
        ),
    ])?;
    let mut c = client(DebridKind::RealDebrid, base)?;
    c.polls = 2;
    let cancel = AtomicBool::new(false);
    let prepared = c.prepare(&source()?, &cancel)?;
    assert_eq!(prepared.files.len(), 3);
    let first = c.resolve_file(&prepared, "7", &cancel)?;
    let renewed = c.resolve_file(&prepared, "7", &cancel)?;
    assert_eq!(first.identity, renewed.identity);
    assert_ne!(first.url.as_url(), renewed.url.as_url());
    assert!(!format!("{first:?}").contains("token=first"));
    let requests = server.join().map_err(|_| "fixture failed")?;
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.contains("/torrents/selectFiles/"))
            .count(),
        1
    );
    assert!(requests[..2].iter().all(|r| !r.contains("selectFiles")));
    assert!(requests[3].starts_with("POST /torrents/selectFiles/job "));
    assert!(requests[3].ends_with("\r\n\r\nfiles=7"));
    assert!(requests[5].contains("link=https%3A%2F%2Fprovider.invalid%2Fepisode"));
    Ok(())
}
fn ad(value: Value) -> Value {
    json!({"status":"success","data":value})
}
fn ad_files() -> Value {
    ad(
        json!({"magnets":[{"id":"42","files":[{"n":"season","e":[{"n":"one.mkv","s":123,"l":"https://ad.invalid/one"},{"n":"two.mkv","s":456,"l":"https://ad.invalid/two"}]}]}]}),
    )
}
#[test]
fn ad_nested_catalog_and_unlock() -> TestResult {
    let (base, server) = fixture(vec![
        (200, ad(json!({"magnets":[{"id":42}]}))),
        (200, ad(json!({"magnets":[{"id":42,"statusCode":4}]}))),
        (200, ad_files()),
        (200, ad_files()),
        (
            200,
            ad(json!({"link":"https://cdn.invalid/two?secret=hidden","filesize":456})),
        ),
    ])?;
    let c = client(DebridKind::AllDebrid, base)?;
    let cancel = AtomicBool::new(false);
    let prepared = c.prepare(&source()?, &cancel)?;
    assert_eq!(prepared.files.len(), 2);
    assert_eq!(prepared.files[1].path, "season/two.mkv");
    assert!(!format!("{prepared:?}").contains("https:"));
    let media = c.resolve_file(&prepared, "1", &cancel)?;
    assert_eq!(media.identity.file_path, "season/two.mkv");
    let requests = server.join().map_err(|_| "fixture failed")?;
    assert!(requests[1].starts_with("POST /v4.1/magnet/status"));
    assert!(requests[4].contains("link=https%3A%2F%2Fad.invalid%2Ftwo"));
    Ok(())
}
#[test]
fn auth_malformed_and_pending_are_not_empty() -> TestResult {
    for kind in [DebridKind::RealDebrid, DebridKind::AllDebrid] {
        let (base, server) = fixture(vec![(
            401,
            json!({"error":"private-token https://secret.invalid"}),
        )])?;
        let error = client(kind, base)?
            .prepare(&source()?, &AtomicBool::new(false))
            .err()
            .ok_or("expected failure")?;
        assert_eq!(error, DebridError::CredentialRejected);
        assert!(!format!("{error:?} {error}").contains("private-token"));
        server.join().map_err(|_| "fixture failed")?;
    }
    let (base, server) = fixture(vec![
        (200, json!({"id":"j"})),
        (200, json!({"status":"magnet_conversion"})),
    ])?;
    assert_eq!(
        client(DebridKind::RealDebrid, base)?.prepare(&source()?, &AtomicBool::new(false)),
        Err(DebridError::PendingTimeout)
    );
    server.join().map_err(|_| "fixture failed")?;
    let (base, server) = fixture(vec![
        (200, ad(json!({"magnets":[{"id":42}]}))),
        (200, ad(json!({"magnets":[{"id":42,"statusCode":1}]}))),
    ])?;
    assert_eq!(
        client(DebridKind::AllDebrid, base)?.prepare(&source()?, &AtomicBool::new(false)),
        Err(DebridError::PendingTimeout)
    );
    server.join().map_err(|_| "fixture failed")?;
    let (base, server) = fixture(vec![(200, json!({"unknown":"private-token"}))])?;
    assert_eq!(
        client(DebridKind::RealDebrid, base)?.prepare(&source()?, &AtomicBool::new(false)),
        Err(DebridError::Malformed)
    );
    server.join().map_err(|_| "fixture failed")?;
    Ok(())
}
#[test]
fn canceled_before_network_and_zero_size() -> TestResult {
    let c = DebridClient::new(
        DebridKind::RealDebrid,
        SecretString::new("secret".to_owned()),
    )?;
    assert_eq!(
        c.prepare(&source()?, &AtomicBool::new(true)),
        Err(DebridError::Canceled)
    );
    let prepared = PreparedTorrent {
        provider: DebridKind::RealDebrid,
        source: source()?,
        job_id: "j".to_owned(),
        files: vec![RemoteFile {
            id: "1".to_owned(),
            path: "file".to_owned(),
            size: 0,
        }],
    };
    assert!(matches!(
        c.resolve_file(&prepared, "1", &AtomicBool::new(false)),
        Err(DebridError::UnknownSize)
    ));
    assert!(SensitiveUrl::new(Url::parse("https://user:password@example.org/file")?).is_err());
    assert!(SensitiveUrl::new(Url::parse("file:///etc/passwd")?).is_err());
    Ok(())
}

#[test]
fn ad_delayed_ready_and_timeout() -> TestResult {
    for ready in [true, false] {
        let response = if ready {
            ad(json!({"status":2,"link":"https://cdn.invalid/renewed"}))
        } else {
            ad(json!({"status":1}))
        };
        let (base, server) = fixture(vec![
            (200, ad_files()),
            (200, ad(json!({"delayed":1234}))),
            (200, response),
        ])?;
        let c = client(DebridKind::AllDebrid, base)?;
        let prepared = PreparedTorrent {
            provider: DebridKind::AllDebrid,
            source: source()?,
            job_id: "42".to_owned(),
            files: vec![RemoteFile {
                id: "0".to_owned(),
                path: "season/one.mkv".to_owned(),
                size: 123,
            }],
        };
        let result = c.resolve_file(&prepared, "0", &AtomicBool::new(false));
        if ready {
            assert_eq!(result?.identity.file_path, "season/one.mkv");
        } else {
            assert!(matches!(result, Err(DebridError::PendingTimeout)));
        }
        let requests = server.join().map_err(|_| "fixture failed")?;
        assert!(requests[2].starts_with("POST /v4/link/delayed"));
    }
    Ok(())
}
#[test]
fn ad_rejected_envelope_and_malformed_tree() -> TestResult {
    let (base, server) = fixture(vec![(
        200,
        json!({"status":"error","error":{"code":"AUTH_BAD_APIKEY","message":"private-token"}}),
    )])?;
    assert_eq!(
        client(DebridKind::AllDebrid, base)?.prepare(&source()?, &AtomicBool::new(false)),
        Err(DebridError::CredentialRejected)
    );
    server.join().map_err(|_| "fixture failed")?;
    let (base, server) = fixture(vec![(
        200,
        ad(
            json!({"magnets":[{"id":"42","files":[{"n":"missing-size","l":"https://ad.invalid/file"}]}]}),
        ),
    )])?;
    let c = client(DebridKind::AllDebrid, base)?;
    let p = PreparedTorrent {
        provider: DebridKind::AllDebrid,
        source: source()?,
        job_id: "42".to_owned(),
        files: vec![RemoteFile {
            id: "0".to_owned(),
            path: "missing-size".to_owned(),
            size: 123,
        }],
    };
    assert!(matches!(
        c.resolve_file(&p, "0", &AtomicBool::new(false)),
        Err(DebridError::UnknownSize)
    ));
    server.join().map_err(|_| "fixture failed")?;
    Ok(())
}
#[test]
fn polling_sleep_is_cancelable() -> TestResult {
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        signal.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let start = Instant::now();
    assert_eq!(http::pause(&cancel), Err(DebridError::Canceled));
    assert!(start.elapsed() < Duration::from_secs(1));
    worker.join().map_err(|_| "cancel worker failed")?;
    Ok(())
}
#[test]
fn rd_inconsistent_link_mapping_is_rejected() -> TestResult {
    let mut info = rd_info("downloaded");
    info["links"] = json!(["https://provider.invalid/only-one"]);
    let (base, server) = fixture(vec![(200, info)])?;
    let c = client(DebridKind::RealDebrid, base)?;
    let p = PreparedTorrent {
        provider: DebridKind::RealDebrid,
        source: source()?,
        job_id: "job".to_owned(),
        files: vec![RemoteFile {
            id: "7".to_owned(),
            path: "/season/episode.mkv".to_owned(),
            size: 123,
        }],
    };
    assert!(matches!(
        c.resolve_file(&p, "7", &AtomicBool::new(false)),
        Err(DebridError::Malformed)
    ));
    server.join().map_err(|_| "fixture failed")?;
    Ok(())
}

#[test]
fn http_body_cap_and_invalid_json() -> TestResult {
    for (declared, body, expected) in [
        (http::MAX_BODY + 1, "", DebridError::TooLarge),
        (8, "not-json", DebridError::Malformed),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let address = listener.local_addr()?;
        let server = thread::spawn(move || -> std::io::Result<()> {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut request = [0; 4096];
            let _ = stream.read(&mut request)?;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n{body}"
            )?;
            Ok(())
        });
        let response = http::json(
            http::client()?.get(format!("http://{address}/")),
            &AtomicBool::new(false),
        );
        assert_eq!(response, Err(expected));
        server.join().map_err(|_| "fixture failed")??;
    }
    Ok(())
}

#[test]
fn accounts_authenticate_without_returning_account_details() -> TestResult {
    for (kind, account, route) in [
        (
            DebridKind::RealDebrid,
            json!({"id":123,"username":"private-user","email":"private-email","type":"free"}),
            "/user",
        ),
        (
            DebridKind::AllDebrid,
            ad(
                json!({"user":{"username":"private-user","isPremium":false,"email":"private-email"}}),
            ),
            "/v4/user",
        ),
    ] {
        let (base, server) = fixture(vec![(200, account)])?;
        let result = client(kind, base)?.check_account(&AtomicBool::new(false));
        assert_eq!(result, Ok(()));
        let requests = server.join().map_err(|_| "fixture failed")?;
        assert!(requests[0].starts_with(&format!("GET {route} HTTP/1.1")));
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer private-token\r\n")
        );
        assert!(
            !requests[0]
                .lines()
                .next()
                .ok_or("missing request line")?
                .contains("private-token")
        );
    }
    Ok(())
}

#[test]
fn account_rejected_malformed_service_and_network_errors_are_safe() -> TestResult {
    for kind in [DebridKind::RealDebrid, DebridKind::AllDebrid] {
        for (status, body, expected) in [
            (
                401,
                json!({"error":"private-token"}),
                DebridError::CredentialRejected,
            ),
            (
                403,
                json!({"error":"private-user"}),
                DebridError::CredentialRejected,
            ),
            (200, json!({}), DebridError::Malformed),
            (500, json!({"error":"private-email"}), DebridError::Service),
        ] {
            let (base, server) = fixture(vec![(status, body)])?;
            let result = client(kind, base)?.check_account(&AtomicBool::new(false));
            assert_eq!(result, Err(expected));
            let error = result.err().ok_or("expected failure")?;
            for secret in ["private-token", "private-user", "private-email"] {
                assert!(!format!("{error:?} {error}").contains(secret));
            }
            server.join().map_err(|_| "fixture failed")?;
        }
        // A reserved local listener closed before use deterministically refuses the connection.
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let base = Url::parse(&format!("http://{}/", listener.local_addr()?))?;
        drop(listener);
        assert_eq!(
            client(kind, base)?.check_account(&AtomicBool::new(false)),
            Err(DebridError::Transport)
        );
        let c = DebridClient::new(kind, SecretString::new("private-token".to_owned()))?;
        assert_eq!(
            c.check_account(&AtomicBool::new(true)),
            Err(DebridError::Canceled)
        );
    }
    let (base, server) = fixture(vec![(
        200,
        json!({"status":"error","error":{"code":"AUTH_BAD_APIKEY","message":"private-token"}}),
    )])?;
    assert_eq!(
        client(DebridKind::AllDebrid, base)?.check_account(&AtomicBool::new(false)),
        Err(DebridError::CredentialRejected)
    );
    server.join().map_err(|_| "fixture failed")?;
    for (kind, body) in [
        (
            DebridKind::RealDebrid,
            json!({"id":0,"username":"private-user"}),
        ),
        (DebridKind::RealDebrid, json!({"id":123,"username":null})),
        (DebridKind::AllDebrid, ad(json!({"user":{}}))),
        (
            DebridKind::AllDebrid,
            ad(json!({"user":{"username":"private-user","isPremium":"true"}})),
        ),
    ] {
        let (base, server) = fixture(vec![(200, body)])?;
        assert_eq!(
            client(kind, base)?.check_account(&AtomicBool::new(false)),
            Err(DebridError::Malformed)
        );
        server.join().map_err(|_| "fixture failed")?;
    }
    Ok(())
}

#[test]
fn rd_downloaded_mapping_and_unselected_file_do_not_reselect() -> TestResult {
    for id in ["9", "1"] {
        let mut responses = vec![(200, rd_info("downloaded"))];
        if id == "9" {
            responses.push((
                200,
                json!({"download":"https://cdn.invalid/other","filesize":456}),
            ));
        }
        let (base, server) = fixture(responses)?;
        let c = client(DebridKind::RealDebrid, base)?;
        let files = vec![
            RemoteFile {
                id: "9".to_owned(),
                path: "/other.mkv".to_owned(),
                size: 456,
            },
            RemoteFile {
                id: "1".to_owned(),
                path: "/extra.txt".to_owned(),
                size: 20,
            },
        ];
        let p = PreparedTorrent {
            provider: DebridKind::RealDebrid,
            source: source()?,
            job_id: "job".to_owned(),
            files,
        };
        let result = c.resolve_file(&p, id, &AtomicBool::new(false));
        if id == "9" {
            assert_eq!(result?.identity.file_path, "/other.mkv");
        } else {
            assert!(matches!(result, Err(DebridError::FileNotFound)));
        }
        let requests = server.join().map_err(|_| "fixture failed")?;
        assert!(requests.iter().all(|r| !r.contains("selectFiles")));
        if id == "9" {
            assert!(requests[1].contains("link=https%3A%2F%2Fprovider.invalid%2Fother"));
        }
    }
    Ok(())
}

#[test]
fn local_http_url_constructor_is_test_only() -> TestResult {
    let url = Url::parse("http://127.0.0.1:12345/media")?;
    assert!(SensitiveUrl::new(url.clone()).is_err());
    assert_eq!(SensitiveUrl::for_test(url.clone()).as_url(), &url);
    Ok(())
}
