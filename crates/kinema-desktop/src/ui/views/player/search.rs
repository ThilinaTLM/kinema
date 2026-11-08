//! Online subtitle search: the sheet over the video and its background work.
//!
//! Playback keeps running behind the sheet. All keyring, network, file and
//! hashing work runs on the background executor with a cancel flag; results are
//! applied only when their generation is still current (see `SearchModel`).
//! Searches run only on request, because every download spends allowance.
//! Logs carry categories and counts, never queries, titles, names, keys,
//! tokens or links.
use super::search_model::{
    Allowance, Phase, Problem, ProblemAction, SearchModel, build_query, filter_labels,
    result_badges, result_detail, searchable_by_id, subtitle_lang, subtitle_title,
};
use super::*;
use crate::{
    credentials::{CredentialError, OpenSubtitlesCredentialStore, opensubtitles_generation},
    paths::AppPaths,
    providers::opensubtitles::{
        DownloadedSubtitle, OpenSubtitlesClient, OpenSubtitlesError, Session, default_user_agent,
    },
    subtitle_cache::SubtitleCache,
    ui::primitives::{
        Badge, BadgeTone, ListRow,
        alert::{Alert, AlertTone},
        scroll_area::ScrollArea,
        segmented::Segmented,
        sheet::Sheet,
        text_input::{TextInput, TextInputEvent},
    },
};
use kinema_core::{
    subtitle_languages::SubtitleLanguages,
    subtitle_search::{MOVIE_HASH_CHUNK, SubtitleQuery, SubtitleResult, movie_hash},
    subtitles::SubtitleFormat,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

/// Where the window finds a hashable file for its session: `(size, path)` only
/// when the first and last 64 KiB are already durable. Set once by the owner of
/// the media service; without it searches simply carry no hash.
pub(crate) type HashWindowFn = Arc<dyn Fn(&str) -> Option<(u64, PathBuf)> + Send + Sync>;

/// Global hook for [`HashWindowFn`].
#[derive(Clone)]
pub(crate) struct HashWindowSource(pub HashWindowFn);
impl Global for HashWindowSource {}

/// Installs the hash source; call after the media service has started.
pub(crate) fn set_hash_window_source(source: HashWindowFn, cx: &mut App) {
    cx.set_global(HashWindowSource(source));
}

/// What is known about the saved API key, without touching secure storage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum KeyKnowledge {
    #[default]
    Unknown,
    Present,
    Missing,
}

/// Window-lifetime online subtitle state; survives closing the sheet.
#[derive(Default)]
pub(super) struct OnlineSubtitles {
    cache: Arc<Mutex<SessionCache>>,
    key: KeyKnowledge,
    /// Credential generation `key` was read at.
    key_generation: Option<u64>,
    key_task: Option<Task<()>>,
    allowance: Option<Allowance>,
    /// External track ids that came from the service, for one media load.
    tracks: Vec<i64>,
    tracks_media: u64,
}

impl OnlineSubtitles {
    /// A key read before the credentials last changed is no longer known.
    pub(super) fn key(&self) -> KeyKnowledge {
        if self.key_generation == Some(opensubtitles_generation()) {
            self.key
        } else {
            KeyKnowledge::Unknown
        }
    }

    fn set_key(&mut self, key: KeyKnowledge) {
        self.key = key;
        self.key_generation = Some(opensubtitles_generation());
    }

    pub(super) fn track_ids(&self, media_generation: u64) -> &[i64] {
        if self.tracks_media == media_generation {
            &self.tracks
        } else {
            &[]
        }
    }

    pub(super) fn record_tracks(&mut self, media_generation: u64, ids: Vec<i64>) {
        if self.tracks_media != media_generation {
            self.tracks.clear();
            self.tracks_media = media_generation;
        }
        self.tracks.extend(ids);
    }

    #[cfg(feature = "ui-probe")]
    pub(super) fn fixture(key: KeyKnowledge, allowance: Option<Allowance>) -> Self {
        Self {
            key,
            key_generation: Some(opensubtitles_generation()),
            allowance,
            ..Self::default()
        }
    }
}

/// The open sheet. Dropping it cancels its work.
pub(super) struct SearchSheet {
    pub(super) model: SearchModel,
    pub(super) query: Option<Entity<TextInput>>,
    scroll: ScrollHandle,
    close_focus: FocusHandle,
    cancel: Arc<AtomicBool>,
    task: Task<()>,
    _submit: Option<Subscription>,
}
impl Drop for SearchSheet {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

// ── background work ──

/// Why background work produced nothing to show.
#[derive(Debug, Eq, PartialEq)]
enum WorkError {
    /// The owner dropped the request; nothing to report.
    Cancelled,
    Problem(Problem),
}
impl From<OpenSubtitlesError> for WorkError {
    fn from(error: OpenSubtitlesError) -> Self {
        Problem::from_error(&error).map_or(Self::Cancelled, Self::Problem)
    }
}

/// Signed-in state reused across searches until the credentials change, the
/// service expires the session, or the window closes. Memory only.
struct Authed {
    generation: u64,
    client: Arc<OpenSubtitlesClient>,
    session: Option<Session>,
    /// The service refused the saved login; never retried with the same credentials.
    login_rejected: bool,
}
#[derive(Default)]
struct SessionCache {
    authed: Option<Authed>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Blocking: reads secure storage and may log in. Worker only.
fn authorize(
    cache: &mut SessionCache,
    cancel: &AtomicBool,
) -> Result<(Arc<OpenSubtitlesClient>, Option<Session>), WorkError> {
    let generation = opensubtitles_generation();
    if cache
        .authed
        .as_ref()
        .is_some_and(|authed| authed.generation != generation)
    {
        cache.authed = None;
    }
    if cache.authed.is_none() {
        let credentials = OpenSubtitlesCredentialStore::new()
            .credentials()
            .map_err(|error| {
                WorkError::Problem(match error {
                    CredentialError::Missing => Problem::NoApiKey,
                    _ => Problem::KeyStorage,
                })
            })?;
        let client = OpenSubtitlesClient::new(credentials.api_key, default_user_agent())?;
        let (session, login_rejected) = match credentials.login {
            Some((username, password)) => {
                match client.login(username.expose(), &password, cancel) {
                    Ok(session) => (Some(session), false),
                    Err(OpenSubtitlesError::LoginRejected) => (None, true),
                    // Not cached: a network failure must not look like a rejection.
                    Err(error) => return Err(error.into()),
                }
            }
            None => (None, false),
        };
        cache.authed = Some(Authed {
            generation,
            client: Arc::new(client),
            session,
            login_rejected,
        });
    }
    match &cache.authed {
        Some(authed) if authed.login_rejected => Err(WorkError::Problem(Problem::LoginRejected)),
        Some(authed) => Ok((authed.client.clone(), authed.session.clone())),
        None => Err(WorkError::Problem(Problem::Service)),
    }
}

/// Runs one service call, signing in first. An expired session is replaced and
/// the call repeated once. Returns the value and the allowance the login reported.
fn with_session<T>(
    cache: &Mutex<SessionCache>,
    cancel: &AtomicBool,
    call: impl Fn(&OpenSubtitlesClient, Option<&Session>) -> Result<T, OpenSubtitlesError>,
) -> Result<(T, Option<u32>), WorkError> {
    let mut cache = lock(cache);
    for attempt in 0..2 {
        let (client, session) = authorize(&mut cache, cancel)?;
        let remaining = session.as_ref().and_then(|s| s.remaining_downloads);
        match call(&client, session.as_ref()) {
            Ok(value) => return Ok((value, remaining)),
            Err(OpenSubtitlesError::SessionExpired) if attempt == 0 && session.is_some() => {
                cache.authed = None;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(WorkError::Problem(Problem::Service))
}

/// Reads the first and last 64 KiB of a durable file and hashes them.
/// `None` on any failure: the hash is only a ranking boost.
pub(super) fn read_movie_hash(size: u64, path: &Path, cancel: &AtomicBool) -> Option<u64> {
    use std::io::{Read, Seek, SeekFrom};
    let chunk = MOVIE_HASH_CHUNK as u64;
    let mut file = std::fs::File::open(path).ok()?;
    if size < chunk || file.metadata().ok()?.len() < size {
        return None;
    }
    let mut head = vec![0u8; MOVIE_HASH_CHUNK];
    file.read_exact(&mut head).ok()?;
    if cancel.load(Ordering::Acquire) {
        return None;
    }
    let mut tail = vec![0u8; MOVIE_HASH_CHUNK];
    file.seek(SeekFrom::Start(size - chunk)).ok()?;
    file.read_exact(&mut tail).ok()?;
    movie_hash(size, &head, &tail)
}

struct SearchDone {
    result: Result<Vec<SubtitleResult>, WorkError>,
    allowance: Option<Allowance>,
}

fn search_blocking(
    cache: &Mutex<SessionCache>,
    mut query: SubtitleQuery,
    hash_source: Option<(HashWindowFn, String)>,
    cancel: &AtomicBool,
) -> SearchDone {
    query.moviehash = hash_source
        .and_then(|(source, session)| source(&session))
        .and_then(|(size, path)| read_movie_hash(size, &path, cancel));
    let result = with_session(cache, cancel, |client, session| {
        client.search(&query, session, cancel)
    });
    match result {
        Ok((results, remaining)) => SearchDone {
            result: Ok(results),
            allowance: remaining.map(|remaining| Allowance {
                remaining,
                reset_time_utc: None,
            }),
        },
        Err(error) => SearchDone {
            result: Err(error),
            allowance: None,
        },
    }
}

struct DownloadDone {
    /// Absolute path of the cached file.
    result: Result<String, WorkError>,
    allowance: Option<Allowance>,
}

fn format_of(downloaded: &DownloadedSubtitle) -> SubtitleFormat {
    let extension = |name: &str| name.rsplit_once('.').map(|(_, e)| e.to_owned());
    downloaded
        .format
        .clone()
        .or_else(|| downloaded.file_name.as_deref().and_then(extension))
        .and_then(|e| SubtitleFormat::from_extension(&e.to_ascii_lowercase()))
        .unwrap_or(SubtitleFormat::Srt)
}

fn download_blocking(
    cache: &Mutex<SessionCache>,
    file_id: u64,
    cancel: &AtomicBool,
) -> DownloadDone {
    let unusable = |allowance| DownloadDone {
        result: Err(WorkError::Problem(Problem::Unusable)),
        allowance,
    };
    let Ok(paths) = AppPaths::resolve() else {
        return unusable(None);
    };
    let files = SubtitleCache::from_paths(&paths);
    // A cached file costs no allowance and needs no sign-in.
    let usable = |path: PathBuf| {
        path.is_absolute()
            .then(|| path.to_str().map(str::to_owned))
            .flatten()
    };
    if let Some(path) = files.lookup(file_id).and_then(usable) {
        return DownloadDone {
            result: Ok(path),
            allowance: None,
        };
    }
    let downloaded = with_session(cache, cancel, |client, session| {
        client.download(file_id, session, cancel)
    });
    let (downloaded, _) = match downloaded {
        Ok(value) => value,
        Err(WorkError::Problem(Problem::QuotaExhausted { reset_time_utc })) => {
            return DownloadDone {
                result: Err(WorkError::Problem(Problem::QuotaExhausted {
                    reset_time_utc: reset_time_utc.clone(),
                })),
                allowance: Some(Allowance {
                    remaining: 0,
                    reset_time_utc,
                }),
            };
        }
        Err(error) => {
            return DownloadDone {
                result: Err(error),
                allowance: None,
            };
        }
    };
    let allowance = downloaded.remaining.map(|remaining| Allowance {
        remaining,
        reset_time_utc: downloaded.reset_time_utc.clone(),
    });
    match files
        .store(file_id, format_of(&downloaded), &downloaded.bytes)
        .ok()
        .and_then(usable)
    {
        Some(path) => DownloadDone {
            result: Ok(path),
            allowance,
        },
        None => unusable(allowance),
    }
}

// ── window glue ──

impl PlayerWindow {
    fn coordinator(&self, cx: &App) -> Option<Entity<AppCoordinator>> {
        self.lease
            .as_ref()
            .and_then(|(id, _)| cx.try_global::<PlayerWindows>()?.entries.get(id))
            .map(|entry| entry.coordinator.clone())
            .or_else(|| self.progress_owner.clone())
    }

    /// Cheap in-memory read of the user's subtitle languages.
    fn subtitle_languages(&self, cx: &App) -> Vec<String> {
        self.coordinator(cx)
            .and_then(|c| {
                c.read(cx)
                    .settings
                    .as_ref()
                    .map(|s| s.subtitle_languages.to_vec())
            })
            .unwrap_or_else(|| SubtitleLanguages::default().to_vec())
    }

    /// Why **Search online** is unavailable, if it is.
    pub(super) fn search_unavailable_reason(&self) -> Option<&'static str> {
        if self.online.key() == KeyKnowledge::Missing {
            return Some("Add an OpenSubtitles API key in Settings");
        }
        let ready = self.subtitles_ready();
        #[cfg(feature = "ui-probe")]
        let ready = ready || self.fixture_subtitles;
        (!ready).then_some("Wait for the media to open")
    }

    /// Re-reads whether an API key is saved, off the UI thread, when the
    /// credentials changed since the last read.
    pub(super) fn refresh_online_key(&mut self, cx: &mut Context<Self>) {
        if self.lease.is_none() {
            return;
        }
        let generation = opensubtitles_generation();
        if self.online.key_generation == Some(generation) {
            return;
        }
        let work = cx.background_executor().spawn(async move {
            OpenSubtitlesCredentialStore::new()
                .status()
                .ok()
                .map(|status| status.api_key)
        });
        self.online.key_task = Some(cx.spawn(async move |view, cx| {
            let present = work.await;
            let _ = view.update(cx, |view, cx| {
                // Storage failures stay unknown: the sheet reports them on use.
                if let Some(present) = present {
                    view.online.key = if present {
                        KeyKnowledge::Present
                    } else {
                        KeyKnowledge::Missing
                    };
                    view.online.key_generation = Some(generation);
                }
                cx.notify();
            });
        }));
    }

    /// Subtitle-menu action: opens the sheet, and searches at once when the
    /// catalog identity is known.
    pub(super) fn open_subtitle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_unavailable_reason().is_some() {
            tracing::info!(category = "subtitle_search", outcome = "unavailable");
            self.show_subtitle_notice(SubtitleNotice::NotReady, cx);
            return;
        }
        let by_id = searchable_by_id(self.subtitle_target.as_ref());
        let languages = self.subtitle_languages(cx);
        let query = (!by_id).then(|| {
            cx.new(|cx| {
                let mut input = TextInput::new(window, cx);
                input.set_label("Subtitle search", cx);
                input.set_placeholder("Title", cx);
                input
            })
        });
        let submit = query.as_ref().map(|input| {
            cx.subscribe(input, |this, _, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submitted => this.search_again(cx),
                // The field consumes Esc as an action before the sheet sees the key.
                TextInputEvent::Cancelled => this.close_search(cx),
                _ => {}
            })
        });
        self.search = Some(SearchSheet {
            model: SearchModel::new(languages, by_id),
            query,
            scroll: ScrollHandle::new(),
            close_focus: cx.focus_handle(),
            cancel: Arc::new(AtomicBool::new(false)),
            task: Task::ready(()),
            _submit: submit,
        });
        tracing::info!(
            category = "subtitle_search",
            by_id,
            "Online subtitle search opened"
        );
        if by_id {
            self.run_search(0, cx);
        }
        self.activity(cx);
    }

    pub(super) fn close_search(&mut self, cx: &mut Context<Self>) {
        if self.search.take().is_some() {
            cx.notify();
        }
    }

    fn open_account_settings(&mut self, cx: &mut Context<Self>) {
        self.close_search(cx);
        if let Some(coordinator) = self.coordinator(cx) {
            coordinator.update(cx, |c, cx| {
                c.open_settings_at(
                    crate::controllers::coordinator::SettingsDestination::Subtitles,
                    cx,
                )
            });
        }
    }

    /// Search button, Enter in the field, Retry and language changes.
    pub(super) fn search_again(&mut self, cx: &mut Context<Self>) {
        let has_text = self.search.as_ref().is_some_and(|sheet| {
            sheet
                .query
                .as_ref()
                .is_none_or(|q| !q.read(cx).text().trim().is_empty())
        });
        if !has_text {
            return;
        }
        if let Some(generation) = self
            .search
            .as_mut()
            .and_then(|sheet| sheet.model.begin_search())
        {
            self.run_search(generation, cx);
        }
    }

    fn select_search_language(&mut self, code: Option<String>, cx: &mut Context<Self>) {
        let changed = self
            .search
            .as_mut()
            .is_some_and(|sheet| sheet.model.select_language(code));
        if changed {
            self.search_again(cx);
        }
        cx.notify();
    }

    /// Spawns the search for `generation`; the model is already `Loading`.
    fn run_search(&mut self, generation: u64, cx: &mut Context<Self>) {
        let hash_source = cx
            .try_global::<HashWindowSource>()
            .map(|source| source.0.clone())
            .zip(self.lease.as_ref().map(|(id, _)| id.clone()));
        let target = self.subtitle_target.clone();
        let cache = self.online.cache.clone();
        let Some(sheet) = self.search.as_mut() else {
            return;
        };
        let text = sheet
            .query
            .as_ref()
            .map(|q| q.read(cx).text().to_owned())
            .unwrap_or_default();
        let Some(query) = build_query(target.as_ref(), &text, sheet.model.query_languages(), None)
        else {
            sheet.model.finish_search(generation, Err(Problem::Service));
            return;
        };
        sheet.cancel.store(true, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        sheet.cancel = cancel.clone();
        let work = cx
            .background_executor()
            .spawn(async move { search_blocking(&cache, query, hash_source, &cancel) });
        sheet.task = cx.spawn(async move |view, cx| {
            let done = work.await;
            let _ = view.update(cx, |view, cx| view.search_finished(generation, done, cx));
        });
        cx.notify();
    }

    fn search_finished(&mut self, generation: u64, done: SearchDone, cx: &mut Context<Self>) {
        let outcome = match done.result {
            Ok(results) => Ok(results),
            Err(WorkError::Cancelled) => return,
            Err(WorkError::Problem(problem)) => Err(problem),
        };
        let Some(sheet) = self.search.as_mut() else {
            return;
        };
        let (results, category) = match &outcome {
            Ok(results) => (results.len(), "ok"),
            Err(problem) => (0, problem.category()),
        };
        if !sheet.model.finish_search(generation, outcome) {
            return;
        }
        tracing::info!(
            category = "subtitle_search",
            outcome = category,
            results,
            "Online subtitle search finished"
        );
        match category {
            "ok" => self.online.set_key(KeyKnowledge::Present),
            "no_api_key" => self.online.set_key(KeyKnowledge::Missing),
            _ => {}
        }
        if done.allowance.is_some() {
            self.online.allowance = done.allowance;
        }
        cx.notify();
    }

    /// A result row was chosen: use the cached file or download it, then load it.
    pub(super) fn choose_result(&mut self, file_id: u64, cx: &mut Context<Self>) {
        if !self.subtitles_ready() {
            self.show_subtitle_notice(SubtitleNotice::NotReady, cx);
            return;
        }
        let cache = self.online.cache.clone();
        let Some(sheet) = self.search.as_mut() else {
            return;
        };
        let Some(generation) = sheet.model.begin_add(file_id) else {
            return;
        };
        sheet.cancel.store(true, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        sheet.cancel = cancel.clone();
        let work = cx
            .background_executor()
            .spawn(async move { download_blocking(&cache, file_id, &cancel) });
        sheet.task = cx.spawn(async move |view, cx| {
            let done = work.await;
            let _ = view.update(cx, |view, cx| view.download_finished(generation, done, cx));
        });
        cx.notify();
    }

    fn download_finished(&mut self, generation: u64, done: DownloadDone, cx: &mut Context<Self>) {
        if done.allowance.is_some() && self.search.is_some() {
            self.online.allowance = done.allowance;
        }
        let Some(sheet) = self.search.as_mut() else {
            return;
        };
        let path = match done.result {
            Ok(path) => path,
            Err(WorkError::Cancelled) => return,
            Err(WorkError::Problem(problem)) => {
                tracing::info!(
                    category = "subtitle_search",
                    outcome = problem.category(),
                    "Online subtitle download failed"
                );
                sheet.model.fail_add(generation, problem);
                cx.notify();
                return;
            }
        };
        let chosen = match (&sheet.model.phase, sheet.model.adding) {
            (Phase::Results(results), Some(id)) if sheet.model.generation() == generation => {
                results.iter().find(|r| r.file_id == id).cloned()
            }
            _ => None,
        };
        let Some(result) = chosen else {
            return;
        };
        let title = subtitle_title(&result);
        let file = kinema_mpv::SubtitleFile {
            path: &path,
            title: Some(&title),
            lang: subtitle_lang(&result),
        };
        if self.submit_subtitles(&[file], 0, true, cx) {
            tracing::info!(
                category = "subtitle_search",
                outcome = "requested",
                "Online subtitle load requested"
            );
        } else if let Some(sheet) = self.search.as_mut() {
            sheet.model.fail_add(generation, Problem::Unusable);
        }
        cx.notify();
    }

    /// The pending online add was observed (`true`) or failed (`false`).
    /// Closes the sheet on success; failures stay visible in it.
    pub(super) fn search_add_resolved(&mut self, added: bool) {
        let Some(sheet) = self.search.as_mut() else {
            return;
        };
        let generation = sheet.model.generation();
        if sheet.model.finish_add(generation, added) == Some(true) {
            self.search = None;
        }
    }

    pub(super) fn search_add_abandoned(&mut self) {
        if let Some(sheet) = self.search.as_mut() {
            sheet.model.abandon_add();
        }
    }

    // ── render ──

    pub(super) fn search_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<Sheet> {
        let weak = cx.weak_entity();
        let allowance = self.online.allowance.as_ref().map(Allowance::label);
        let sheet = self.search.as_ref()?;
        let model = &sheet.model;
        let busy = model.adding.is_some();
        let list_max = (window.viewport_size().height - player::SEARCH_SHEET_RESERVE)
            .max(size::ROW_TALL * 2.)
            .min(player::SEARCH_LIST_MAX);
        let mut out = Sheet::new("subtitle-search", "Search subtitles").on_dismiss({
            let weak = weak.clone();
            move |_, cx| {
                let _ = weak.update(cx, |view, cx| view.close_search(cx));
            }
        });
        out = match &sheet.query {
            Some(input) => out.initial_focus(input.read(cx).focus_handle(cx)),
            None => out.initial_focus(sheet.close_focus.clone()),
        };
        if let Some(input) = &sheet.query {
            let searching = model.phase == Phase::Loading || busy;
            out = out.body(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S8)
                    .child(div().flex_1().min_w_0().child(input.clone()))
                    .child(
                        Button::new("subtitle-search-run", "Search")
                            .variant(ButtonVariant::Primary)
                            .disabled(searching)
                            .on_click(cx.listener(|view, _, _, cx| view.search_again(cx))),
                    ),
            );
        }
        let p = *crate::ui::theme::palette(cx);
        let filter = (model.languages.len() > 1).then(|| {
            let options = filter_labels(&model.languages);
            Segmented::new(
                "subtitle-search-language",
                options
                    .into_iter()
                    .map(|(code, label)| (code, label.into())),
                model.selected.clone(),
            )
            .label("Language")
            .on_change({
                let weak = weak.clone();
                move |code, _, cx| {
                    let code = code.clone();
                    let _ = weak.update(cx, |view, cx| view.select_search_language(code, cx));
                }
            })
        });
        if filter.is_some() || allowance.is_some() {
            out = out.body(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(s::S12)
                    .child(div().children(filter))
                    .child(
                        div()
                            .text(TextSize::Sm)
                            .text_color(p.muted_foreground)
                            .children(allowance),
                    ),
            );
        }
        if let Some(problem) = &model.add_problem {
            out = out.body(self.problem_alert(problem, false, cx));
        }
        out = out.body(match &model.phase {
            Phase::Idle => div()
                .text_color(p.muted_foreground)
                .child("Enter a title to search.")
                .into_any_element(),
            Phase::Loading => div()
                .text_color(p.muted_foreground)
                .child("Searching…")
                .into_any_element(),
            Phase::Empty => Alert::new(AlertTone::Empty, "No subtitles found")
                .body(if sheet.query.is_some() {
                    "Try another language or a different title."
                } else {
                    "Try another language."
                })
                .into_any_element(),
            Phase::Failed(problem) => self.problem_alert(problem, true, cx).into_any_element(),
            Phase::Results(results) => {
                let mut list = div().flex().flex_col();
                for result in results {
                    let file_id = result.file_id;
                    let adding = model.adding == Some(file_id);
                    let badges = result_badges(result);
                    let trailing = div()
                        .flex()
                        .items_center()
                        .gap(s::S4)
                        .children(badges.into_iter().map(|label| {
                            Badge::new(label).tone(match label {
                                "Hash match" => BadgeTone::Success,
                                "AI translated" => BadgeTone::Warning,
                                _ => BadgeTone::Neutral,
                            })
                        }))
                        .when(adding, |el| {
                            el.child(
                                div()
                                    .text(TextSize::Sm)
                                    .text_color(p.muted_foreground)
                                    .child("Adding…"),
                            )
                        });
                    let mut row = ListRow::new(
                        SharedString::from(format!("subtitle-result-{file_id}")),
                        subtitle_title(result),
                    )
                    .subtitle(result_detail(result))
                    .trailing(trailing)
                    .selected(adding);
                    if !busy {
                        row = row.on_click(
                            cx.listener(move |view, _, _, cx| view.choose_result(file_id, cx)),
                        );
                    }
                    list = list.child(row);
                }
                let height = (size::ROW_TALL * results.len() as f32).min(list_max);
                div()
                    .h(height)
                    .child(
                        ScrollArea::new("subtitle-results", sheet.scroll.clone())
                            .label("Subtitle results")
                            .child(list),
                    )
                    .into_any_element()
            }
        });
        Some(
            out.footer(
                Button::new("subtitle-search-close", "Close")
                    .focus_handle(sheet.close_focus.clone())
                    .on_click(cx.listener(|view, _, _, cx| view.close_search(cx))),
            ),
        )
    }

    /// `retry` offers Retry for a failed search; a failed choose is retried by
    /// clicking the row again.
    fn problem_alert(&self, problem: &Problem, retry: bool, cx: &mut Context<Self>) -> Alert {
        let tone = match problem {
            Problem::Service | Problem::KeyStorage | Problem::Unusable => AlertTone::Error,
            _ => AlertTone::Warning,
        };
        let alert = Alert::new(tone, problem.title()).body(problem.detail());
        match problem.action() {
            ProblemAction::Retry if retry => alert.action(
                Button::new("subtitle-search-retry", "Retry")
                    .on_click(cx.listener(|view, _, _, cx| view.search_again(cx))),
            ),
            ProblemAction::OpenSettings => alert.action(
                Button::new("subtitle-search-settings", "Open Settings")
                    .on_click(cx.listener(|view, _, _, cx| view.open_account_settings(cx))),
            ),
            _ => alert,
        }
    }
}

#[cfg(feature = "ui-probe")]
impl PlayerWindow {
    /// Lab and tests: show the sheet in a fixed state without any service call.
    pub(super) fn show_search_fixture(
        &mut self,
        model: SearchModel,
        text_mode: bool,
        allowance: Option<Allowance>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = text_mode.then(|| {
            cx.new(|cx| {
                let mut input = TextInput::new(window, cx);
                input.set_label("Subtitle search", cx);
                input.set_placeholder("Title", cx);
                input
            })
        });
        let submit = query.as_ref().map(|input| {
            cx.subscribe(input, |this, _, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submitted => this.search_again(cx),
                // The field consumes Esc as an action before the sheet sees the key.
                TextInputEvent::Cancelled => this.close_search(cx),
                _ => {}
            })
        });
        self.online.allowance = allowance;
        self.search = Some(SearchSheet {
            model,
            query,
            scroll: ScrollHandle::new(),
            close_focus: cx.focus_handle(),
            cancel: Arc::new(AtomicBool::new(false)),
            task: Task::ready(()),
            _submit: submit,
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn temp_file(bytes: &[u8]) -> std::io::Result<PathBuf> {
        let path = std::env::temp_dir().join(format!(
            "kinema-search-hash-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    #[test]
    fn movie_hash_reads_head_and_tail_of_a_durable_file() -> TestResult {
        let size = MOVIE_HASH_CHUNK * 3;
        let bytes: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let path = temp_file(&bytes)?;
        let cancel = AtomicBool::new(false);
        let expected = movie_hash(
            size as u64,
            &bytes[..MOVIE_HASH_CHUNK],
            &bytes[size - MOVIE_HASH_CHUNK..],
        );
        assert!(expected.is_some());
        assert_eq!(read_movie_hash(size as u64, &path, &cancel), expected);
        // The service's size must not exceed what is on disk.
        assert_eq!(read_movie_hash(size as u64 + 1, &path, &cancel), None);
        // Too small, missing and cancelled inputs just omit the hash.
        assert_eq!(read_movie_hash(10, &path, &cancel), None);
        assert_eq!(
            read_movie_hash(size as u64, &path.with_extension("missing"), &cancel),
            None
        );
        cancel.store(true, Ordering::Release);
        assert_eq!(read_movie_hash(size as u64, &path, &cancel), None);
        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn downloaded_format_comes_from_the_service_then_the_name() {
        let downloaded = |format: Option<&str>, name: Option<&str>| DownloadedSubtitle {
            bytes: Vec::new(),
            file_name: name.map(Into::into),
            format: format.map(Into::into),
            remaining: None,
            reset_time_utc: None,
        };
        assert_eq!(
            format_of(&downloaded(Some("ASS"), Some("a.srt"))),
            SubtitleFormat::Ass
        );
        assert_eq!(
            format_of(&downloaded(None, Some("movie.en.VTT"))),
            SubtitleFormat::WebVtt
        );
        assert_eq!(format_of(&downloaded(None, None)), SubtitleFormat::Srt);
        assert_eq!(
            format_of(&downloaded(Some("zip"), Some("x.zip"))),
            SubtitleFormat::Srt
        );
    }

    #[test]
    fn cancelled_service_calls_are_not_reported() {
        assert_eq!(
            WorkError::from(OpenSubtitlesError::Cancelled),
            WorkError::Cancelled
        );
        assert_eq!(
            WorkError::from(OpenSubtitlesError::Network),
            WorkError::Problem(Problem::Offline)
        );
    }

    #[test]
    fn online_tracks_are_scoped_to_one_media_load() {
        let mut online = OnlineSubtitles::default();
        online.record_tracks(3, vec![5]);
        online.record_tracks(3, vec![7]);
        assert_eq!(online.track_ids(3), [5, 7]);
        assert!(online.track_ids(4).is_empty());
        online.record_tracks(4, vec![2]);
        assert_eq!(online.track_ids(4), [2]);
        assert!(online.track_ids(3).is_empty());
    }
}
