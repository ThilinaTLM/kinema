//! Application-owned adapter workers. No registration occurs before explicit start.
use super::{
    Command, CommandSender, Playback, PlayerSnapshot, PlayerTarget,
    inhibition::{Inhibitor, PlayerActivity},
    media_session::MediaSession,
    tray::Tray,
};
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Default, Debug)]
pub struct RuntimeSnapshot {
    pub players: Vec<PlayerSnapshot>,
    pub selected: Option<PlayerTarget>,
    pub activities: Vec<PlayerActivity>,
}

#[derive(Clone)]
pub struct IconData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Default for IconData {
    fn default() -> Self {
        // Small in-memory K glyph. No file lifetime, image decoding or GTK dependency.
        let mut rgba = vec![0; 16 * 16 * 4];
        for y in 2..14usize {
            for x in 3..13usize {
                if x < 5 || (x as isize - 5 - (y as isize - 7).abs()).abs() < 2 {
                    rgba[(y * 16 + x) * 4..(y * 16 + x + 1) * 4]
                        .copy_from_slice(&[75, 205, 190, 255]);
                }
            }
        }
        Self {
            rgba,
            width: 16,
            height: 16,
        }
    }
}

#[derive(Clone, Default)]
pub struct RuntimeConfig {
    pub icon: IconData,
    #[cfg(target_os = "windows")]
    pub media_host: Option<Rc<dyn super::media_session::WindowsMediaHost>>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeStatus {
    pub tray_usable: bool,
    /// Deadline is mandatory when lifecycle caches a status between polls.
    pub tray_valid_until: Option<Instant>,
    pub media_registered: bool,
    pub inhibition_acquired: bool,
    pub native_error: Option<String>,
    pub inhibition_error: Option<String>,
    pub shutting_down: bool,
}

impl RuntimeStatus {
    pub fn tray_usable_at(&self, now: Instant) -> bool {
        !self.shutting_down
            && self.tray_usable
            && self.tray_valid_until.is_some_and(|deadline| now < deadline)
    }
}

#[derive(Default)]
struct Latest {
    revision: u64,
    snapshot: RuntimeSnapshot,
}

#[derive(Default)]
struct Shared {
    latest: Mutex<Latest>,
    wake: Condvar,
    stop: AtomicBool,
    status: Mutex<RuntimeStatus>,
}

impl Shared {
    fn status(&self, update: impl FnOnce(&mut RuntimeStatus)) {
        if let Ok(mut status) = self.status.lock() {
            update(&mut status);
        }
    }

    fn wait(&self, previous: u64) -> Option<(u64, RuntimeSnapshot)> {
        let mut latest = self.latest.lock().ok()?;
        if latest.revision == previous && !self.stop.load(Ordering::Acquire) {
            latest = self
                .wake
                .wait_timeout(latest, Duration::from_secs(1))
                .ok()?
                .0;
        }
        if self.stop.load(Ordering::Acquire) {
            None
        } else {
            Some((latest.revision, latest.snapshot.clone()))
        }
    }
}

/// Keep this owner in the app coordinator, not in Main. Start only after the
/// native application loop/window is ready. Windows/macOS: start/poll/drop on
/// GPUI's UI thread. Linux native adapters live on their own worker. All targets
/// have a separate thread-affine inhibitor worker. Runtime itself is !Send.
pub struct Runtime {
    shared: Arc<Shared>,
    #[cfg(target_os = "linux")]
    sender: CommandSender,
    inhibitor_worker: Option<JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    native_worker: Option<JoinHandle<()>>,
    #[cfg(not(target_os = "linux"))]
    native: NativeAdapters,
    #[cfg(not(target_os = "linux"))]
    ui_revision: u64,
    _ui_thread: PhantomData<Rc<()>>,
}

impl Runtime {
    pub fn start_after_native_ready(
        sender: CommandSender,
        config: RuntimeConfig,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        let inhibitor_worker = thread::Builder::new()
            .name("kinema-inhibition".into())
            .spawn(move || run_inhibitor(worker_shared))
            .map_err(|e| e.to_string())?;
        #[cfg(target_os = "linux")]
        let native_worker = {
            let worker_shared = shared.clone();
            let worker_sender = sender.clone();
            match thread::Builder::new()
                .name("kinema-desktop".into())
                .spawn(move || run_native(worker_shared, worker_sender, config))
            {
                Ok(worker) => worker,
                Err(error) => {
                    shared.stop.store(true, Ordering::Release);
                    shared.wake.notify_all();
                    let _ = inhibitor_worker.join();
                    return Err(error.to_string());
                }
            }
        };
        #[cfg(not(target_os = "linux"))]
        let native = NativeAdapters::create(sender.clone(), config);
        Ok(Self {
            shared,
            #[cfg(target_os = "linux")]
            sender,
            inhibitor_worker: Some(inhibitor_worker),
            #[cfg(target_os = "linux")]
            native_worker: Some(native_worker),
            #[cfg(not(target_os = "linux"))]
            native,
            #[cfg(not(target_os = "linux"))]
            ui_revision: u64::MAX,
            _ui_thread: PhantomData,
        })
    }

    /// Constant-size mailbox: replaces obsolete snapshots, never queues OS work.
    pub fn update(&mut self, snapshot: RuntimeSnapshot) {
        if self.shared.stop.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut latest) = self.shared.latest.lock() {
            latest.revision = latest.revision.wrapping_add(1);
            latest.snapshot = snapshot;
        }
        self.shared.wake.notify_all();
    }

    /// Call on the owned GPUI coordination task at least twice per second. On
    /// Windows/macOS this performs required UI-thread native updates; no artwork
    /// fetching/decoding is done here. Linux only reads shared worker status.
    pub fn poll(&mut self) -> RuntimeStatus {
        #[cfg(not(target_os = "linux"))]
        if !self.shared.stop.load(Ordering::Acquire) {
            let update = self
                .shared
                .latest
                .lock()
                .ok()
                .map(|latest| (latest.revision, latest.snapshot.clone()));
            if let Some((revision, snapshot)) = update {
                let changed = revision != self.ui_revision;
                self.ui_revision = revision;
                self.native.refresh(&snapshot, changed, &self.shared);
            }
        }
        #[cfg(target_os = "linux")]
        if self
            .native_worker
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
            && !self.shared.stop.load(Ordering::Acquire)
        {
            self.shared.status(|status| {
                status.tray_usable = false;
                status.media_registered = false;
                status.native_error = Some("Desktop adapter worker stopped".into());
            });
            self.sender.try_send(Command::TrayHostLost);
        }
        if self
            .inhibitor_worker
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
            && !self.shared.stop.load(Ordering::Acquire)
        {
            self.shared.status(|status| {
                status.inhibition_acquired = false;
                status.inhibition_error = Some("Inhibition worker stopped".into());
            });
        }
        let mut status = self
            .shared
            .status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default();
        status.tray_usable = status.tray_usable_at(Instant::now());
        status
    }

    /// Request cancellation before quitting; poll shutdown_complete from the
    /// existing bounded quit gate. No callback may be dispatched after this call.
    pub fn shutdown(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.wake.notify_all();
        self.shared.status(|status| {
            status.shutting_down = true;
            status.tray_usable = false;
        });
        #[cfg(not(target_os = "linux"))]
        self.native.close();
    }

    /// Nonblocking join of completed workers. OS calls already in progress may
    /// finish later; the application quit gate owns its overall timeout.
    pub fn shutdown_complete(&mut self) -> bool {
        join_finished(&mut self.inhibitor_worker);
        #[cfg(target_os = "linux")]
        join_finished(&mut self.native_worker);
        self.inhibitor_worker.is_none() && {
            #[cfg(target_os = "linux")]
            {
                self.native_worker.is_none()
            }
            #[cfg(not(target_os = "linux"))]
            {
                true
            }
        }
    }
}

fn join_finished(worker: &mut Option<JoinHandle<()>>) {
    if worker.as_ref().is_some_and(JoinHandle::is_finished)
        && let Some(worker) = worker.take()
    {
        let _ = worker.join();
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
        // Never turn the application's bounded quit gate into an unbounded UI
        // join. Normally shutdown_complete already joined these workers. If an
        // OS call outlives that deadline, cancellation remains set and the worker
        // retains only its native handles/shared state (no GPUI entities). Its
        // JoinHandle is detached at this final owner drop; it exits and releases
        // on its creation thread when that non-cancellable OS call returns.
        join_finished(&mut self.inhibitor_worker);
        #[cfg(target_os = "linux")]
        join_finished(&mut self.native_worker);
    }
}

type MediaSignature = Option<(PlayerTarget, Playback, bool, u64)>;
type MenuSignature = (
    Vec<(PlayerTarget, String, super::Playback)>,
    Option<PlayerTarget>,
);

struct NativeAdapters {
    tray: Option<Tray>,
    media: Option<MediaSession>,
    sender: CommandSender,
    error: Option<String>,
    config: RuntimeConfig,
    retry_at: Instant,
    last_probe: Option<Instant>,
    last_publish: Option<Instant>,
    media_signature: MediaSignature,
    menu_signature: Option<MenuSignature>,
    pending_publication: bool,
}

fn make_tray(config: &RuntimeConfig, sender: &CommandSender) -> Result<Tray, String> {
    let icon = tray_icon::Icon::from_rgba(
        config.icon.rgba.clone(),
        config.icon.width,
        config.icon.height,
    )
    .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    let result = Tray::create_on_worker(icon, sender.clone());
    #[cfg(not(target_os = "linux"))]
    let result = Tray::create_on_native_thread(icon, sender.clone());
    result.map_err(|e| format!("Tray registration: {e:?}"))
}

fn make_media(
    config: &RuntimeConfig,
    sender: &CommandSender,
    tray: Option<&Tray>,
) -> Result<MediaSession, String> {
    #[cfg(target_os = "windows")]
    let result = match config
        .media_host
        .clone()
        .or_else(|| tray.and_then(Tray::retained_media_host))
    {
        Some(host) => MediaSession::register_with_host(sender.clone(), host),
        None => MediaSession::register(sender.clone()),
    };
    #[cfg(not(target_os = "windows"))]
    let result = {
        let _ = (config, tray);
        MediaSession::register(sender.clone())
    };
    result.map_err(|e| format!("Media registration: {e:?}"))
}

impl NativeAdapters {
    fn create(sender: CommandSender, config: RuntimeConfig) -> Self {
        let mut error = None;
        let tray = make_tray(&config, &sender)
            .map_err(|e| error = Some(e))
            .ok();
        let media = make_media(&config, &sender, tray.as_ref())
            .map_err(|e| error = Some(e))
            .ok();
        Self {
            tray,
            media,
            sender,
            error,
            config,
            retry_at: Instant::now() + Duration::from_secs(5),
            last_probe: None,
            last_publish: None,
            media_signature: None,
            menu_signature: None,
            pending_publication: true,
        }
    }

    fn refresh(&mut self, snapshot: &RuntimeSnapshot, changed: bool, shared: &Shared) {
        let mut changed = changed;
        if Instant::now() >= self.retry_at {
            if self.tray.is_none() {
                self.last_probe = None;
                self.menu_signature = None;
                self.tray = make_tray(&self.config, &self.sender)
                    .map_err(|e| self.error = Some(e))
                    .ok();
                changed = true;
            }
            if self.media.is_none() {
                self.media = make_media(&self.config, &self.sender, self.tray.as_ref())
                    .map_err(|e| self.error = Some(e))
                    .ok();
                changed = true;
            }
            self.retry_at = Instant::now() + Duration::from_secs(5);
        }
        let selected = snapshot
            .players
            .iter()
            .find(|player| Some(player.target) == snapshot.selected);
        let now = Instant::now();
        let media_signature = selected.map(|player| {
            (
                player.target,
                player.playback,
                player.seekable,
                player.volume.to_bits(),
            )
        });
        // Position-only samples are coalesced to 1 Hz. Selection, play/pause,
        // seekability and volume changes publish immediately, keeping callback
        // gating short and avoiding repeated native menu replacement while open.
        let publish_due = self.last_publish.is_none_or(|previous| {
            now.saturating_duration_since(previous) >= Duration::from_secs(1)
        }) || self.media_signature != media_signature;
        self.pending_publication |= changed;
        if self.pending_publication && publish_due {
            if let Some(media) = &mut self.media
                && let Err(error) = media.publish(selected)
            {
                self.error = Some(format!("Media publication: {error}"));
                self.media = None;
            }
            self.last_publish = Some(now);
            self.media_signature = media_signature;
            self.pending_publication = false;
        }
        let menu_signature = (
            snapshot
                .players
                .iter()
                .map(|player| (player.target, player.title.clone(), player.playback))
                .collect(),
            snapshot.selected,
        );
        if self.menu_signature.as_ref() != Some(&menu_signature) {
            if let Some(tray) = &mut self.tray
                && let Err(error) = tray.update_menu(&snapshot.players, snapshot.selected)
            {
                self.error = Some(format!("Tray menu: {error:?}"));
                self.tray = None;
            }
            self.menu_signature = Some(menu_signature);
        }
        let now = Instant::now();
        let mut recreate = false;
        let probe_due = self.last_probe.is_none_or(|previous| {
            now.saturating_duration_since(previous) >= Duration::from_secs(2)
        });
        let usable = if let Some(tray) = &mut self.tray {
            if probe_due {
                self.last_probe = Some(now);
                match tray.refresh_host(now) {
                    Ok(()) => tray.usable_at(now),
                    Err(error) => {
                        recreate = matches!(error, super::tray::TrayError::HostChanged);
                        self.error = Some(format!("Tray host: {error:?}"));
                        false
                    }
                }
            } else {
                tray.usable_at(now)
            }
        } else {
            false
        };
        if recreate {
            self.tray = None;
        }
        if usable && self.media.is_some() {
            self.error = None;
        }
        shared.status(|status| {
            status.tray_usable = usable;
            status.tray_valid_until = if usable {
                self.last_probe
                    .map(|checked| checked + Duration::from_secs(5))
            } else {
                None
            };
            status.media_registered = self.media.is_some();
            status.native_error = self.error.clone();
        });
        if !usable {
            self.sender.try_send(Command::TrayHostLost);
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn close(&mut self) {
        self.media = None;
        self.tray = None;
    }
}

#[cfg(target_os = "linux")]
fn run_native(shared: Arc<Shared>, sender: CommandSender, config: RuntimeConfig) {
    let mut adapters = NativeAdapters::create(sender, config);
    let mut revision = u64::MAX;
    while let Some((next, snapshot)) = shared.wait(revision) {
        adapters.refresh(&snapshot, revision != next, &shared);
        revision = next;
    }
    drop(adapters);
    shared.status(|status| {
        status.tray_usable = false;
        status.media_registered = false;
    });
}

fn run_inhibitor(shared: Arc<Shared>) {
    let mut inhibitor = Inhibitor::default();
    let mut revision = u64::MAX;
    let mut retry_after = Instant::now();
    while let Some((next, snapshot)) = shared.wait(revision) {
        revision = next;
        let now = Instant::now();
        let required = super::inhibition::inhibition_required(&snapshot.activities, now);
        // A denied backend must not be hammered by frequent playback samples.
        // Release/pause and buffering expiry are never delayed by retry backoff.
        if !required || inhibitor.is_acquired() || now >= retry_after {
            let result = inhibitor.reconcile(&snapshot.activities, now);
            if result.is_err() {
                retry_after = now + Duration::from_secs(5);
            }
            shared.status(|status| {
                status.inhibition_acquired = inhibitor.is_acquired();
                status.inhibition_error = result.err();
            });
        }
    }
    inhibitor.release();
    shared.status(|status| status.inhibition_acquired = false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn latest_update_and_owned_inhibitor_worker_cancel_without_os_registration() {
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        let worker = thread::spawn(move || run_inhibitor(worker_shared));
        let (sender, _receiver) = super::super::command_channel(1);
        let mut runtime = Runtime {
            shared,
            sender,
            inhibitor_worker: Some(worker),
            native_worker: None,
            _ui_thread: PhantomData,
        };
        for generation in [1, 2] {
            runtime.update(RuntimeSnapshot {
                selected: Some(PlayerTarget {
                    session_id: 7,
                    generation,
                }),
                ..RuntimeSnapshot::default()
            });
        }
        let latest = runtime
            .shared
            .latest
            .lock()
            .ok()
            .map(|latest| latest.snapshot.selected);
        assert_eq!(
            latest,
            Some(Some(PlayerTarget {
                session_id: 7,
                generation: 2
            }))
        );
        runtime.shutdown();
        let joined = runtime
            .inhibitor_worker
            .take()
            .map(|worker| worker.join().is_ok());
        assert_eq!(joined, Some(true));
        assert!(runtime.shutdown_complete());
        assert!(runtime.poll().shutting_down);
    }

    #[test]
    fn health_deadline_and_shutdown_are_fail_closed() {
        let now = Instant::now();
        let mut status = RuntimeStatus {
            tray_usable: true,
            tray_valid_until: Some(now + Duration::from_secs(5)),
            ..RuntimeStatus::default()
        };
        assert!(status.tray_usable_at(now));
        assert!(!status.tray_usable_at(now + Duration::from_secs(5)));
        status.shutting_down = true;
        assert!(!status.tray_usable_at(now));
    }

    #[test]
    fn mailbox_is_latest_value_and_cancel_wakes_without_os_calls() {
        let shared = Shared::default();
        if let Ok(mut latest) = shared.latest.lock() {
            latest.revision = 4;
            latest.snapshot.selected = Some(PlayerTarget {
                session_id: 9,
                generation: 2,
            });
        }
        let received = shared.wait(3);
        assert_eq!(
            received.map(|(revision, snapshot)| (revision, snapshot.selected)),
            Some((
                4,
                Some(PlayerTarget {
                    session_id: 9,
                    generation: 2
                })
            ))
        );
        shared.stop.store(true, Ordering::Release);
        assert!(shared.wait(4).is_none());
    }
}
