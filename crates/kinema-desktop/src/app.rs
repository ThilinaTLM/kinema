//! Bounded application composition. No filesystem, vault, network or process work
//! happens in submit/poll; dropping the handle signals shutdown without joining.
use crate::{
    credentials::{CredentialError, CredentialStore, MetadataCredentialStore, SecretString},
    media::{MediaError, MediaService, PlaybackSession, UsageSnapshot},
    paths::AppPaths,
    player,
    providers::{DebridClient, DebridError, PreparedTorrent, ResolvedMedia},
    settings::{Settings, SettingsError, SettingsStore},
    tmdb::TmdbClient,
};
use kinema_core::{
    CacheBudget,
    sources::{
        DebridKind, FileHint, FileSelection, PlaybackTarget, SourceCandidate, select_file_with_hint,
    },
};
use std::{
    collections::VecDeque,
    io,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const CAPACITY: usize = 24;
const TICK: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct AppCommand {
    pub generation: u64,
    pub action: AppAction,
}
#[derive(Debug)]
pub enum AppAction {
    LoadSettings,
    SaveSettings(Settings),
    SetCredential {
        provider: DebridKind,
        value: SecretString,
    },
    DeleteCredential(DebridKind),
    TestCredential(DebridKind),
    SetTmdbCredential {
        value: SecretString,
    },
    DeleteTmdbCredential,
    TestTmdbCredential,
    Prepare {
        target: PlaybackTarget,
        candidate: SourceCandidate,
    },
    PlayFile {
        preparation: u64,
        file_id: String,
    },
    PlayLocal {
        path: PathBuf,
    },
    ReleaseSession(String),
    CleanupUnused,
    ReplaySession {
        session_id: String,
    },
    RevealSession {
        session_id: String,
    },
}
impl AppAction {
    pub(crate) fn operation(&self) -> &'static str {
        match self {
            Self::LoadSettings => "load_settings",
            Self::SaveSettings(_) => "save_settings",
            Self::SetCredential { .. } => "set_credential",
            Self::DeleteCredential(_) => "delete_credential",
            Self::TestCredential(_) => "test_credential",
            Self::SetTmdbCredential { .. } => "set_tmdb_credential",
            Self::DeleteTmdbCredential => "delete_tmdb_credential",
            Self::TestTmdbCredential => "test_tmdb_credential",
            Self::Prepare { .. } => "prepare",
            Self::PlayFile { .. } => "play_file",
            Self::PlayLocal { .. } => "play_local",
            Self::ReleaseSession(_) => "release_session",
            Self::CleanupUnused => "cleanup_unused",
            Self::ReplaySession { .. } => "replay_session",
            Self::RevealSession { .. } => "reveal_session",
        }
    }
}
// Never format AppAction/AppOutcome: they contain URLs, filenames and tokens.
fn log_operation_result(result: &Result<AppOutcome, String>, canceled: bool, started: Instant) {
    let duration_ms = started.elapsed().as_millis() as u64;
    match result {
        Err(_) if canceled => tracing::debug!(
            duration_ms,
            outcome = "cancelled",
            "Application operation discarded"
        ),
        Err(_) => tracing::warn!(
            duration_ms,
            category = "operation_failed",
            "Application operation failed"
        ),
        Ok(
            AppOutcome::Failed { .. }
            | AppOutcome::CacheFull { .. }
            | AppOutcome::CacheLimitNeeded { .. }
            | AppOutcome::Busy { .. }
            | AppOutcome::SettingsWarning { .. },
        ) => tracing::warn!(
            duration_ms,
            category = "user_visible_failure",
            "Application operation needs user attention"
        ),
        Ok(AppOutcome::Prepared { prepared, .. }) => {
            tracing::info!(duration_ms, provider = ?prepared.provider, files = prepared.files.len(), "Application preparation completed")
        }
        Ok(
            AppOutcome::PlaybackStarted { .. }
            | AppOutcome::EmbeddedPlaybackStarted { .. }
            | AppOutcome::SessionRelaunched { .. },
        ) => {
            tracing::info!(duration_ms, "Application player launch completed")
        }
        Ok(AppOutcome::CredentialChecked { provider }) => {
            tracing::info!(duration_ms, ?provider, "Credential check succeeded")
        }
        Ok(AppOutcome::TmdbCredentialChecked) => {
            tracing::info!(duration_ms, provider = "tmdb", "Credential check succeeded")
        }
        Ok(_) => tracing::debug!(
            duration_ms,
            outcome = "completed",
            "Application operation completed"
        ),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialState {
    Present,
    Missing,
    Unavailable(String),
}
#[derive(Clone, Debug)]
pub struct AppEvent {
    pub generation: u64,
    pub outcome: AppOutcome,
}
#[derive(Clone, Debug)]
pub enum AppOutcome {
    SettingsWarning {
        settings: Settings,
        credentials: Vec<(DebridKind, CredentialState)>,
        tmdb_credential: CredentialState,
        message: String,
    },
    Settings {
        settings: Settings,
        credentials: Vec<(DebridKind, CredentialState)>,
        tmdb_credential: CredentialState,
    },
    /// Auto-play only `Unique(index)`; otherwise show `selection.error_message()`.
    /// Non-unique preparations cannot be used by `PlayFile`.
    Prepared {
        prepared: PreparedTorrent,
        target: PlaybackTarget,
        selection: FileSelection,
    },
    PlaybackStarted {
        session: PlaybackSession,
        file_name: String,
    },
    /// The worker owns the lease; the UI must open/activate its managed window.
    EmbeddedPlaybackStarted {
        session: PlaybackSession,
        file_name: String,
        mode: player::PlayerChoice,
    },
    CredentialChecked {
        provider: DebridKind,
    },
    MetadataCredential {
        status: CredentialState,
    },
    TmdbCredentialChecked,
    MediaFailed {
        session_id: String,
        message: String,
    },
    Invalidated {
        released: Vec<String>,
    },
    CacheFull {
        message: String,
    },
    CacheLimitNeeded {
        size: u64,
        limit: u64,
    },
    Released {
        id: String,
    },
    Cleaned {
        bytes: u64,
        protected_files: usize,
    },
    Usage(UsageSnapshot),
    SessionUpdates(Vec<crate::media::SessionUpdate>),
    SessionRelaunched {
        session_id: String,
    },
    RevealRequested {
        session_id: String,
    },
    Failed {
        message: String,
    },
    Busy {
        label: String,
    },
}

struct Request {
    command: AppCommand,
    cancel: Arc<AtomicBool>,
    credential_revisions: [u64; 2],
}
struct Mailbox {
    requests: VecDeque<Request>,
    events: VecDeque<AppEvent>,
    outstanding: usize,
    stopped: bool,
    pipeline: Vec<Arc<AtomicBool>>,
    metadata_pipeline: Vec<Arc<AtomicBool>>,
    account: Arc<AtomicBool>,
    active: Option<Arc<AtomicBool>>,
    active_generation: Option<u64>,
    discarded_preparations: Vec<u64>,
    prepared_generation: Option<u64>,
    // Identity belongs to `account`, not to a potentially unfinished settings
    // save. Publish identity + fresh flag together under this mailbox lock.
    scope_provider: Option<DebridKind>,
    account_epoch: u64,
    credential_revisions: [u64; 2],
}
struct Shared {
    mailbox: Mutex<Mailbox>,
    wake: Condvar,
    terminated: AtomicBool,
}
pub struct AppWorker {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(all(test, feature = "ui-probe-test"))]
impl AppWorker {
    pub(crate) fn test_mailbox() -> Self {
        Self {
            shared: new_shared(),
            thread: None,
        }
    }

    pub(crate) fn test_drain_requests(&self) -> Vec<AppCommand> {
        let mut mail = self
            .shared
            .mailbox
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let commands = mail
            .requests
            .drain(..)
            .map(|request| request.command)
            .collect();
        mail.outstanding = 0;
        commands
    }
}
fn new_shared() -> Arc<Shared> {
    Arc::new(Shared {
        mailbox: Mutex::new(Mailbox {
            requests: VecDeque::new(),
            events: VecDeque::new(),
            outstanding: 0,
            stopped: false,
            pipeline: Vec::new(),
            metadata_pipeline: Vec::new(),
            account: Arc::new(AtomicBool::new(false)),
            active: None,
            active_generation: None,
            discarded_preparations: Vec::new(),
            prepared_generation: None,
            scope_provider: None,
            account_epoch: 0,
            credential_revisions: [0; 2],
        }),
        wake: Condvar::new(),
        terminated: AtomicBool::new(false),
    })
}
// Terminal command outcomes and budgeted media failures consume a reserved
// slot. Invalidation is ordered before its command terminal, so at most one
// such control per outstanding command can exist. Usage has one lossy slot.
fn consumes_budget(outcome: &AppOutcome) -> bool {
    !matches!(
        outcome,
        AppOutcome::Usage(_) | AppOutcome::SessionUpdates(_) | AppOutcome::Invalidated { .. }
    )
}
fn emit_invalidation(shared: &Shared, released: Vec<String>) {
    let mut mail = shared.mailbox.lock().unwrap_or_else(|e| e.into_inner());
    if !mail.stopped {
        mail.events.push_back(AppEvent {
            generation: 0,
            outcome: AppOutcome::Invalidated { released },
        });
    }
}
struct CloseOnExit(Arc<Shared>);
impl Drop for CloseOnExit {
    fn drop(&mut self) {
        tracing::info!("Application worker stopped");
        let mut mail = self.0.mailbox.lock().unwrap_or_else(|e| e.into_inner());
        mail.stopped = true;
        mail.account.store(true, Ordering::Release);
        if let Some(cancel) = mail.active.take() {
            cancel.store(true, Ordering::Release);
        }
        for cancel in mail.pipeline.drain(..) {
            cancel.store(true, Ordering::Release);
        }
        for cancel in mail.metadata_pipeline.drain(..) {
            cancel.store(true, Ordering::Release);
        }
        if let Some(generation) = mail.active_generation.take() {
            mail.events.push_back(AppEvent {
                generation,
                outcome: failed("Application worker stopped before completing the request"),
            });
        }
        while let Some(request) = mail.requests.pop_front() {
            request.cancel.store(true, Ordering::Release);
            mail.events.push_back(AppEvent {
                generation: request.command.generation,
                outcome: failed("Application worker stopped before completing the request"),
            });
        }
        self.0.terminated.store(true, Ordering::Release);
        self.0.wake.notify_all();
    }
}

impl AppWorker {
    pub fn spawn() -> io::Result<Self> {
        Self::spawn_task(run)
    }
    fn spawn_task(task: impl FnOnce(Arc<Shared>) + Send + 'static) -> io::Result<Self> {
        let shared = new_shared();
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("application".into())
            .spawn(move || {
                let _close = CloseOnExit(worker.clone());
                task(worker);
            })?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }
    /// True after normal or unexpected worker termination. Never joins on the UI.
    pub fn is_stopped(&self) -> bool {
        self.shared.terminated.load(Ordering::Acquire)
            || self
                .thread
                .as_ref()
                .is_some_and(|thread| thread.is_finished())
    }
    /// False means backpressure: ownership was not queued and the caller must retry.
    pub fn submit(&self, command: AppCommand) -> bool {
        let Ok(mut mail) = self.shared.mailbox.lock() else {
            return false;
        };
        if mail.stopped || mail.outstanding >= CAPACITY {
            return false;
        }
        if let Some(provider) = credential_mutation(&command.action) {
            mail.credential_revisions[provider_index(provider)] += 1;
        }
        let request_family = family(&command.action);
        if invalidates_account(&command.action, mail.scope_provider) {
            mail.account.store(true, Ordering::Release);
        }
        if pipeline(&command.action)
            || invalidates_account(&command.action, mail.scope_provider)
            || matches!(command.action, AppAction::SaveSettings(_))
        {
            for cancel in mail.pipeline.drain(..) {
                cancel.store(true, Ordering::Release);
            }
        }
        if metadata_action(&command.action) {
            for cancel in mail.metadata_pipeline.drain(..) {
                cancel.store(true, Ordering::Release);
            }
        }
        // Coalesce only replaceable reads/preparations, never writes or release actions.
        if let Some(family_key) = request_family
            && let Some(index) = mail
                .requests
                .iter()
                .position(|r| family(&r.command.action) == Some(family_key))
            && let Some(old) = mail.requests.remove(index)
        {
            old.cancel.store(true, Ordering::Release);
            mail.events.push_back(AppEvent {
                generation: old.command.generation,
                outcome: failed("Request superseded"),
            });
        }
        let cancel = Arc::new(AtomicBool::new(false));
        if pipeline(&command.action) {
            mail.pipeline.push(cancel.clone());
        }
        if matches!(command.action, AppAction::TestTmdbCredential) {
            mail.metadata_pipeline.push(cancel.clone());
        }
        let credential_revisions = mail.credential_revisions;
        mail.requests.push_back(Request {
            command,
            cancel,
            credential_revisions,
        });
        mail.outstanding += 1;
        self.shared.wake.notify_one();
        true
    }
    /// Nonblocking cancellation. A launch already handed to the OS is never killed.
    pub fn cancel_request(&self, generation: u64) -> bool {
        let Ok(mut mail) = self.shared.mailbox.lock() else {
            return false;
        };
        if mail.stopped {
            return false;
        }
        // At most the bounded queue, one active command, and one retained preparation
        // may own cancellation controls. Unknown generations never allocate controls.
        if mail.active_generation != Some(generation)
            && mail.prepared_generation != Some(generation)
            && !mail
                .requests
                .iter()
                .any(|request| request.command.generation == generation)
        {
            return false;
        }
        if mail.active_generation == Some(generation)
            && let Some(flag) = &mail.active
        {
            flag.store(true, Ordering::Release);
        }
        for request in &mail.requests {
            if request.command.generation == generation {
                request.cancel.store(true, Ordering::Release);
            }
        }
        if !mail.discarded_preparations.contains(&generation) {
            mail.discarded_preparations.push(generation);
        }
        self.shared.wake.notify_one();
        true
    }
    pub fn poll(&self) -> Option<AppEvent> {
        let mut mail = self
            .shared
            .mailbox
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let event = mail.events.pop_front()?;
        if consumes_budget(&event.outcome) {
            mail.outstanding -= 1;
        }
        Some(event)
    }
}
impl Drop for AppWorker {
    fn drop(&mut self) {
        if let Ok(mut mail) = self.shared.mailbox.lock() {
            mail.stopped = true;
            if let Some(cancel) = &mail.active {
                cancel.store(true, Ordering::Release);
            }
            mail.account.store(true, Ordering::Release);
            for cancel in mail.pipeline.drain(..) {
                cancel.store(true, Ordering::Release);
            }
            for cancel in mail.metadata_pipeline.drain(..) {
                cancel.store(true, Ordering::Release);
            }
            self.shared.wake.notify_all();
        }
    }
}
fn family(action: &AppAction) -> Option<u8> {
    match action {
        AppAction::LoadSettings => Some(0),
        AppAction::Prepare { .. } => Some(1),
        AppAction::PlayFile { .. } | AppAction::PlayLocal { .. } => Some(2),
        AppAction::TestCredential(DebridKind::RealDebrid) => Some(3),
        AppAction::TestCredential(DebridKind::AllDebrid) => Some(4),
        AppAction::TestTmdbCredential => Some(5),
        _ => None,
    }
}
fn pipeline(action: &AppAction) -> bool {
    matches!(
        action,
        AppAction::Prepare { .. }
            | AppAction::PlayFile { .. }
            | AppAction::PlayLocal { .. }
            | AppAction::TestCredential(_)
    )
}
fn metadata_action(action: &AppAction) -> bool {
    matches!(
        action,
        AppAction::SetTmdbCredential { .. }
            | AppAction::DeleteTmdbCredential
            | AppAction::TestTmdbCredential
    )
}
fn provider_index(provider: DebridKind) -> usize {
    match provider {
        DebridKind::RealDebrid => 0,
        DebridKind::AllDebrid => 1,
    }
}
fn credential_mutation(action: &AppAction) -> Option<DebridKind> {
    match action {
        AppAction::SetCredential { provider, .. } | AppAction::DeleteCredential(provider) => {
            Some(*provider)
        }
        _ => None,
    }
}
fn rotate_account(mail: &mut Mailbox, provider: Option<DebridKind>) {
    mail.account.store(true, Ordering::Release);
    mail.account_epoch += 1;
    mail.scope_provider = provider;
    mail.account = Arc::new(AtomicBool::new(false));
}
fn begin_account_action(mail: &mut Mailbox, action: &AppAction) -> bool {
    if !invalidates_account(action, mail.scope_provider) {
        return false;
    }
    let provider = match action {
        AppAction::SaveSettings(settings) => Some(settings.debrid),
        _ => mail.scope_provider,
    };
    rotate_account(mail, provider);
    true
}
fn reconcile_account(mail: &mut Mailbox, effective: DebridKind) {
    if mail.scope_provider != Some(effective) {
        rotate_account(mail, Some(effective));
    }
}

fn validate_credential_revision(
    mail: &Mailbox,
    revisions: [u64; 2],
    provider: DebridKind,
) -> Result<(), String> {
    if revisions[provider_index(provider)] != mail.credential_revisions[provider_index(provider)] {
        return Err("Credential changed; retry the request".into());
    }
    Ok(())
}

fn invalidates_account(action: &AppAction, current: Option<DebridKind>) -> bool {
    match action {
        AppAction::SaveSettings(settings) => Some(settings.debrid) != current,
        AppAction::SetCredential { provider, .. } | AppAction::DeleteCredential(provider) => {
            Some(*provider) == current
        }
        _ => false,
    }
}
fn failed(message: impl Into<String>) -> AppOutcome {
    AppOutcome::Failed {
        message: message.into(),
    }
}
fn credential_state(result: Result<bool, CredentialError>) -> CredentialState {
    match result {
        Ok(true) => CredentialState::Present,
        Ok(false) | Err(CredentialError::Missing) => CredentialState::Missing,
        Err(error) => CredentialState::Unavailable(error.to_string()),
    }
}

// Three playback boundary seams: secure read, provider client, external launch.
// The production composition below remains the only orchestration path; tests
// replace these boundaries while retaining the actual worker and MediaService.
trait DebridApi: Send + Sync {
    fn prepare(
        &self,
        source: &kinema_core::sources::TorrentSource,
        cancel: &AtomicBool,
    ) -> Result<PreparedTorrent, DebridError>;
    fn resolve_file(
        &self,
        prepared: &PreparedTorrent,
        file_id: &str,
        cancel: &AtomicBool,
    ) -> Result<ResolvedMedia, DebridError>;
    fn check_account(&self, cancel: &AtomicBool) -> Result<(), DebridError>;
}
impl DebridApi for DebridClient {
    fn prepare(
        &self,
        source: &kinema_core::sources::TorrentSource,
        cancel: &AtomicBool,
    ) -> Result<PreparedTorrent, DebridError> {
        DebridClient::prepare(self, source, cancel)
    }
    fn resolve_file(
        &self,
        prepared: &PreparedTorrent,
        file_id: &str,
        cancel: &AtomicBool,
    ) -> Result<ResolvedMedia, DebridError> {
        DebridClient::resolve_file(self, prepared, file_id, cancel)
    }
    fn check_account(&self, cancel: &AtomicBool) -> Result<(), DebridError> {
        DebridClient::check_account(self, cancel)
    }
}
type CredentialRead = dyn Fn(DebridKind) -> Result<SecretString, CredentialError> + Send;
type ClientFactory =
    dyn Fn(DebridKind, SecretString) -> Result<Arc<dyn DebridApi>, DebridError> + Send;
type PlayerLaunch = dyn Fn(&player::PlayerChoice, &str) -> Result<Option<std::process::Child>, player::PlayerError>
    + Send;
struct PlaybackIo {
    credential: Box<CredentialRead>,
    client: Box<ClientFactory>,
    launch: Box<PlayerLaunch>,
}
impl Default for PlaybackIo {
    fn default() -> Self {
        Self {
            credential: Box::new(|provider| CredentialStore::new().get(provider)),
            client: Box::new(|provider, secret| Ok(Arc::new(DebridClient::new(provider, secret)?))),
            launch: Box::new(|choice, url| player::launch(choice, url).map(Some)),
        }
    }
}

struct Preparation {
    id: u64,
    selected_file_id: String,
    prepared: PreparedTorrent,
    client: Arc<dyn DebridApi>,
}
#[derive(Default)]
struct Application {
    io: PlaybackIo,
    paths: Option<AppPaths>,
    store: Option<SettingsStore>,
    settings: Option<Settings>,
    media: Option<MediaService>,
    prepared: Option<Preparation>,
    sessions: Vec<SessionRecord>,
    children: Vec<std::process::Child>,
}
struct SessionRecord {
    id: String,
    file_name: String,
    player: player::PlayerChoice,
    remote: bool,
    reported_errors: Vec<MediaError>,
}
impl Application {
    fn settings(&mut self) -> Result<Settings, String> {
        if self.paths.is_none() {
            self.paths = Some(AppPaths::resolve().map_err(|e| e.to_string())?);
        }
        if self.store.is_none() {
            self.store = Some(
                SettingsStore::open(self.paths.as_ref().ok_or("Paths unavailable")?)
                    .map_err(|e| e.to_string())?,
            );
        }
        if self.settings.is_none() {
            self.settings = Some(
                self.store
                    .as_ref()
                    .ok_or("Settings unavailable")?
                    .load()
                    .map_err(|e| e.to_string())?,
            );
        }
        self.settings
            .clone()
            .ok_or_else(|| "Settings unavailable".into())
    }
    fn media(&mut self) -> Result<&MediaService, String> {
        let settings = self.settings()?;
        if self.media.is_none() {
            let limit = budget(settings.media_limit_bytes)?;
            self.media = Some(
                MediaService::start(self.paths.as_ref().ok_or("Paths unavailable")?, limit)
                    .map_err(|e| media_message(&e))?,
            );
        }
        self.media
            .as_ref()
            .ok_or_else(|| "Media unavailable".into())
    }
    fn invalidate(&mut self) -> Vec<String> {
        self.prepared = None;
        let mut released = Vec::new();
        release_remote(&mut self.sessions, |id| {
            released.push(id.to_owned());
            if let Some(media) = &self.media {
                let _ = media.release(id);
            }
        });
        released
    }
    fn snapshot(&mut self) -> Result<AppOutcome, String> {
        self.snapshot_warning(None)
    }
    fn snapshot_warning(&mut self, warning: Option<String>) -> Result<AppOutcome, String> {
        let settings = self.settings()?;
        let vault = CredentialStore::new();
        let credentials = [DebridKind::RealDebrid, DebridKind::AllDebrid]
            .into_iter()
            .map(|kind| (kind, credential_state(vault.status(kind))))
            .collect();
        let tmdb_credential = credential_state(MetadataCredentialStore::new().status());
        Ok(match warning {
            Some(message) => AppOutcome::SettingsWarning {
                settings,
                credentials,
                tmdb_credential,
                message,
            },
            None => AppOutcome::Settings {
                settings,
                credentials,
                tmdb_credential,
            },
        })
    }

    #[cfg(test)]
    fn execute(
        &mut self,
        command: AppCommand,
        cancel: Arc<AtomicBool>,
        account: Arc<AtomicBool>,
    ) -> Result<AppOutcome, String> {
        self.execute_checked(command, cancel, account, |_| Ok(()))
    }
    fn execute_checked(
        &mut self,
        command: AppCommand,
        cancel: Arc<AtomicBool>,
        account: Arc<AtomicBool>,
        validate: impl Fn(DebridKind) -> Result<(), String>,
    ) -> Result<AppOutcome, String> {
        let span = tracing::info_span!(
            "application_operation",
            operation = command.action.operation(),
            generation = command.generation,
            provider = tracing::field::Empty
        );
        match &command.action {
            AppAction::SetCredential { provider, .. }
            | AppAction::DeleteCredential(provider)
            | AppAction::TestCredential(provider) => {
                span.record("provider", tracing::field::debug(provider));
            }
            AppAction::SetTmdbCredential { .. }
            | AppAction::DeleteTmdbCredential
            | AppAction::TestTmdbCredential => {
                span.record("provider", "tmdb");
            }
            _ => {}
        }
        let _span = span.entered();
        let started = Instant::now();
        if matches!(
            command.action,
            AppAction::Prepare { .. }
                | AppAction::PlayFile { .. }
                | AppAction::PlayLocal { .. }
                | AppAction::ReplaySession { .. }
        ) {
            tracing::info!("Application preparation or player launch requested");
        }
        let result = (|| {
            check(&cancel)?;
            match command.action {
                AppAction::LoadSettings => {
                    self.settings = None;
                    self.snapshot()
                }
                AppAction::SaveSettings(settings) => {
                    // Open the durable cache even before first playback, so persisted
                    // reservations participate in quota validation after restart.
                    let previous = self.settings()?;
                    let limit = budget(settings.media_limit_bytes)?;
                    validate_quota(
                        settings.media_limit_bytes,
                        self.media()?.usage().reserved_bytes,
                    )?;
                    self.media()?.set_limit(limit).map_err(|e| e.to_string())?;
                    let result = self
                        .store
                        .as_ref()
                        .ok_or("Settings unavailable")?
                        .save(&settings);
                    let warning = apply_save_result(result, settings, &mut self.settings, || {
                        if let (Some(media), Some(limit)) =
                            (&self.media, CacheBudget::new(previous.media_limit_bytes))
                            && let Err(category) = media.set_limit(limit)
                        {
                            tracing::warn!(?category, "Media quota rollback failed");
                        }
                    })?;
                    self.snapshot_warning(warning)
                }
                AppAction::SetCredential { provider, value } => {
                    CredentialStore::new()
                        .set(provider, value)
                        .map_err(|e| e.to_string())?;
                    self.snapshot()
                }
                AppAction::DeleteCredential(provider) => {
                    match CredentialStore::new().delete(provider) {
                        Ok(()) | Err(CredentialError::Missing) => {}
                        Err(e) => return Err(e.to_string()),
                    }
                    self.snapshot()
                }
                AppAction::SetTmdbCredential { value } => {
                    update_metadata_credential(|| MetadataCredentialStore::new().set(value), false)
                }
                AppAction::DeleteTmdbCredential => {
                    update_metadata_credential(|| MetadataCredentialStore::new().delete(), true)
                }
                AppAction::TestTmdbCredential => {
                    test_credential(
                        || MetadataCredentialStore::new().get(),
                        |token| TmdbClient::new(token)?.check_account(),
                        &cancel,
                    )?;
                    Ok(AppOutcome::TmdbCredentialChecked)
                }
                AppAction::TestCredential(provider) => {
                    validate(provider)?;
                    test_credential(
                        || (self.io.credential)(provider),
                        |token| {
                            let client = (self.io.client)(provider, token)?;
                            client.check_account(&cancel)
                        },
                        &cancel,
                    )?;
                    check(&cancel)?;
                    validate(provider)?;
                    Ok(AppOutcome::CredentialChecked { provider })
                }
                AppAction::Prepare { target, candidate } => {
                    self.prepared = None;
                    let settings = self.settings()?;
                    tracing::Span::current()
                        .record("provider", tracing::field::debug(settings.debrid));
                    validate(settings.debrid)?;
                    let token = (self.io.credential)(settings.debrid).map_err(|e| e.to_string())?;
                    check(&cancel)?;
                    check(&account)?;
                    let client =
                        (self.io.client)(settings.debrid, token).map_err(|e| e.to_string())?;
                    let prepared = client
                        .prepare(&candidate.torrent, &cancel)
                        .map_err(|e| e.to_string())?;
                    check(&cancel)?;
                    check(&account)?;
                    validate(settings.debrid)?;
                    let hint = FileHint::from(&candidate.torrent);
                    let (selection, method) = select_file_with_hint(
                        &target,
                        &prepared.files,
                        prepared.provider,
                        Some(&hint),
                    );
                    tracing::debug!(
                        method = method.as_str(),
                        file_count = prepared.files.len(),
                        match_count = match &selection {
                            FileSelection::Unique(_) => 1,
                            FileSelection::Ambiguous(indices) => indices.len(),
                            FileSelection::NotFound => 0,
                        },
                        unique = matches!(selection, FileSelection::Unique(_)),
                        "Torrent file selection completed"
                    );
                    if let FileSelection::Unique(index) = &selection {
                        self.prepared = Some(Preparation {
                            id: command.generation,
                            selected_file_id: prepared.files[*index].id.clone(),
                            prepared: prepared.clone(),
                            client,
                        });
                    }
                    Ok(AppOutcome::Prepared {
                        prepared,
                        target,
                        selection,
                    })
                }
                AppAction::PlayFile {
                    preparation,
                    file_id,
                } => {
                    let current = self
                        .prepared
                        .as_ref()
                        .filter(|p| p.id == preparation)
                        .ok_or("Preparation is obsolete; prepare the source again")?;
                    if current.selected_file_id != file_id {
                        return Err(
                            "Couldn't identify the file in this torrent — try another stream."
                                .into(),
                        );
                    }
                    tracing::debug!("Playing policy-selected torrent file");
                    tracing::Span::current()
                        .record("provider", tracing::field::debug(current.prepared.provider));
                    validate(current.prepared.provider)?;
                    let prepared = current.prepared.clone();
                    let client = current.client.clone();
                    let file = prepared
                        .files
                        .iter()
                        .find(|f| f.id == file_id)
                        .cloned()
                        .ok_or("File no longer exists")?;
                    check(&account)?;
                    let resolved = client
                        .resolve_file(&prepared, &file_id, &cancel)
                        .map_err(|e| e.to_string())?;
                    check(&cancel)?;
                    check(&account)?;
                    validate(prepared.provider)?;
                    let file_name = resolved.file_name.clone();
                    let renew = Arc::new(move || {
                        if account.load(Ordering::Acquire) {
                            return Err(MediaError::Canceled);
                        }
                        match client.resolve_file(&prepared, &file.id, &account) {
                            Ok(resolved) => Ok(resolved),
                            Err(DebridError::FileNotFound | DebridError::Failed) => {
                                let refreshed = client
                                    .prepare(&prepared.source, &account)
                                    .map_err(|_| MediaError::Upstream)?;
                                let matches: Vec<_> = refreshed
                                    .files
                                    .iter()
                                    .filter(|f| f.path == file.path && f.size == file.size)
                                    .collect();
                                if matches.len() != 1 {
                                    return Err(MediaError::IdentityMismatch);
                                }
                                client
                                    .resolve_file(&refreshed, &matches[0].id, &account)
                                    .map_err(|_| MediaError::Upstream)
                            }
                            Err(_) => Err(MediaError::Upstream),
                        }
                    });
                    let session = match self.media()?.register_remote(resolved, renew) {
                        Ok(session) => session,
                        Err(error) => return Ok(media_outcome(error)),
                    };
                    self.launch(session, file_name, &cancel, true)
                }
                AppAction::PlayLocal { path } => {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Local media".into());
                    let session = self
                        .media()?
                        .register_local(&path)
                        .map_err(|e| e.to_string())?;
                    self.launch(session, name, &cancel, false)
                }
                AppAction::ReleaseSession(id) => {
                    if let Some(media) = &self.media {
                        media.release(&id).map_err(|e| e.to_string())?;
                    }
                    self.sessions.retain(|existing| existing.id != id);
                    Ok(AppOutcome::Released { id })
                }
                AppAction::ReplaySession { session_id } => {
                    let session = self
                        .media
                        .as_ref()
                        .ok_or("Media session ended")?
                        .session(&session_id)
                        .map_err(|e| e.to_string())?;
                    check(&cancel)?;
                    // A lease never moves between managed and external players.
                    // Preference changes apply only when creating a new session.
                    let record = self
                        .sessions
                        .iter()
                        .find(|record| record.id == session_id)
                        .ok_or("Media session ended")?;
                    if record.player.is_managed() {
                        return Ok(AppOutcome::EmbeddedPlaybackStarted {
                            session,
                            file_name: record.file_name.clone(),
                            mode: record.player.clone(),
                        });
                    }
                    if let Some(child) =
                        (self.io.launch)(&record.player, &session.url).map_err(|e| e.to_string())?
                    {
                        self.children.push(child);
                    }
                    Ok(AppOutcome::SessionRelaunched { session_id })
                }
                AppAction::RevealSession { session_id } => {
                    let path = self
                        .media
                        .as_ref()
                        .ok_or("Media session ended")?
                        .reveal_path(&session_id)
                        .map_err(|e| e.to_string())?;
                    player::reveal_file(&path)?;
                    Ok(AppOutcome::RevealRequested { session_id })
                }
                AppAction::CleanupUnused => {
                    let report = self
                        .media()?
                        .cleanup_unused()
                        .map_err(|e| media_message(&e))?;
                    Ok(AppOutcome::Cleaned {
                        bytes: report.freed_bytes,
                        protected_files: report.protected_files,
                    })
                }
            }
        })();
        log_operation_result(&result, cancel.load(Ordering::Acquire), started);
        result
    }
    fn launch(
        &mut self,
        session: PlaybackSession,
        file_name: String,
        cancel: &AtomicBool,
        remote: bool,
    ) -> Result<AppOutcome, String> {
        let mut child = None;
        let mut choice = player::PlayerChoice::Embedded;
        let result = (|| {
            check(cancel)?;
            let settings = self.settings()?;
            check(cancel)?;
            // A spawned child is not evidence of playing/completed. Retain it
            // only for nonblocking reaping; never terminate the external player.
            choice = settings.player;
            if !choice.is_managed() {
                child = (self.io.launch)(&choice, &session.url).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        finish_launch(result, &session.id, |id| {
            if let Some(media) = &self.media {
                let _ = media.release(id);
            }
        })?;
        if let Some(child) = child {
            self.children.push(child);
        }
        self.sessions.push(SessionRecord {
            id: session.id.clone(),
            file_name: file_name.clone(),
            player: choice.clone(),
            remote,
            reported_errors: Vec::new(),
        });
        Ok(if choice.is_managed() {
            AppOutcome::EmbeddedPlaybackStarted {
                session,
                file_name,
                mode: choice,
            }
        } else {
            AppOutcome::PlaybackStarted { session, file_name }
        })
    }
}
/// Rename success is effective even if directory sync failed. Only failures
/// before persistence may roll runtime admission back to the previous quota.
fn apply_save_result(
    result: Result<(), SettingsError>,
    proposed: Settings,
    effective: &mut Option<Settings>,
    rollback: impl FnOnce(),
) -> Result<Option<String>, String> {
    let warning = match result {
        Ok(()) => None,
        Err(SettingsError::DurabilityUncertain) => {
            Some(SettingsError::DurabilityUncertain.to_string())
        }
        Err(error) => {
            rollback();
            return Err(error.to_string());
        }
    };
    *effective = Some(proposed);
    Ok(warning)
}

// Independent secure-storage mutation: deliberately has no SettingsStore or
// media-service access, and never turns a vault failure into successful status.
fn update_metadata_credential(
    operation: impl FnOnce() -> Result<(), CredentialError>,
    deleted: bool,
) -> Result<AppOutcome, String> {
    match operation() {
        Ok(()) => {}
        Err(CredentialError::Missing) if deleted => {}
        Err(error) => return Err(error.to_string()),
    }
    Ok(AppOutcome::MetadataCredential {
        status: if deleted {
            CredentialState::Missing
        } else {
            CredentialState::Present
        },
    })
}

fn test_credential<E: std::fmt::Display>(
    get: impl FnOnce() -> Result<SecretString, CredentialError>,
    verify: impl FnOnce(SecretString) -> Result<(), E>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    check(cancel)?;
    let started = Instant::now();
    let token = get().map_err(|category| {
        tracing::warn!(?category, "Credential check could not read secure storage");
        category.to_string()
    })?;
    check(cancel)?;
    verify(token).map_err(|error| {
        tracing::warn!(
            category = "verification_failed",
            duration_ms = started.elapsed().as_millis() as u64,
            "Credential check failed"
        );
        error.to_string()
    })?;
    tracing::debug!(
        duration_ms = started.elapsed().as_millis() as u64,
        "Credential verification completed"
    );
    check(cancel)
}
fn validate_quota(bytes: u64, reserved: u64) -> Result<(), String> {
    let limit = budget(bytes)?;
    if !limit.admits(reserved, 0) {
        return Err("Media limit cannot be lower than reserved usage; release sessions and clean unused media first".into());
    }
    Ok(())
}
fn media_outcome(error: MediaError) -> AppOutcome {
    match error {
        MediaError::FileExceedsLimit { size, limit } => {
            AppOutcome::CacheLimitNeeded { size, limit }
        }
        MediaError::CacheFull => AppOutcome::CacheFull {
            message: media_message(&MediaError::CacheFull),
        },
        MediaError::Busy => AppOutcome::Busy {
            label: "Media service capacity reached; release an inactive session and retry".into(),
        },
        other => failed(media_message(&other)),
    }
}
fn media_message(error: &MediaError) -> String {
    if matches!(error, MediaError::CacheFull) {
        "Media cache or disk is full. Release inactive sessions and clean unused media, increase the cache limit, or free disk space.".into()
    } else {
        error.to_string()
    }
}
fn budget(bytes: u64) -> Result<CacheBudget, String> {
    CacheBudget::new(bytes).ok_or_else(|| "Media limit must be greater than zero".into())
}
fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Operation canceled".into())
    } else {
        Ok(())
    }
}
fn finish_launch(
    result: Result<(), String>,
    id: &str,
    release: impl FnOnce(&str),
) -> Result<(), String> {
    if result.is_err() {
        release(id);
    }
    result
}
fn queue_media_errors(
    sessions: &mut [SessionRecord],
    mail: &mut Mailbox,
    errors: Vec<(String, MediaError)>,
) {
    for (session_id, error) in errors {
        if mail.outstanding >= CAPACITY {
            break;
        }
        if let Some(session) = sessions.iter_mut().find(|s| s.id == session_id)
            && !session.reported_errors.contains(&error)
        {
            let message = media_message(&error);
            session.reported_errors.push(error);
            mail.events.push_back(AppEvent {
                generation: 0,
                outcome: AppOutcome::MediaFailed {
                    session_id,
                    message,
                },
            });
            mail.outstanding += 1;
        }
    }
}
fn release_remote(sessions: &mut Vec<SessionRecord>, mut release: impl FnMut(&str)) {
    sessions.retain(|session| {
        if session.remote {
            release(&session.id);
        }
        !session.remote
    });
}

fn run(shared: Arc<Shared>) {
    run_application(shared, Application::default());
}
fn run_application(shared: Arc<Shared>, mut app: Application) {
    tracing::info!("Application worker started");
    let mut last_tick = Instant::now();
    loop {
        let (request, account, invalidate) = {
            let Ok(mut mail) = shared.mailbox.lock() else {
                return;
            };
            if mail.stopped {
                return;
            }
            if mail.requests.is_empty() {
                let Ok((next, _)) = shared.wake.wait_timeout(mail, TICK) else {
                    return;
                };
                mail = next;
            }
            if mail.stopped {
                return;
            }
            for generation in mail.discarded_preparations.drain(..) {
                if app.prepared.as_ref().is_some_and(|p| p.id == generation) {
                    app.prepared = None;
                }
            }
            mail.prepared_generation = app.prepared.as_ref().map(|preparation| preparation.id);
            let request = mail.requests.pop_front();
            mail.active = request.as_ref().map(|r| r.cancel.clone());
            mail.active_generation = request.as_ref().map(|r| r.command.generation);
            let invalidate = request
                .as_ref()
                .is_some_and(|r| begin_account_action(&mut mail, &r.command.action));
            (request, mail.account.clone(), invalidate)
        };
        let mut invalidated = invalidate;
        if invalidate {
            let released = app.invalidate();
            emit_invalidation(&shared, released);
        }
        if let Some(request) = request {
            let mut account = account;
            let preflight = if matches!(request.command.action, AppAction::Prepare { .. })
                && app.settings.is_none()
                && !request.cancel.load(Ordering::Acquire)
            {
                app.settings().map(|settings| {
                    let changed = {
                        let mut mail = shared.mailbox.lock().unwrap_or_else(|e| e.into_inner());
                        let changed = mail.scope_provider.is_some()
                            && mail.scope_provider != Some(settings.debrid);
                        reconcile_account(&mut mail, settings.debrid);
                        account = mail.account.clone();
                        changed
                    };
                    if changed {
                        let released = app.invalidate();
                        emit_invalidation(&shared, released);
                        invalidated = true;
                    }
                })
            } else {
                Ok(())
            };
            let generation = request.command.generation;
            let result = preflight.and_then(|()| {
                app.execute_checked(
                    request.command,
                    request.cancel.clone(),
                    account,
                    |provider| {
                        let mail = shared
                            .mailbox
                            .lock()
                            .map_err(|_| "Application worker mailbox unavailable".to_string())?;
                        validate_credential_revision(&mail, request.credential_revisions, provider)
                    },
                )
            });
            // Reconcile partial-success or failed writes as well as explicit
            // reloads. Never publish a flag with a different provider identity.
            if let Some(settings) = &app.settings {
                let changed = {
                    let mut mail = shared.mailbox.lock().unwrap_or_else(|e| e.into_inner());
                    let changed = mail.scope_provider.is_some()
                        && mail.scope_provider != Some(settings.debrid);
                    reconcile_account(&mut mail, settings.debrid);
                    changed
                };
                if changed && !invalidated {
                    let released = app.invalidate();
                    emit_invalidation(&shared, released);
                }
            }
            let mut outcome = result.unwrap_or_else(failed);
            if matches!(outcome, AppOutcome::Prepared { .. })
                && request.cancel.load(Ordering::Acquire)
            {
                app.prepared = None;
                tracing::debug!(
                    generation,
                    outcome = "superseded",
                    "Application preparation discarded"
                );
                outcome = failed("Preparation superseded");
            }
            if let Ok(mut mail) = shared.mailbox.lock() {
                if mail.stopped {
                    return;
                }
                mail.active = None;
                mail.active_generation = None;
                mail.prepared_generation = app.prepared.as_ref().map(|preparation| preparation.id);
                mail.pipeline
                    .retain(|flag| !Arc::ptr_eq(flag, &request.cancel));
                mail.metadata_pipeline
                    .retain(|flag| !Arc::ptr_eq(flag, &request.cancel));
                mail.events.push_back(AppEvent {
                    generation,
                    outcome,
                });
            }
        }
        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            // Reap exited children without translating OS process status into
            // claims about media playing or completion. Never kill a player.
            app.children
                .retain_mut(|child| match child.try_wait() {
                    Ok(None) => true,
                    Ok(Some(status)) => { tracing::debug!(success = status.success(), "External player process exited"); false }
                    Err(error) => { tracing::warn!(io_kind = ?error.kind(), "External player process status unavailable"); false }
                });
            if let Some(media) = &app.media {
                let usage = media.usage();
                let errors = media.session_errors();
                let updates = media.session_updates();
                if let Ok(mut mail) = shared.mailbox.lock() {
                    if mail.stopped {
                        return;
                    }
                    queue_media_errors(&mut app.sessions, &mut mail, errors);
                    // One lossy telemetry slot; terminal outcomes never evicted.
                    mail.events.retain(|e| {
                        !matches!(
                            e.outcome,
                            AppOutcome::Usage(_) | AppOutcome::SessionUpdates(_)
                        )
                    });
                    mail.events.push_back(AppEvent {
                        generation: 0,
                        outcome: AppOutcome::Usage(usage),
                    });
                    mail.events.push_back(AppEvent {
                        generation: 0,
                        outcome: AppOutcome::SessionUpdates(updates),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_diagnostics_exclude_debrid_tokens_and_playback_urls() {
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "app_diagnostics_exclude_debrid_tokens_and_playback_urls",
        ) {
            return;
        }

        use crate::providers::test_logging::{capture, excludes};
        let mut app = Application::default();
        app.io.credential = Box::new(|_| Ok(SecretString::new("app-debrid-secret".into())));
        app.io.client = Box::new(|_, token| {
            assert_eq!(token.expose(), "app-debrid-secret");
            Err(DebridError::CredentialRejected)
        });
        let (result, output) = capture(|| {
            let result = app.execute(
                AppCommand {
                    generation: 1,
                    action: AppAction::TestCredential(DebridKind::RealDebrid),
                },
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
            );
            log_operation_result(
                &Ok(AppOutcome::PlaybackStarted {
                    session: PlaybackSession {
                        id: "lease-secret".into(),
                        url: "http://127.0.0.1/media/lease-secret?token=playback-secret".into(),
                    },
                    file_name: "filename-secret".into(),
                }),
                false,
                Instant::now(),
            );
            assert!(
                test_credential(
                    || Ok(SecretString::new("verifier-token-secret".into())),
                    |_| Err("https://cdn.invalid/?signature=error-secret"),
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            result
        });
        assert!(result.is_err());
        assert!(output.contains("Credential check failed"));
        excludes(
            &output,
            &[
                "app-debrid-secret",
                "lease-secret",
                "playback-secret",
                "filename-secret",
                "verifier-token-secret",
                "error-secret",
                "signature=",
            ],
        );
    }

    #[test]
    fn credential_failures_are_not_missing() {
        assert_eq!(
            credential_state(Err(CredentialError::Missing)),
            CredentialState::Missing
        );
        assert!(matches!(
            credential_state(Err(CredentialError::Unavailable)),
            CredentialState::Unavailable(_)
        ));
        assert!(matches!(
            credential_state(Err(CredentialError::AccessDenied)),
            CredentialState::Unavailable(_)
        ));
    }
    #[test]
    fn launch_failure_releases_exact_session() {
        let mut released = String::new();
        assert!(
            finish_launch(Err("spawn failed".into()), "lease", |id| released =
                id.into())
            .is_err()
        );
        assert_eq!(released, "lease");
        assert!(
            finish_launch(Ok(()), "retained", |_| panic!(
                "must retain successful lease"
            ))
            .is_ok()
        );
    }
    #[test]
    fn obsolete_preparation_fails_before_io() {
        let mut app = Application::default();
        let result = app.execute(
            AppCommand {
                generation: 2,
                action: AppAction::PlayFile {
                    preparation: 1,
                    file_id: "x".into(),
                },
            },
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(result.is_err_and(|e| e.contains("obsolete")));
        assert!(app.paths.is_none());
    }
    #[test]
    fn canceled_request_does_not_initialize_services() {
        let mut app = Application::default();
        assert!(
            app.execute(
                AppCommand {
                    generation: 1,
                    action: AppAction::LoadSettings
                },
                Arc::new(AtomicBool::new(true)),
                Arc::new(AtomicBool::new(false))
            )
            .is_err()
        );
        assert!(app.paths.is_none());
    }
    #[test]
    fn quota_save_validation_includes_reserved_not_just_downloaded() {
        assert!(validate_quota(0, 0).is_err());
        assert!(validate_quota(99, 100).is_err());
        assert!(validate_quota(100, 100).is_ok());
        assert!(validate_quota(101, 100).is_ok());
    }
    #[test]
    fn account_check_distinguishes_vault_failure_and_rejected_account() {
        let cancel = AtomicBool::new(false);
        let missing = test_credential::<DebridError>(
            || Err(CredentialError::Missing),
            |_| panic!("no network without token"),
            &cancel,
        )
        .err()
        .unwrap_or_default();
        let unavailable = test_credential::<DebridError>(
            || Err(CredentialError::Unavailable),
            |_| panic!("no network without token"),
            &cancel,
        )
        .err()
        .unwrap_or_default();
        let rejected = test_credential::<DebridError>(
            || Ok(SecretString::new("private".into())),
            |_| Err(DebridError::CredentialRejected),
            &cancel,
        )
        .err()
        .unwrap_or_default();
        assert_ne!(missing, unavailable);
        assert_ne!(missing, rejected);
        assert!(!rejected.contains("private"));
        assert!(
            test_credential::<DebridError>(
                || Ok(SecretString::new("private".into())),
                |_| Ok(()),
                &cancel
            )
            .is_ok()
        );
    }
    fn unstarted_worker() -> AppWorker {
        AppWorker {
            shared: new_shared(),
            thread: None,
        }
    }
    #[test]
    fn coalesced_terminals_are_bounded_and_apply_backpressure() {
        let worker = unstarted_worker();
        for generation in 0..CAPACITY as u64 {
            assert!(worker.submit(AppCommand {
                generation,
                action: AppAction::LoadSettings
            }));
        }
        assert!(!worker.submit(AppCommand {
            generation: 99,
            action: AppAction::LoadSettings
        }));
        {
            let mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            assert_eq!(mail.requests.len(), 1);
            assert_eq!(mail.events.len(), CAPACITY - 1);
        }
        for generation in 0..(CAPACITY - 1) as u64 {
            assert_eq!(worker.poll().map(|e| e.generation), Some(generation));
        }
        assert!(worker.submit(AppCommand {
            generation: 100,
            action: AppAction::LoadSettings
        }));
    }
    #[test]
    fn credential_change_and_drop_cancel_inflight_without_join() {
        let worker = unstarted_worker();
        let cancel = Arc::new(AtomicBool::new(false));
        let account;
        {
            let mut mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            mail.pipeline.push(cancel.clone());
            mail.active = Some(cancel.clone());
            mail.scope_provider = Some(DebridKind::RealDebrid);
            account = mail.account.clone();
        }
        assert!(worker.submit(AppCommand {
            generation: 1,
            action: AppAction::DeleteCredential(DebridKind::RealDebrid)
        }));
        assert!(cancel.load(Ordering::Acquire));
        assert!(account.load(Ordering::Acquire));
        cancel.store(false, Ordering::Release);
        let shared = worker.shared.clone();
        drop(worker);
        assert!(cancel.load(Ordering::Acquire));
        assert!(
            shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .stopped
        );
    }
    #[test]
    fn account_changes_preserve_local_sessions() {
        let mut sessions = vec![
            SessionRecord {
                id: "remote".into(),
                file_name: "remote.mkv".into(),
                player: player::PlayerChoice::Mpv,
                remote: true,
                reported_errors: Vec::new(),
            },
            SessionRecord {
                id: "local".into(),
                file_name: "local.mkv".into(),
                player: player::PlayerChoice::Mpv,
                remote: false,
                reported_errors: Vec::new(),
            },
        ];
        let mut released = Vec::new();
        release_remote(&mut sessions, |id| released.push(id.to_string()));
        assert_eq!(released, ["remote"]);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "local");
        let settings = Settings::default();
        assert!(!invalidates_account(
            &AppAction::SaveSettings(settings.clone()),
            Some(settings.debrid)
        ));
    }
    #[test]
    fn acquisition_errors_deduplicate_and_wait_for_event_capacity() {
        let worker = unstarted_worker();
        let mut sessions = vec![SessionRecord {
            id: "remote".into(),
            file_name: "remote.mkv".into(),
            player: player::PlayerChoice::Mpv,
            remote: true,
            reported_errors: Vec::new(),
        }];
        let mut mail = worker
            .shared
            .mailbox
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        mail.outstanding = CAPACITY;
        queue_media_errors(
            &mut sessions,
            &mut mail,
            vec![("remote".into(), MediaError::Upstream)],
        );
        assert!(sessions[0].reported_errors.is_empty());
        mail.outstanding = 0;
        for error in [
            MediaError::Upstream,
            MediaError::CacheFull,
            MediaError::Upstream,
        ] {
            queue_media_errors(&mut sessions, &mut mail, vec![("remote".into(), error)]);
        }
        assert_eq!(mail.events.len(), 2);
        assert_eq!(mail.outstanding, 2);
    }
    #[test]
    fn running_worker_drops_with_unpolled_terminal_events() -> io::Result<()> {
        let worker = AppWorker::spawn()?;
        let weak = Arc::downgrade(&worker.shared);
        for generation in 0..CAPACITY as u64 {
            assert!(worker.submit(AppCommand {
                generation,
                action: AppAction::ReleaseSession(format!("lease-{generation}"))
            }));
        }
        assert!(!worker.submit(AppCommand {
            generation: 99,
            action: AppAction::CleanupUnused
        }));
        drop(worker);
        let deadline = Instant::now() + Duration::from_secs(2);
        while weak.upgrade().is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            weak.upgrade().is_none(),
            "worker must not block waiting to deliver events"
        );
        Ok(())
    }

    #[test]
    fn oversized_file_requires_explicit_user_quota_choice() {
        assert!(matches!(
            media_outcome(MediaError::FileExceedsLimit {
                size: 200,
                limit: 100
            }),
            AppOutcome::CacheLimitNeeded {
                size: 200,
                limit: 100
            }
        ));
        assert!(media_message(&MediaError::CacheFull).contains("clean unused"));
        assert!(
            matches!(media_outcome(MediaError::CacheFull), AppOutcome::CacheFull { message } if message.contains("clean unused"))
        );
    }
    #[test]
    fn inactive_credentials_do_not_invalidate_active_provider() {
        let worker = unstarted_worker();
        let cancel = Arc::new(AtomicBool::new(false));
        let account;
        {
            let mut mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            mail.scope_provider = Some(DebridKind::RealDebrid);
            mail.pipeline.push(cancel.clone());
            account = mail.account.clone();
        }
        assert!(worker.submit(AppCommand {
            generation: 1,
            action: AppAction::DeleteCredential(DebridKind::AllDebrid)
        }));
        assert!(!cancel.load(Ordering::Acquire));
        assert!(!account.load(Ordering::Acquire));
        assert!(worker.submit(AppCommand {
            generation: 2,
            action: AppAction::DeleteCredential(DebridKind::RealDebrid)
        }));
        assert!(cancel.load(Ordering::Acquire));
        assert!(account.load(Ordering::Acquire));
    }
    #[test]
    fn invalidation_controls_do_not_consume_command_budget() {
        let worker = unstarted_worker();
        {
            let mut mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            mail.outstanding = CAPACITY;
        }
        for generation in 0..CAPACITY as u64 {
            emit_invalidation(&worker.shared, vec![format!("released-{generation}")]);
            worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .events
                .push_back(AppEvent {
                    generation,
                    outcome: failed("mutation failed"),
                });
        }
        assert!(!worker.submit(AppCommand {
            generation: 99,
            action: AppAction::LoadSettings
        }));
        for generation in 0..CAPACITY as u64 {
            assert!(
                matches!(worker.poll().map(|e| e.outcome), Some(AppOutcome::Invalidated { released }) if released == [format!("released-{generation}")])
            );
            assert_eq!(worker.poll().map(|e| e.generation), Some(generation));
        }
        assert!(worker.poll().is_none());
        assert_eq!(
            worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .outstanding,
            0
        );
    }
    #[test]
    fn panicked_worker_closes_and_completes_accepted_requests_once() -> io::Result<()> {
        let worker = AppWorker::spawn_task(|shared| {
            let mut mail = shared.mailbox.lock().unwrap_or_else(|e| e.into_inner());
            while mail.requests.len() < CAPACITY {
                mail = shared.wake.wait(mail).unwrap_or_else(|e| e.into_inner());
            }
            let request = mail.requests.pop_front().unwrap_or_else(|| unreachable!());
            mail.active = Some(request.cancel);
            mail.active_generation = Some(request.command.generation);
            panic!("injected application failure while mailbox locked");
        })?;
        for generation in 0..CAPACITY as u64 {
            assert!(worker.submit(AppCommand {
                generation,
                action: AppAction::ReleaseSession("lease".into())
            }));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.is_stopped() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(worker.is_stopped());
        assert!(worker.thread.is_some());
        for generation in 0..CAPACITY as u64 {
            let event = worker.poll();
            assert!(
                matches!(event, Some(AppEvent { generation: actual, outcome: AppOutcome::Failed { .. } }) if actual == generation)
            );
        }
        assert!(worker.poll().is_none());
        assert!(!worker.submit(AppCommand {
            generation: 99,
            action: AppAction::LoadSettings
        }));
        Ok(())
    }
    #[test]
    fn postrename_failure_keeps_written_settings_and_quota() -> Result<(), String> {
        let previous = Settings::default();
        let mut effective = Some(previous.clone());
        let proposed = Settings {
            media_limit_bytes: previous.media_limit_bytes * 2,
            ..previous.clone()
        };
        let mut quota = proposed.media_limit_bytes;
        let warning = apply_save_result(
            Err(SettingsError::DurabilityUncertain),
            proposed.clone(),
            &mut effective,
            || quota = previous.media_limit_bytes,
        )?;
        assert!(warning.is_some());
        assert_eq!(quota, proposed.media_limit_bytes);
        assert_eq!(
            effective.as_ref().map(|s| s.media_limit_bytes),
            Some(proposed.media_limit_bytes)
        );
        let mut rolled_back = false;
        let failure = apply_save_result(
            Err(SettingsError::Io),
            previous.clone(),
            &mut effective,
            || {
                quota = proposed.media_limit_bytes;
                rolled_back = true;
            },
        );
        assert!(failure.is_err());
        assert!(rolled_back);
        assert_eq!(
            effective.as_ref().map(|s| s.media_limit_bytes),
            Some(proposed.media_limit_bytes)
        );
        Ok(())
    }
    #[test]
    fn metadata_credentials_have_independent_cancellation_scope() {
        let worker = unstarted_worker();
        let debrid = Arc::new(AtomicBool::new(false));
        let metadata = Arc::new(AtomicBool::new(false));
        let account;
        {
            let mut mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            mail.scope_provider = Some(DebridKind::RealDebrid);
            mail.pipeline.push(debrid.clone());
            mail.metadata_pipeline.push(metadata.clone());
            account = mail.account.clone();
        }
        assert!(worker.submit(AppCommand {
            generation: 1,
            action: AppAction::SetTmdbCredential {
                value: SecretString::new("tmdb-test-token".into())
            }
        }));
        assert!(metadata.load(Ordering::Acquire));
        assert!(!debrid.load(Ordering::Acquire));
        assert!(!account.load(Ordering::Acquire));
        assert!(!invalidates_account(
            &AppAction::DeleteTmdbCredential,
            Some(DebridKind::RealDebrid)
        ));
        assert!(!pipeline(&AppAction::TestTmdbCredential));
        let mut mail = worker
            .shared
            .mailbox
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let new_metadata = Arc::new(AtomicBool::new(false));
        mail.metadata_pipeline.push(new_metadata.clone());
        drop(mail);
        drop(worker);
        assert!(new_metadata.load(Ordering::Acquire));
    }
    #[test]
    fn metadata_vault_missing_unavailable_and_rejection_are_distinct() {
        use crate::tmdb::TmdbError;
        let cancel = AtomicBool::new(false);
        let missing = test_credential::<TmdbError>(
            || Err(CredentialError::Missing),
            |_| panic!("must not authenticate without token"),
            &cancel,
        )
        .err();
        let unavailable = test_credential::<TmdbError>(
            || Err(CredentialError::Unavailable),
            |_| panic!("must not authenticate without token"),
            &cancel,
        )
        .err();
        let rejected = test_credential::<TmdbError>(
            || Ok(SecretString::new("tmdb-test-token".into())),
            |_| Err(TmdbError::RejectedCredential),
            &cancel,
        )
        .err();
        assert_eq!(missing, Some(CredentialError::Missing.to_string()));
        assert_eq!(unavailable, Some(CredentialError::Unavailable.to_string()));
        assert_ne!(missing, unavailable);
        assert_ne!(missing, rejected);
        assert!(matches!(
            credential_state(Err(CredentialError::Unavailable)),
            CredentialState::Unavailable(_)
        ));
        assert!(update_metadata_credential(|| Err(CredentialError::Unavailable), false).is_err());
        assert!(update_metadata_credential(|| Err(CredentialError::Missing), false).is_err());
        assert!(matches!(
            update_metadata_credential(|| Err(CredentialError::Missing), true),
            Ok(AppOutcome::MetadataCredential {
                status: CredentialState::Missing
            })
        ));
    }
    #[test]
    fn metadata_token_mutation_never_writes_ordinary_settings()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let paths = AppPaths {
            config: root.path().into(),
            data: root.path().into(),
            cache: root.path().into(),
        };
        let store = SettingsStore::open(&paths)?;
        store.save(&Settings::default())?;
        let before = std::fs::read(root.path().join("settings.toml"))?;
        let mut secure_value = None;
        let secret = SecretString::new("tmdb-test-token-never-in-settings".into());
        let outcome = update_metadata_credential(
            || {
                secure_value = Some(secret);
                Ok(())
            },
            false,
        )?;
        assert!(matches!(
            outcome,
            AppOutcome::MetadataCredential {
                status: CredentialState::Present
            }
        ));
        assert!(secure_value.is_some());
        let after = std::fs::read(root.path().join("settings.toml"))?;
        assert_eq!(before, after);
        assert!(!String::from_utf8_lossy(&after).contains("tmdb-test-token"));
        assert!(matches!(
            update_metadata_credential(
                || {
                    secure_value = None;
                    Ok(())
                },
                true
            ),
            Ok(AppOutcome::MetadataCredential {
                status: CredentialState::Missing
            })
        ));
        assert!(secure_value.is_none());
        assert_eq!(after, std::fs::read(root.path().join("settings.toml"))?);
        Ok(())
    }
    #[test]
    fn save_switch_and_concurrent_old_provider_edit_do_not_poison_new_epoch() {
        let worker = unstarted_worker();
        let rd;
        {
            let mut mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            rotate_account(&mut mail, Some(DebridKind::RealDebrid));
            rd = mail.account.clone();
        }
        let settings = Settings {
            debrid: DebridKind::AllDebrid,
            ..Settings::default()
        };
        assert!(worker.submit(AppCommand {
            generation: 1,
            action: AppAction::SaveSettings(settings)
        }));
        let ad;
        let epoch;
        {
            // Pause exactly after dequeue installs the AD epoch but before the
            // settings write completes, reproducing the reported interleaving.
            let mut mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let request = mail.requests.pop_front().unwrap_or_else(|| unreachable!());
            assert!(begin_account_action(&mut mail, &request.command.action));
            ad = mail.account.clone();
            epoch = mail.account_epoch;
            assert_eq!(mail.scope_provider, Some(DebridKind::AllDebrid));
        }
        assert!(rd.load(Ordering::Acquire));
        assert!(worker.submit(AppCommand {
            generation: 2,
            action: AppAction::DeleteCredential(DebridKind::RealDebrid)
        }));
        assert!(!ad.load(Ordering::Acquire));
        {
            let mut mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            reconcile_account(&mut mail, DebridKind::AllDebrid); // save commits
            let request = mail.requests.pop_front().unwrap_or_else(|| unreachable!());
            assert!(!begin_account_action(&mut mail, &request.command.action));
            assert_eq!(mail.account_epoch, epoch);
            assert!(!mail.account.load(Ordering::Acquire));
            assert!(Arc::ptr_eq(&mail.account, &ad));
        }
    }
    #[test]
    fn queued_future_provider_work_rejects_superseded_credentials() {
        let worker = unstarted_worker();
        let revisions;
        {
            let mut mail = worker
                .shared
                .mailbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            rotate_account(&mut mail, Some(DebridKind::RealDebrid));
            revisions = mail.credential_revisions;
        }
        // This AD mutation is inactive now, but may precede execution of a
        // prepare queued behind a switch to AD. Its revision must still count.
        assert!(worker.submit(AppCommand {
            generation: 1,
            action: AppAction::DeleteCredential(DebridKind::AllDebrid)
        }));
        let mut mail = worker
            .shared
            .mailbox
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(validate_credential_revision(&mail, revisions, DebridKind::RealDebrid).is_ok());
        let old_scope = mail.account.clone();
        assert!(!old_scope.load(Ordering::Acquire));
        rotate_account(&mut mail, Some(DebridKind::AllDebrid));
        assert!(old_scope.load(Ordering::Acquire));
        assert!(!mail.account.load(Ordering::Acquire));
        assert!(validate_credential_revision(&mail, revisions, DebridKind::AllDebrid).is_err());
        // A failed switch restores an uncanceled scope for effective RD.
        reconcile_account(&mut mail, DebridKind::RealDebrid);
        assert!(!mail.account.load(Ordering::Acquire));
    }

    struct HttpFixture {
        url: String,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl HttpFixture {
        fn start() -> io::Result<Self> {
            use std::io::{Read, Write};
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            let url = format!("http://{}/upstream-only", listener.local_addr()?);
            listener.set_nonblocking(true)?;
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let thread = std::thread::spawn(move || {
                while !stopped.load(Ordering::Acquire) {
                    let mut stream = match listener.accept() {
                        Ok((stream, _)) => stream,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => break,
                    };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while request.len() < 8192 && !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => request.extend_from_slice(&buffer[..n]),
                        }
                    }
                    let request = String::from_utf8_lossy(&request).to_ascii_lowercase();
                    let response = if request.contains("range: bytes=0-7") {
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: 8\r\nContent-Range: bytes 0-7/8\r\nConnection: close\r\n\r\n12345678"
                    } else {
                        "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    };
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            Ok(Self {
                url,
                stop,
                thread: Some(thread),
            })
        }
    }
    impl Drop for HttpFixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
    #[derive(Default)]
    struct PlaybackTrace {
        clients: std::sync::atomic::AtomicUsize,
        resolved: Mutex<Vec<String>>,
        launched: Mutex<Vec<String>>,
    }
    struct FixtureDebrid {
        upstream: String,
        trace: Arc<PlaybackTrace>,
    }
    impl DebridApi for FixtureDebrid {
        fn prepare(
            &self,
            source: &kinema_core::sources::TorrentSource,
            cancel: &AtomicBool,
        ) -> Result<PreparedTorrent, DebridError> {
            if cancel.load(Ordering::Acquire) {
                return Err(DebridError::Canceled);
            }
            Ok(PreparedTorrent {
                provider: DebridKind::RealDebrid,
                source: source.clone(),
                job_id: "fixture-job".into(),
                files: vec![
                    kinema_core::sources::RemoteFile {
                        id: "episode-1".into(),
                        path: "Show.S01E01.mkv".into(),
                        size: 8,
                    },
                    kinema_core::sources::RemoteFile {
                        id: "episode-2".into(),
                        path: "Show.S01E02.mkv".into(),
                        size: 8,
                    },
                ],
            })
        }
        fn resolve_file(
            &self,
            prepared: &PreparedTorrent,
            file_id: &str,
            cancel: &AtomicBool,
        ) -> Result<ResolvedMedia, DebridError> {
            if cancel.load(Ordering::Acquire) {
                return Err(DebridError::Canceled);
            }
            let file = prepared
                .files
                .iter()
                .find(|f| f.id == file_id)
                .ok_or(DebridError::FileNotFound)?;
            self.trace
                .resolved
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(file_id.into());
            Ok(ResolvedMedia {
                identity: kinema_core::sources::MediaIdentity {
                    torrent: prepared.source.hash.clone(),
                    file_path: file.path.clone(),
                    size: file.size,
                },
                file_name: file.path.clone(),
                url: crate::providers::SensitiveUrl::for_test(
                    reqwest::Url::parse(&self.upstream).map_err(|_| DebridError::InvalidUrl)?,
                ),
                expires_at: None,
            })
        }
        fn check_account(&self, _: &AtomicBool) -> Result<(), DebridError> {
            Ok(())
        }
    }
    fn composed_worker(
        root: &std::path::Path,
        upstream: String,
        trace: Arc<PlaybackTrace>,
        missing: bool,
        launch_fails: bool,
    ) -> io::Result<AppWorker> {
        let provider_trace = trace.clone();
        let launch_trace = trace;
        let io = PlaybackIo {
            credential: Box::new(move |_| {
                if missing {
                    Err(CredentialError::Missing)
                } else {
                    Ok(SecretString::new("fixture-token".into()))
                }
            }),
            client: Box::new(move |_, _secret| {
                provider_trace.clients.fetch_add(1, Ordering::Relaxed);
                Ok(Arc::new(FixtureDebrid {
                    upstream: upstream.clone(),
                    trace: provider_trace.clone(),
                }))
            }),
            launch: Box::new(move |_, url| {
                launch_trace
                    .launched
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(url.into());
                if launch_fails {
                    Err(player::PlayerError::SpawnFailed)
                } else {
                    Ok(None)
                }
            }),
        };
        let app = Application {
            io,
            paths: Some(AppPaths {
                config: root.into(),
                data: root.into(),
                cache: root.into(),
            }),
            settings: Some(Settings {
                media_limit_bytes: 1024,
                player: player::PlayerChoice::Mpv,
                ..Settings::default()
            }),
            ..Application::default()
        };
        AppWorker::spawn_task(move |shared| {
            {
                let mut mail = shared.mailbox.lock().unwrap_or_else(|e| e.into_inner());
                reconcile_account(&mut mail, DebridKind::RealDebrid);
            }
            run_application(shared, app);
        })
    }
    fn episode(number: u32) -> Result<PlaybackTarget, Box<dyn std::error::Error>> {
        Ok(PlaybackTarget::Episode {
            series: kinema_core::catalog::ImdbId::new("tt1234567")?,
            season: 1,
            episode: number,
        })
    }
    fn fixture_candidate() -> Result<SourceCandidate, Box<dyn std::error::Error>> {
        Ok(SourceCandidate {
            torrent: kinema_core::sources::TorrentSource {
                hash: kinema_core::sources::InfoHash::new(
                    "0123456789012345678901234567890123456789",
                )?,
                file_index: None,
                filename: None,
            },
            name: "Fixture show".into(),
            quality: None,
            size: Some(16),
            seeders: None,
        })
    }
    fn command_result(
        worker: &AppWorker,
        generation: u64,
        action: AppAction,
    ) -> Result<AppOutcome, Box<dyn std::error::Error>> {
        if !worker.submit(AppCommand { generation, action }) {
            return Err("fixture submission rejected".into());
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(event) = worker.poll() {
                if event.generation == generation {
                    return Ok(event.outcome);
                }
                if let AppOutcome::MediaFailed { message, .. } = event.outcome {
                    return Err(message.into());
                }
            }
            if worker.is_stopped() {
                return Err("fixture worker stopped".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err("fixture worker timed out".into())
    }
    #[test]
    fn composition_nonunique_preparation_reports_error_and_cannot_play()
    -> Result<(), Box<dyn std::error::Error>> {
        let upstream = HttpFixture::start()?;
        let root = tempfile::tempdir()?;
        let trace = Arc::new(PlaybackTrace::default());
        let worker = composed_worker(
            root.path(),
            upstream.url.clone(),
            trace.clone(),
            false,
            false,
        )?;
        for (generation, target, ambiguous) in [
            (
                1,
                PlaybackTarget::Movie(episode(1)?.imdb_id().clone()),
                true,
            ),
            (3, episode(99)?, false),
        ] {
            let outcome = command_result(
                &worker,
                generation,
                AppAction::Prepare {
                    target,
                    candidate: fixture_candidate()?,
                },
            )?;
            let AppOutcome::Prepared { selection, .. } = outcome else {
                return Err("expected preparation outcome".into());
            };
            assert_eq!(matches!(selection, FileSelection::Ambiguous(_)), ambiguous);
            assert_eq!(
                selection.error_message(),
                Some("Couldn't identify the file in this torrent — try another stream.")
            );
            assert!(matches!(
                command_result(
                    &worker,
                    generation + 1,
                    AppAction::PlayFile {
                        preparation: generation,
                        file_id: "episode-2".into(),
                    }
                )?,
                AppOutcome::Failed { .. }
            ));
        }
        assert!(
            trace
                .resolved
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
        assert!(
            trace
                .launched
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn composition_filename_hint_selects_pack_file_and_rejects_other_file()
    -> Result<(), Box<dyn std::error::Error>> {
        let upstream = HttpFixture::start()?;
        let root = tempfile::tempdir()?;
        let trace = Arc::new(PlaybackTrace::default());
        let worker = composed_worker(
            root.path(),
            upstream.url.clone(),
            trace.clone(),
            false,
            false,
        )?;
        let mut candidate = fixture_candidate()?;
        candidate.torrent.filename = Some("release/Show.S01E02.mkv".into());
        let outcome = command_result(
            &worker,
            1,
            AppAction::Prepare {
                target: PlaybackTarget::Movie(episode(1)?.imdb_id().clone()),
                candidate,
            },
        )?;
        assert!(matches!(
            outcome,
            AppOutcome::Prepared {
                selection: FileSelection::Unique(1),
                ..
            }
        ));
        assert!(matches!(command_result(&worker, 2, AppAction::PlayFile {
            preparation: 1, file_id: "episode-1".into(),
        })?, AppOutcome::Failed { message }
            if message == "Couldn't identify the file in this torrent — try another stream."));
        assert!(
            trace
                .resolved
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn composition_explicit_episode_plays_only_loopback_and_survives_new_navigation()
    -> Result<(), Box<dyn std::error::Error>> {
        let upstream = HttpFixture::start()?;
        let root = tempfile::tempdir()?;
        let trace = Arc::new(PlaybackTrace::default());
        let worker = composed_worker(
            root.path(),
            upstream.url.clone(),
            trace.clone(),
            false,
            false,
        )?;
        let prepared = command_result(
            &worker,
            1,
            AppAction::Prepare {
                target: episode(2)?,
                candidate: fixture_candidate()?,
            },
        )?;
        assert!(matches!(
            prepared,
            AppOutcome::Prepared {
                selection: FileSelection::Unique(1),
                ..
            }
        ));
        assert!(
            trace
                .resolved
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
        assert!(
            trace
                .launched
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
        let started = command_result(
            &worker,
            2,
            AppAction::PlayFile {
                preparation: 1,
                file_id: "episode-2".into(),
            },
        )?;
        let AppOutcome::PlaybackStarted { session, file_name } = started else {
            return Err("expected playback launch".into());
        };
        assert_eq!(file_name, "Show.S01E02.mkv");
        assert_eq!(
            *trace.resolved.lock().unwrap_or_else(|e| e.into_inner()),
            ["episode-2"]
        );
        assert_eq!(
            trace
                .launched
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_slice(),
            std::slice::from_ref(&session.url)
        );
        assert_ne!(session.url, upstream.url);
        assert!(session.url.starts_with("http://127.0.0.1:"));
        assert!(session.url.contains("/media/"));
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?;
        assert_eq!(
            client
                .get(&session.url)
                .send()?
                .error_for_status()?
                .bytes()?
                .as_ref(),
            b"12345678"
        );
        // A new screen/source preparation must not release the independent lease.
        assert!(matches!(
            command_result(
                &worker,
                3,
                AppAction::Prepare {
                    target: episode(3)?,
                    candidate: fixture_candidate()?
                }
            )?,
            AppOutcome::Prepared { .. }
        ));
        assert_eq!(
            client
                .get(&session.url)
                .send()?
                .error_for_status()?
                .bytes()?
                .as_ref(),
            b"12345678"
        );
        assert!(matches!(
            command_result(&worker, 4, AppAction::ReleaseSession(session.id))?,
            AppOutcome::Released { .. }
        ));
        assert_eq!(client.get(&session.url).send()?.status(), 404);
        Ok(())
    }
    #[test]
    fn composition_launch_failure_releases_real_media_lease()
    -> Result<(), Box<dyn std::error::Error>> {
        let upstream = HttpFixture::start()?;
        let root = tempfile::tempdir()?;
        let trace = Arc::new(PlaybackTrace::default());
        let worker = composed_worker(
            root.path(),
            upstream.url.clone(),
            trace.clone(),
            false,
            true,
        )?;
        assert!(matches!(
            command_result(
                &worker,
                1,
                AppAction::Prepare {
                    target: episode(2)?,
                    candidate: fixture_candidate()?
                }
            )?,
            AppOutcome::Prepared { .. }
        ));
        assert!(matches!(
            command_result(
                &worker,
                2,
                AppAction::PlayFile {
                    preparation: 1,
                    file_id: "episode-2".into()
                }
            )?,
            AppOutcome::Failed { .. }
        ));
        let urls = trace
            .launched
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        assert_eq!(urls.len(), 1);
        assert_ne!(urls[0], upstream.url);
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?
            .get(&urls[0])
            .send()?;
        assert_eq!(response.status(), 404);
        Ok(())
    }
    #[test]
    fn composition_missing_credential_blocks_provider_and_player()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let trace = Arc::new(PlaybackTrace::default());
        let worker = composed_worker(
            root.path(),
            "http://127.0.0.1:1/not-used".into(),
            trace.clone(),
            true,
            false,
        )?;
        assert!(
            matches!(command_result(&worker, 1, AppAction::Prepare { target: episode(2)?, candidate: fixture_candidate()? })?, AppOutcome::Failed { message } if message == CredentialError::Missing.to_string())
        );
        assert_eq!(trace.clients.load(Ordering::Relaxed), 0);
        assert!(
            trace
                .resolved
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
        assert!(
            trace
                .launched
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
        Ok(())
    }
    mod sessions;
}
