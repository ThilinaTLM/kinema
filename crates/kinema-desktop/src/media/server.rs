use super::*;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path as RoutePath, State as HttpState},
    http::{HeaderMap, Method, StatusCode, header},
    response::Response,
    routing::get,
};
use kinema_core::media::parse_http_range;
use std::io::{Read, Seek, SeekFrom};
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
struct Server {
    shared: Arc<Shared>,
    slots: Arc<tokio::sync::Semaphore>,
}
pub(super) fn start(
    shared: Arc<Shared>,
) -> Result<(std::net::SocketAddr, tokio::sync::oneshot::Sender<()>), MediaError> {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    std::thread::Builder::new()
        .name("media-loopback".into())
        .spawn(move || {
            let owner = shared.clone();
            runtime.block_on(async move {
                let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                    return;
                };
                let router = Router::new()
                    .route("/media/{id}", get(serve))
                    .with_state(Server {
                        shared,
                        slots: Arc::new(tokio::sync::Semaphore::new(8)),
                    });
                let listener = LimitedListener {
                    listener,
                    slots: Arc::new(tokio::sync::Semaphore::new(32)),
                };
                let _ = axum::serve(listener, router)
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await;
            });
            runtime.shutdown_timeout(Duration::from_secs(2));
            drop(owner);
        })?;
    Ok((address, stop))
}
fn response(status: StatusCode, size: Option<u64>) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    if let Some(size) = size
        && let Ok(value) = format!("bytes */{size}").parse()
    {
        response.headers_mut().insert(header::CONTENT_RANGE, value);
    }
    response
}
async fn serve(
    HttpState(server): HttpState<Server>,
    RoutePath(id): RoutePath<String>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if server.shared.canceled.load(Ordering::Relaxed) {
        return response(StatusCode::SERVICE_UNAVAILABLE, None);
    }
    let Ok(permit) = server.slots.clone().try_acquire_owned() else {
        return response(StatusCode::SERVICE_UNAVAILABLE, None);
    };
    let lookup = server.shared.clone();
    let source = tokio::task::spawn_blocking(move || {
        lookup
            .state
            .lock()
            .ok()
            .and_then(|state| state.sessions.get(&id).cloned())
    })
    .await
    .ok()
    .flatten();
    let Some(source) = source else {
        return response(StatusCode::NOT_FOUND, None);
    };
    if !source.active.load(Ordering::Acquire) {
        return response(StatusCode::NOT_FOUND, None);
    }
    let size = match &source.source {
        Source::Local(_, size, _) => *size,
        Source::Remote(asset) => asset.identity.size,
    };
    let (start, end, partial) = if let Some(value) = headers.get(header::RANGE) {
        // Duplicate Range headers and multipart ranges are intentionally unsupported.
        if headers.get_all(header::RANGE).iter().count() != 1 {
            return response(StatusCode::RANGE_NOT_SATISFIABLE, Some(size));
        }
        match value
            .to_str()
            .ok()
            .and_then(|s| parse_http_range(s, size).ok())
        {
            Some(range) => (range.start(), range.end(), true),
            None => return response(StatusCode::RANGE_NOT_SATISFIABLE, Some(size)),
        }
    } else {
        (0, size, false)
    };
    let mut result = Response::new(Body::empty());
    *result.status_mut() = if partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let mime = match &source.source {
        Source::Local(_, _, mime) => *mime,
        Source::Remote(asset) => content_type(std::path::Path::new(&asset.identity.file_path)),
    };
    let headers = result.headers_mut();
    headers.insert(
        header::ACCEPT_RANGES,
        axum::http::HeaderValue::from_static("bytes"),
    );
    headers.insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static(mime),
    );
    headers.insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    let Ok(length) = (end - start).to_string().parse() else {
        return response(StatusCode::INTERNAL_SERVER_ERROR, None);
    };
    headers.insert(header::CONTENT_LENGTH, length);
    if partial {
        let Ok(value) = format!("bytes {start}-{}/{size}", end - 1).parse() else {
            return response(StatusCode::INTERNAL_SERVER_ERROR, None);
        };
        headers.insert(header::CONTENT_RANGE, value);
    }
    if method == Method::HEAD || start == end {
        return result;
    }
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(2);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let result = stream(&server.shared, source, start, end, &sender);
        if result.is_err() {
            let _ = sender.try_send(Err(std::io::Error::other("Media read interrupted")));
        }
    });
    *result.body_mut() = Body::from_stream(ReceiverStream::new(receiver));
    result
}
fn stream(
    shared: &Shared,
    lease: Lease,
    mut start: u64,
    end: u64,
    sender: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) -> Result<(), MediaError> {
    while start < end {
        if sender.is_closed()
            || shared.canceled.load(Ordering::Relaxed)
            || !lease.active.load(Ordering::Acquire)
        {
            return Err(MediaError::Canceled);
        }
        let next = end.min(start.saturating_add(64 * 1024));
        let mut bytes = vec![0u8; (next - start) as usize];
        match &lease.source {
            Source::Local(file, _, _) => {
                // Read-at is platform-specific; reopening is avoided to preserve file identity.
                read_at(file, start, &mut bytes)?;
            }
            Source::Remote(asset) => {
                let mut state = asset.state.lock().map_err(|_| MediaError::Storage)?;
                let deadline = std::time::Instant::now() + Duration::from_secs(45);
                while !storage::available(&state.ranges, start, next) {
                    if let Some(error) = &state.error {
                        return Err(error.clone());
                    }
                    if sender.is_closed()
                        || !lease.active.load(Ordering::Acquire)
                        || asset.cancel.load(Ordering::Relaxed)
                        || shared.canceled.load(Ordering::Relaxed)
                        || std::time::Instant::now() >= deadline
                    {
                        return Err(MediaError::Canceled);
                    }
                    state.priority = Some(start);
                    state = asset
                        .changed
                        .wait_timeout(state, Duration::from_millis(200))
                        .map_err(|_| MediaError::Storage)?
                        .0;
                }
                drop(state);
                storage::check_payload(asset)?;
                let mut file = File::open(asset.dir.join("payload"))?;
                file.seek(SeekFrom::Start(start))?;
                file.read_exact(&mut bytes)?;
            }
        }
        let mut message = Ok(Bytes::from(bytes));
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if !lease.active.load(Ordering::Acquire) || shared.canceled.load(Ordering::Relaxed) {
                return Err(MediaError::Canceled);
            }
            match sender.try_send(message) {
                Ok(()) => break,
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    return Err(MediaError::Canceled);
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(value)) => message = value,
            }
            if shared.canceled.load(Ordering::Relaxed) || std::time::Instant::now() >= deadline {
                return Err(MediaError::Canceled);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        start = next;
    }
    Ok(())
}
#[cfg(unix)]
fn read_at(file: &File, start: u64, bytes: &mut [u8]) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(bytes, start)
}
#[cfg(windows)]
fn read_at(file: &File, mut start: u64, mut bytes: &mut [u8]) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !bytes.is_empty() {
        let count = file.seek_read(bytes, start)?;
        if count == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        start += count as u64;
        bytes = &mut bytes[count..];
    }
    Ok(())
}

// Bound accepted sockets independently of body workers. A stalled local client cannot
// create unlimited tasks, file handles, or unbounded writes.
struct LimitedListener {
    listener: tokio::net::TcpListener,
    slots: Arc<tokio::sync::Semaphore>,
}
struct Connection {
    socket: tokio::net::TcpStream,
    _permit: tokio::sync::OwnedSemaphorePermit,
    idle: std::pin::Pin<Box<tokio::time::Sleep>>,
}
impl axum::serve::Listener for LimitedListener {
    type Io = Connection;
    type Addr = std::net::SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let permit = match self.slots.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => std::future::pending().await,
            };
            match self.listener.accept().await {
                Ok((socket, address)) => {
                    return (
                        Connection {
                            socket,
                            _permit: permit,
                            idle: Box::pin(tokio::time::sleep(Duration::from_secs(60))),
                        },
                        address,
                    );
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    }
    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}
impl Connection {
    fn timed_out(&mut self, cx: &mut std::task::Context<'_>) -> bool {
        std::future::Future::poll(self.idle.as_mut(), cx).is_ready()
    }
    fn progress(&mut self) {
        self.idle
            .as_mut()
            .reset(tokio::time::Instant::now() + Duration::from_secs(60));
    }
}
impl tokio::io::AsyncRead for Connection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.timed_out(cx) {
            return std::task::Poll::Ready(Err(std::io::ErrorKind::TimedOut.into()));
        }
        let result = std::pin::Pin::new(&mut this.socket).poll_read(cx, buf);
        if result.is_ready() {
            this.progress();
        }
        result
    }
}
impl tokio::io::AsyncWrite for Connection {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        if this.timed_out(cx) {
            return std::task::Poll::Ready(Err(std::io::ErrorKind::TimedOut.into()));
        }
        let result = std::pin::Pin::new(&mut this.socket).poll_write(cx, buf);
        if result.is_ready() {
            this.progress();
        }
        result
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.timed_out(cx) {
            return std::task::Poll::Ready(Err(std::io::ErrorKind::TimedOut.into()));
        }
        std::pin::Pin::new(&mut this.socket).poll_flush(cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().socket).poll_shutdown(cx)
    }
}
