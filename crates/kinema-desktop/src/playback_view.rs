//! Acquisition, file-choice and session panel.
//!
//! No I/O happens here. The view shows what the application worker reports and emits
//! [`PlaybackIntent`]s. It never guesses a file: only a `Unique` selection is highlighted, and
//! even then the user presses Play. "Started" means the external player was launched, not that
//! media is playing. Progress is bytes acquired into the cache, not watch progress.

use gpui_kit::base::Disableable as _;
use gpui_kit::component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants},
    progress::Progress,
    radio::{Radio, RadioGroup},
};
use gpui_kit::*;
use kinema_core::sources::{DebridKind, FileSelection, PlaybackTarget, RemoteFile};

use crate::{
    media::{PlaybackSession, UsageSnapshot},
    providers::PreparedTorrent,
    ui::strip::{NoticeTone, notice},
};

/// Requests for the application worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaybackIntent {
    /// Resolve and play one file of the preparation the user was shown.
    PlayFile {
        preparation: u64,
        file_id: String,
    },
    ReleaseSession(String),
    OpenLocalFile,
    /// Manual removal of unused cached media.
    CleanupUnused,
    /// Navigate to Settings so the user can raise the cache limit. Never changes it itself.
    OpenSettings,
}

// ---------------------------------------------------------------------------------------
// Pure model
// ---------------------------------------------------------------------------------------

/// Binary-unit size text, e.g. `1.5 GiB`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

pub fn provider_label(kind: DebridKind) -> &'static str {
    match kind {
        DebridKind::RealDebrid => "Real-Debrid",
        DebridKind::AllDebrid => "AllDebrid",
    }
}

/// Only plain-HTTP loopback IP URLs without credentials are shown or copied.
/// Upstream (provider) URLs never reach this view's display path.
pub fn safe_local_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let loopback = match parsed.host()? {
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
        url::Host::Domain(_) => false,
    };
    (parsed.scheme() == "http"
        && loopback
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none())
    .then(|| url.to_owned())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowStatus {
    Suggested,
    Possible,
    Unmatched,
}

impl RowStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Suggested => "Suggested match",
            Self::Possible => "Possible match",
            Self::Unmatched => "Not matched to this title",
        }
    }
}

/// File choice for one preparation.
#[derive(Clone, Debug)]
pub struct FilePicker {
    pub preparation: u64,
    pub provider: DebridKind,
    pub target_label: String,
    pub files: Vec<RemoteFile>,
    /// Indices into `files` proposed by the selection policy, in provider order.
    suggested: Vec<usize>,
    unique: bool,
    highlighted: Option<usize>,
    show_all: bool,
}

impl FilePicker {
    pub fn new(
        preparation: u64,
        provider: DebridKind,
        target_label: String,
        files: Vec<RemoteFile>,
        selection: &FileSelection,
    ) -> Self {
        let valid = |i: &usize| *i < files.len();
        let (suggested, unique): (Vec<usize>, bool) = match selection {
            FileSelection::Unique(i) => (Some(*i).filter(valid).into_iter().collect(), true),
            FileSelection::Ambiguous(list) => {
                let mut seen = Vec::new();
                for i in list.iter().filter(|i| valid(i)) {
                    if !seen.contains(i) {
                        seen.push(*i);
                    }
                }
                (seen, false)
            }
            FileSelection::NotFound => (Vec::new(), false),
        };
        // Conservative: highlight only a unique match, never by size or order.
        let highlighted = if unique {
            suggested.first().copied()
        } else {
            None
        };
        Self {
            preparation,
            provider,
            target_label,
            files,
            unique: unique && highlighted.is_some(),
            suggested,
            highlighted,
            show_all: false,
        }
    }

    pub fn has_suggestions(&self) -> bool {
        !self.suggested.is_empty()
    }

    pub fn is_unique(&self) -> bool {
        self.unique
    }

    pub fn show_all(&self) -> bool {
        self.show_all
    }

    pub fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    /// Indices shown: suggestions, or every file when asked or when none matched.
    pub fn visible(&self) -> Vec<usize> {
        if self.show_all || self.suggested.is_empty() {
            (0..self.files.len()).collect()
        } else {
            self.suggested.clone()
        }
    }

    pub fn set_show_all(&mut self, show_all: bool) {
        self.show_all = show_all;
        if self
            .highlighted
            .is_some_and(|i| !self.visible().contains(&i))
        {
            self.highlighted = None;
        }
    }

    /// Selects a visible file; returns whether the selection changed.
    pub fn select(&mut self, index: usize) -> bool {
        if self.visible().contains(&index) && self.highlighted != Some(index) {
            self.highlighted = Some(index);
            return true;
        }
        false
    }

    pub fn status(&self, index: usize) -> RowStatus {
        match (self.suggested.contains(&index), self.unique) {
            (true, true) => RowStatus::Suggested,
            (true, false) => RowStatus::Possible,
            _ => RowStatus::Unmatched,
        }
    }

    /// The confirmed choice as `(preparation, provider file id)`; `None` until the user picks.
    pub fn confirm(&self) -> Option<(u64, String)> {
        let file = self.files.get(self.highlighted?)?;
        Some((self.preparation, file.id.clone()))
    }

    pub fn row_label(&self, index: usize) -> String {
        match self.files.get(index) {
            Some(file) => format!(
                "{}  ·  {}  ·  {}",
                file.path,
                format_bytes(file.size),
                self.status(index).label()
            ),
            None => String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StartedSession {
    pub id: String,
    pub file_name: String,
    /// Present only when the URL is a safe loopback address.
    pub local_url: Option<String>,
    /// A release was requested and the worker has not confirmed it yet.
    pub releasing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheNotice {
    /// One file is bigger than the whole cache limit. The user decides whether to raise it.
    LimitNeeded { size: u64, limit: u64 },
    /// Cache or disk is full; cleaning unused media manually may help.
    Full(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UsageLine {
    /// Acquired / reserved, `None` when nothing is reserved.
    pub fraction: Option<f32>,
    pub text: String,
    pub over_limit: bool,
}

pub fn usage_line(usage: &UsageSnapshot) -> UsageLine {
    let limit = format_bytes(usage.limit_bytes);
    let over_limit = usage.reserved_bytes > usage.limit_bytes;
    if usage.reserved_bytes == 0 {
        return UsageLine {
            fraction: None,
            text: format!("Nothing cached or reserved. Cache limit {limit}."),
            over_limit,
        };
    }
    let fraction = (usage.downloaded_bytes as f64 / usage.reserved_bytes as f64).clamp(0.0, 1.0);
    UsageLine {
        fraction: Some(fraction as f32),
        text: format!(
            "Downloaded {} of {} reserved. Cache limit {limit}.",
            format_bytes(usage.downloaded_bytes.min(usage.reserved_bytes)),
            format_bytes(usage.reserved_bytes),
        ),
        over_limit,
    }
}

/// State transitions, separate from rendering. Errors and notices never discard the picker,
/// session or usage the user is already looking at.
#[derive(Debug, Default)]
pub struct PlaybackModel {
    pub picker: Option<FilePicker>,
    pub session: Option<StartedSession>,
    pub busy: Option<String>,
    pub error: Option<String>,
    pub notice: Option<CacheNotice>,
    pub usage: Option<UsageSnapshot>,
}

impl PlaybackModel {
    pub fn show_prepared(&mut self, picker: FilePicker) {
        self.picker = Some(picker);
        self.busy = None;
        self.error = None;
        self.notice = None;
    }

    pub fn show_started(&mut self, session: PlaybackSession, file_name: String) {
        self.session = Some(StartedSession {
            local_url: safe_local_url(&session.url),
            id: session.id,
            file_name,
            releasing: false,
        });
        self.busy = None;
        self.error = None;
        self.notice = None;
    }

    pub fn show_usage(&mut self, usage: UsageSnapshot) {
        self.usage = Some(usage);
    }

    pub fn show_busy(&mut self, message: String) {
        self.busy = Some(message);
        self.error = None;
        self.notice = None;
    }

    pub fn show_error(&mut self, message: String) {
        self.busy = None;
        self.notice = None;
        self.error = Some(message);
    }

    pub fn show_limit_needed(&mut self, size: u64, limit: u64) {
        self.busy = None;
        self.error = None;
        self.notice = Some(CacheNotice::LimitNeeded { size, limit });
    }

    pub fn show_cache_full(&mut self, message: String) {
        self.busy = None;
        self.error = None;
        self.notice = Some(CacheNotice::Full(message));
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.id.as_str())
    }

    /// Marks the session as releasing and returns its id. The session stays visible until
    /// [`Self::released`] confirms. `None` if there is no session or a release is pending.
    pub fn begin_release(&mut self) -> Option<String> {
        let session = self.session.as_mut().filter(|s| !s.releasing)?;
        session.releasing = true;
        let id = session.id.clone();
        self.error = None;
        Some(id)
    }

    /// The worker confirmed the release. Other sessions' confirmations are ignored.
    pub fn released(&mut self, id: &str) -> bool {
        if self.session_id() == Some(id) {
            self.session = None;
            return true;
        }
        false
    }

    /// The release was rejected, dropped by backpressure or failed: keep the session and let
    /// the user retry. Ignored when `id` is not the displayed session.
    pub fn release_failed(&mut self, id: &str, message: String) -> bool {
        match self.session.as_mut().filter(|s| s.id == id) {
            Some(session) => {
                session.releasing = false;
                self.busy = None;
                self.notice = None;
                self.error = Some(message);
                true
            }
            None => false,
        }
    }

    /// The server dropped this lease (for example after an account change).
    pub fn clear_session(&mut self, id: &str) -> bool {
        self.released(id)
    }

    /// The prepared torrent is no longer playable; its file list must not be offered.
    pub fn clear_prepared(&mut self) {
        self.picker = None;
    }

    /// Async media error for one session; errors for other sessions are ignored.
    pub fn media_error(&mut self, id: &str, message: String) -> bool {
        if self.session_id() != Some(id) {
            return false;
        }
        self.show_error(message);
        true
    }

    /// Settings changed, so a "raise the limit" notice may be out of date.
    pub fn settings_changed(&mut self) {
        if matches!(self.notice, Some(CacheNotice::LimitNeeded { .. })) {
            self.notice = None;
        }
    }

    /// Starts playing the confirmed choice, unless busy.
    pub fn play(&mut self) -> Option<PlaybackIntent> {
        if self.busy.is_some() {
            return None;
        }
        let (preparation, file_id) = self.picker.as_ref()?.confirm()?;
        self.show_busy("Preparing the selected file…".into());
        Some(PlaybackIntent::PlayFile {
            preparation,
            file_id,
        })
    }
}

fn target_label(target: &PlaybackTarget) -> String {
    match target {
        PlaybackTarget::Movie(id) => format!("Movie {}", id.as_str()),
        PlaybackTarget::Episode {
            series,
            season,
            episode,
        } => format!("{} S{season:02}E{episode:02}", series.as_str()),
    }
}

// ---------------------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------------------

pub struct PlaybackView {
    model: PlaybackModel,
}

impl EventEmitter<PlaybackIntent> for PlaybackView {}

impl PlaybackView {
    pub fn new(_window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            model: PlaybackModel::default(),
        })
    }

    /// Shows the files of a prepared torrent. `preparation` is echoed in `PlayFile` so the
    /// worker can reject a stale choice.
    pub fn show_prepared(
        &mut self,
        preparation: u64,
        prepared: PreparedTorrent,
        target: PlaybackTarget,
        selection: FileSelection,
        cx: &mut Context<Self>,
    ) {
        self.model.show_prepared(FilePicker::new(
            preparation,
            prepared.provider,
            target_label(&target),
            prepared.files,
            &selection,
        ));
        cx.notify();
    }

    /// The external player was launched. This does not mean playback is progressing.
    pub fn show_started(
        &mut self,
        session: PlaybackSession,
        file_name: String,
        cx: &mut Context<Self>,
    ) {
        self.model.show_started(session, file_name);
        cx.notify();
    }

    pub fn show_usage(&mut self, usage: UsageSnapshot, cx: &mut Context<Self>) {
        self.model.show_usage(usage);
        cx.notify();
    }

    pub fn show_busy(&mut self, message: String, cx: &mut Context<Self>) {
        self.model.show_busy(message);
        cx.notify();
    }

    /// Keeps any shown picker, session and progress; only adds the error.
    pub fn show_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.model.show_error(message);
        cx.notify();
    }

    /// One file needs `size` bytes but the cache limit is `limit`. Offers Settings; the limit is
    /// never raised automatically.
    pub fn show_limit_needed(&mut self, size: u64, limit: u64, cx: &mut Context<Self>) {
        self.model.show_limit_needed(size, limit);
        cx.notify();
    }

    /// Cache or disk full: shows `message` with manual-cleanup guidance.
    pub fn show_cache_full(&mut self, message: String, cx: &mut Context<Self>) {
        self.model.show_cache_full(message);
        cx.notify();
    }

    /// The worker confirmed `Released{id}`; only then does the session leave the view.
    pub fn show_released(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.model.released(id) {
            cx.notify();
        }
    }

    /// A release was rejected, lost to backpressure or failed. Keeps the session and re-enables
    /// Release so the user can retry. Ignored for a session that is not displayed.
    pub fn release_failed(&mut self, id: &str, message: String, cx: &mut Context<Self>) {
        if self.model.release_failed(id, message) {
            cx.notify();
        }
    }

    /// The worker invalidated this lease (account change). Removes it from view.
    pub fn clear_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.model.clear_session(id) {
            cx.notify();
        }
    }

    /// The prepared torrent was dropped by the worker; hides its file list.
    pub fn clear_prepared(&mut self, cx: &mut Context<Self>) {
        self.model.clear_prepared();
        cx.notify();
    }

    /// Async `MediaFailed`: shown only if `id` is the displayed session.
    pub fn show_media_error(&mut self, id: &str, message: String, cx: &mut Context<Self>) {
        if self.model.media_error(id, message) {
            cx.notify();
        }
    }

    /// Call when settings change so an outdated cache-limit notice disappears.
    pub fn settings_changed(&mut self, cx: &mut Context<Self>) {
        self.model.settings_changed();
        cx.notify();
    }

    fn play(&mut self, cx: &mut Context<Self>) {
        if let Some(intent) = self.model.play() {
            cx.emit(intent);
            cx.notify();
        }
    }

    fn picker_panel(&self, picker: &FilePicker, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let visible = picker.visible();
        let selected = picker
            .highlighted()
            .and_then(|i| visible.iter().position(|v| *v == i));
        let guidance = if picker.is_unique() {
            "One file matches this title. Check it, then press Play."
        } else if picker.has_suggestions() {
            "Several files could match. Choose the right one; nothing is picked for you."
        } else {
            "No file clearly matches this title. Choose a file yourself, or go back."
        };
        let mut radios = Vec::new();
        for &i in &visible {
            radios.push(
                Radio::new(("file", i))
                    .min_h(px(32.))
                    .text_size(px(13.))
                    .label(picker.row_label(i))
                    .accessibility_label(picker.row_label(i)),
            );
        }
        let busy = self.model.busy.is_some();
        let indices = visible.clone();
        let list = RadioGroup::vertical("playback-files")
            .disabled(busy)
            .selected_index(selected)
            .children(radios)
            .on_click(cx.listener(move |this, ix: &usize, _, cx| {
                if let (Some(&file), Some(picker)) = (indices.get(*ix), this.model.picker.as_mut())
                    && picker.select(file)
                {
                    cx.notify();
                }
            }));

        let mut actions = div().flex().flex_wrap().items_center().gap_2().child(
            Button::new("playback-play")
                .primary()
                .h(px(28.))
                .label("Play selected file")
                .disabled(busy || picker.confirm().is_none())
                .on_click(cx.listener(|this, _, _, cx| this.play(cx))),
        );
        if picker.has_suggestions() && (picker.show_all() || picker.files.len() > visible.len()) {
            let show_all = picker.show_all();
            actions = actions.child(
                Button::new("playback-show-all")
                    .h(px(28.))
                    .outline()
                    .label(if show_all {
                        "Show suggested files only".to_owned()
                    } else {
                        format!("Show all {} files", picker.files.len())
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(picker) = this.model.picker.as_mut() {
                            picker.set_show_all(!show_all);
                            cx.notify();
                        }
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Choose a file · {}", picker.target_label)),
            )
            .child(div().text_size(px(12.)).text_color(muted).child(format!(
                "Provider: {}. {guidance}",
                provider_label(picker.provider)
            )))
            .child(
                div()
                    .id("playback-file-scroll")
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .child(list),
            )
            .child(actions)
    }

    fn session_panel(&self, session: &StartedSession, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut buttons = div().flex().flex_wrap().items_center().gap_2();
        if let Some(url) = session.local_url.clone() {
            buttons = buttons.child(
                Button::new("playback-copy-url")
                    .outline()
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label("Copy local address")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                    })),
            );
        }
        buttons = buttons.child(
            Button::new("playback-release")
                .outline()
                .small()
                .h(px(24.))
                .text_size(px(12.))
                .label(if session.releasing {
                    "Releasing…"
                } else {
                    "Release session"
                })
                .disabled(session.releasing)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(released) = this.model.begin_release() {
                        cx.emit(PlaybackIntent::ReleaseSession(released));
                        cx.notify();
                    }
                })),
        );
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("External player launched"),
            )
            .child(div().truncate().child(session.file_name.clone()))
            .child(div().text_size(px(12.)).text_color(muted).child(
                "The player was started. This app does not track whether it is playing. \
                 Release the session when you are done to free its cache lease. \
                 It is removed here only once the release is confirmed.",
            ));
        if let Some(url) = &session.local_url {
            panel = panel.child(
                div()
                    .text_size(px(12.))
                    .text_color(muted)
                    .child(format!("Local address: {url}")),
            );
        }
        panel.child(buttons)
    }

    fn usage_panel(&self, usage: &UsageSnapshot, cx: &mut Context<Self>) -> Div {
        let line = usage_line(usage);
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Cache"),
            );
        if let Some(fraction) = line.fraction {
            panel = panel.child(
                Progress::new("playback-acquired")
                    .value(fraction * 100.0)
                    .accessibility_label("Bytes downloaded into the cache"),
            );
        }
        panel = panel.child(
            div()
                .text_size(px(12.))
                .text_color(cx.theme().muted_foreground)
                .child(line.text),
        );
        if line.over_limit {
            panel = panel.child(div().text_size(px(12.)).text_color(cx.theme().danger).child(
                "Reserved space exceeds the limit. Clean up or raise the limit in Settings.",
            ));
        }
        panel
    }
}

impl Render for PlaybackView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("playback-view")
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .size_full()
            .text_size(px(13.))
            .overflow_y_scroll()
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Playback"),
            );

        if let Some(message) = self.model.busy.clone() {
            root = root.child(notice("playback-busy", message, NoticeTone::Info, cx));
        }
        if let Some(message) = self.model.error.clone() {
            root = root.child(notice("playback-error", message, NoticeTone::Error, cx));
        }
        match self.model.notice.clone() {
            Some(CacheNotice::LimitNeeded { size, limit }) => {
                root = root.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(notice(
                            "playback-limit",
                            format!(
                                "This file needs {} but the cache limit is {}. \
                                 Raise the limit in Settings to play it. Nothing was changed.",
                                format_bytes(size),
                                format_bytes(limit)
                            ),
                            NoticeTone::Warning,
                            cx,
                        ))
                        .child(
                            div().child(
                                Button::new("playback-open-settings")
                                    .primary()
                                    .h(px(28.))
                                    .label("Open Settings to raise the limit")
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.emit(PlaybackIntent::OpenSettings)
                                    })),
                            ),
                        ),
                );
            }
            Some(CacheNotice::Full(message)) => {
                root = root.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(notice(
                            "playback-cache-full",
                            format!(
                                "{message} Remove unused cached media, or release sessions you no longer need. \
                                 Cleanup is manual and never touches files in use."
                            ),
                            NoticeTone::Warning,
                            cx,
                        ))
                        .child(
                            div().child(
                                Button::new("playback-cleanup-full")
                                    .outline()
                                    .h(px(28.))
.label("Remove unused cached media")
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.emit(PlaybackIntent::CleanupUnused)
                                    })),
                            ),
                        ),
                );
            }
            None => {}
        }

        if let Some(picker) = self.model.picker.clone() {
            root = root.child(self.picker_panel(&picker, cx));
        } else if self.model.session.is_none() {
            root = root.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("Choose a source to see its files here, or open a local file."),
            );
        }
        if let Some(session) = self.model.session.clone() {
            root = root.child(self.session_panel(&session, cx));
        }
        if let Some(usage) = self.model.usage.clone() {
            root = root.child(self.usage_panel(&usage, cx));
        }
        root.child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("playback-open-local")
                        .outline()
                        .h(px(28.))
                        .label("Open local file…")
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(PlaybackIntent::OpenLocalFile)),
                        ),
                )
                .child(
                    Button::new("playback-cleanup")
                        .outline()
                        .h(px(28.))
                        .label("Remove unused cached media")
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(PlaybackIntent::CleanupUnused)),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` also exports a `test` attribute; keep the standard one.
    use std::prelude::v1::test;

    fn file(id: &str, path: &str, size: u64) -> RemoteFile {
        RemoteFile {
            id: id.into(),
            path: path.into(),
            size,
        }
    }

    fn files() -> Vec<RemoteFile> {
        vec![
            file("a", "Show/S01E01.mkv", 10),
            file("b", "Show/S01E02.mkv", 20),
            file("c", "Show/S01E03.mkv", 9_000),
        ]
    }

    fn picker(selection: FileSelection) -> FilePicker {
        FilePicker::new(7, DebridKind::RealDebrid, "T".into(), files(), &selection)
    }

    #[test]
    fn only_unique_is_highlighted_and_never_autoplays() {
        let unique = picker(FileSelection::Unique(1));
        assert_eq!(unique.highlighted(), Some(1));
        assert_eq!(unique.visible(), vec![1]);
        assert_eq!(unique.status(1), RowStatus::Suggested);
        assert_eq!(unique.confirm(), Some((7, "b".into())));

        let ambiguous = picker(FileSelection::Ambiguous(vec![0, 2]));
        assert_eq!(ambiguous.highlighted(), None);
        assert_eq!(ambiguous.confirm(), None);
        assert_eq!(ambiguous.visible(), vec![0, 2]);
        assert_eq!(ambiguous.status(0), RowStatus::Possible);

        let none = picker(FileSelection::NotFound);
        assert_eq!(none.highlighted(), None);
        // The largest file (index 2) is never chosen for the user.
        assert_eq!(none.confirm(), None);
        assert_eq!(none.visible(), vec![0, 1, 2]);
        assert_eq!(none.status(2), RowStatus::Unmatched);
    }

    #[test]
    fn invalid_or_duplicate_indices_are_ignored() {
        let p = picker(FileSelection::Unique(9));
        assert_eq!(p.highlighted(), None);
        assert!(!p.is_unique());
        assert_eq!(p.visible(), vec![0, 1, 2]);
        let p = picker(FileSelection::Ambiguous(vec![1, 1, 8]));
        assert_eq!(p.visible(), vec![1]);
    }

    #[test]
    fn selection_is_limited_to_visible_files() {
        let mut p = picker(FileSelection::Ambiguous(vec![0, 2]));
        assert!(!p.select(1));
        assert!(p.select(2));
        assert_eq!(p.confirm(), Some((7, "c".into())));
        p.set_show_all(true);
        assert!(p.select(1));
        p.set_show_all(false);
        // Hidden highlight is dropped rather than silently kept.
        assert_eq!(p.highlighted(), None);
    }

    #[test]
    fn play_requires_choice_and_blocks_double_submit() {
        let mut model = PlaybackModel::default();
        assert_eq!(model.play(), None);
        model.show_prepared(picker(FileSelection::Ambiguous(vec![0, 2])));
        assert_eq!(model.play(), None);
        model.picker.as_mut().map(|p| p.select(0));
        assert_eq!(
            model.play(),
            Some(PlaybackIntent::PlayFile {
                preparation: 7,
                file_id: "a".into()
            })
        );
        assert!(model.busy.is_some());
        assert_eq!(model.play(), None);
    }

    #[test]
    fn errors_and_notices_retain_session_picker_and_usage() {
        let mut model = PlaybackModel::default();
        model.show_prepared(picker(FileSelection::Unique(0)));
        model.show_started(
            PlaybackSession {
                id: "s1".into(),
                url: "http://127.0.0.1:5000/media/s1".into(),
            },
            "x.mkv".into(),
        );
        model.show_usage(UsageSnapshot {
            reserved_bytes: 100,
            limit_bytes: 1000,
            downloaded_bytes: 40,
        });
        model.show_busy("working".into());
        model.show_error("boom".into());
        assert!(model.busy.is_none());
        assert_eq!(model.error.as_deref(), Some("boom"));
        assert!(model.session.is_some() && model.picker.is_some() && model.usage.is_some());
        model.show_limit_needed(5000, 1000);
        assert!(model.error.is_none());
        assert_eq!(
            model.notice,
            Some(CacheNotice::LimitNeeded {
                size: 5000,
                limit: 1000
            })
        );
        assert!(model.session.is_some() && model.usage.is_some());
        model.show_cache_full("Cache is full.".into());
        assert!(matches!(model.notice, Some(CacheNotice::Full(_))));
        assert_eq!(model.begin_release().as_deref(), Some("s1"));
        assert_eq!(model.begin_release(), None);
        assert!(model.session.is_some(), "stays until confirmed");
        assert!(model.released("s1"));
        assert!(!model.released("s1"));
    }

    #[test]
    fn local_urls_only() {
        assert!(safe_local_url("http://127.0.0.1:8080/media/abc").is_some());
        assert!(safe_local_url("http://[::1]:8080/media/abc").is_some());
        assert!(safe_local_url("https://127.0.0.1/x").is_none());
        assert!(safe_local_url("http://localhost:1/x").is_none());
        assert!(safe_local_url("https://cdn.example.com/f?token=secret").is_none());
        assert!(safe_local_url("http://user:pw@127.0.0.1/x").is_none());
        assert!(safe_local_url("not a url").is_none());
        let mut model = PlaybackModel::default();
        model.show_started(
            PlaybackSession {
                id: "s".into(),
                url: "https://cdn.example.com/f?token=secret".into(),
            },
            "f".into(),
        );
        assert_eq!(model.session.and_then(|s| s.local_url), None);
    }

    #[test]
    fn usage_reports_acquired_bytes_not_watch_progress() {
        let line = usage_line(&UsageSnapshot {
            reserved_bytes: 2048,
            limit_bytes: 4096,
            downloaded_bytes: 1024,
        });
        assert_eq!(line.fraction, Some(0.5));
        assert!(!line.over_limit);
        assert!(line.text.contains("1.0 KiB") && line.text.contains("2.0 KiB"));
        let empty = usage_line(&UsageSnapshot {
            reserved_bytes: 0,
            limit_bytes: 10,
            downloaded_bytes: 0,
        });
        assert_eq!(empty.fraction, None);
        let over = usage_line(&UsageSnapshot {
            reserved_bytes: 10,
            limit_bytes: 5,
            downloaded_bytes: 99,
        });
        assert!(over.over_limit);
        assert_eq!(over.fraction, Some(1.0));
    }

    #[test]
    fn byte_formatting() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(20 * 1024 * 1024 * 1024), "20.0 GiB");
        assert!(format_bytes(u64::MAX).ends_with("TiB"));
    }

    fn started(model: &mut PlaybackModel, id: &str) {
        model.show_started(
            PlaybackSession {
                id: id.into(),
                url: format!("http://127.0.0.1:1/media/{id}"),
            },
            "f.mkv".into(),
        );
    }

    #[test]
    fn release_rejection_keeps_session_and_allows_retry() {
        let mut model = PlaybackModel::default();
        started(&mut model, "s1");
        assert_eq!(model.begin_release().as_deref(), Some("s1"));
        assert!(model.session.as_ref().is_some_and(|s| s.releasing));
        // Submit rejected by backpressure or a worker error.
        assert!(model.release_failed("s1", "queue full".into()));
        assert_eq!(model.error.as_deref(), Some("queue full"));
        assert!(model.session.as_ref().is_some_and(|s| !s.releasing));
        assert_eq!(model.begin_release().as_deref(), Some("s1"));
        assert!(model.released("s1"));
        assert!(model.session.is_none());
    }

    #[test]
    fn other_sessions_do_not_affect_the_displayed_one() {
        let mut model = PlaybackModel::default();
        started(&mut model, "s2");
        assert!(!model.released("s1"));
        assert!(!model.release_failed("s1", "x".into()));
        assert!(!model.media_error("s1", "old session failed".into()));
        assert!(model.error.is_none());
        assert_eq!(model.session_id(), Some("s2"));
        assert!(model.media_error("s2", "upstream failed".into()));
        assert_eq!(model.error.as_deref(), Some("upstream failed"));
        assert!(model.session.is_some(), "async error keeps the session");
    }

    #[test]
    fn invalidation_clears_prepared_files_and_the_exact_session() {
        let mut model = PlaybackModel::default();
        model.show_prepared(picker(FileSelection::Unique(0)));
        started(&mut model, "s1");
        model.show_usage(UsageSnapshot {
            reserved_bytes: 1,
            limit_bytes: 2,
            downloaded_bytes: 1,
        });
        model.clear_prepared();
        assert!(model.picker.is_none());
        assert!(!model.clear_session("other"));
        assert!(model.clear_session("s1"));
        assert!(model.session.is_none());
        assert!(model.usage.is_some());
    }

    #[test]
    fn limit_notice_clears_on_settings_change_or_next_action() {
        let mut model = PlaybackModel::default();
        model.show_limit_needed(10, 5);
        model.settings_changed();
        assert!(model.notice.is_none());
        model.show_limit_needed(10, 5);
        model.show_busy("Preparing".into());
        assert!(model.notice.is_none());
        // A cache-full notice is not a limit notice and is kept.
        model.show_cache_full("full".into());
        model.settings_changed();
        assert!(matches!(model.notice, Some(CacheNotice::Full(_))));
    }
}
