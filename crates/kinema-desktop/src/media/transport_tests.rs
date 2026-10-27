use super::*;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
};

struct DelayedDns {
    address: SocketAddr,
    delay: Duration,
}
impl reqwest::dns::Resolve for DelayedDns {
    fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let address = self.address;
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Ok(Box::new(std::iter::once(address)) as reqwest::dns::Addrs)
        })
    }
}

#[test]
fn connect_budget_allows_one_five_second_dns_retry() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let worker = std::thread::spawn(move || -> std::io::Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(12);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return Err(std::io::ErrorKind::TimedOut.into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        };
        // Do not inherit the polling listener's mode on BSD/macOS.
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        let mut request = [0; 4096];
        assert!(stream.read(&mut request)? > 0);
        stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/1\r\nConnection: close\r\n\r\nx")
    });
    let client = client_builder()
        .no_proxy()
        .dns_resolver(Arc::new(DelayedDns {
            address,
            delay: Duration::from_millis(5100),
        }))
        .build()?;
    let response = client
        .get(format!("http://media.test:{}/fixture", address.port()))
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send();
    worker.join().map_err(|_| "fixture panicked")??;
    assert_eq!(response?.bytes()?.as_ref(), b"x");
    Ok(())
}

fn assert_redacted(error: MediaError, expected: MediaError) {
    assert_eq!(error, expected);
    let rendered = format!("{error} {error:?}");
    for secret in [
        "SECRET_TOKEN",
        "media.test",
        "http://",
        "signed-path",
        "token=",
    ] {
        assert!(!rendered.contains(secret));
    }
}

#[test]
fn request_timeouts_distinguish_connection_from_response_and_redact_urls()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    let url = format!(
        "http://media.test:{}/signed-path?token=SECRET_TOKEN",
        address.port()
    );
    let client = client_builder()
        .no_proxy()
        .connect_timeout(Duration::from_millis(30))
        .dns_resolver(Arc::new(DelayedDns {
            address,
            delay: Duration::from_millis(200),
        }))
        .build()?;
    let error = client
        .get(&url)
        .send()
        .err()
        .ok_or("expected DNS timeout")?;
    assert_redacted(request_error(error), MediaError::ConnectTimeout);

    listener.set_nonblocking(true)?;
    let worker = std::thread::spawn(move || -> std::io::Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return Err(std::io::ErrorKind::TimedOut.into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        };
        // Do not inherit the polling listener's mode on BSD/macOS.
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        let mut bytes = [0; 4096];
        let _ = stream.read(&mut bytes)?;
        // Deliberately withhold response headers beyond the total deadline.
        std::thread::sleep(Duration::from_millis(300));
        Ok(())
    });
    let client = client_builder()
        .no_proxy()
        .resolve("media.test", address)
        .timeout(Duration::from_millis(100))
        .build()?;
    let result = client.get(&url).send();
    worker.join().map_err(|_| "fixture panicked")??;
    let error = result.err().ok_or("expected response timeout")?;
    assert_redacted(request_error(error), MediaError::RequestTimeout);
    assert_redacted(MediaError::BodyRead, MediaError::BodyRead);
    assert_redacted(MediaError::RenewalExhausted, MediaError::RenewalExhausted);
    Ok(())
}
