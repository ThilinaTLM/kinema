//! Settings panel: provider, player and cache-limit preferences plus debrid credentials.
//!
//! The view performs no I/O. It emits [`SettingsIntent`]s for the application worker and
//! only shows "saved" when a worker snapshot matches the form; failures arrive through
//! [`SettingsView::set_feedback`]. Loaded tokens are never read, stored or displayed: the
//! masked inputs only hold text the user is typing, and are cleared as soon as it is emitted.

use std::path::PathBuf;

use gpui_kit::base::Disableable as _;
use gpui_kit::component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    radio::{Radio, RadioGroup},
};
use gpui_kit::*;
use kinema_core::sources::{DebridKind, IndexerKind};

use crate::{
    app::CredentialState,
    credentials::SecretString,
    player::{PlayerChoice, validate_choice},
    settings::{MetadataProvider, Settings},
    ui::strip::{NoticeTone, notice},
};

const GIB: u64 = 1024 * 1024 * 1024;
/// Longest backend-provided keyring description shown; the backend text is expected to be safe.
const MAX_DETAIL_CHARS: usize = 160;

/// Requests for the application worker. Nothing here has taken effect when emitted.
#[derive(Debug)]
pub enum SettingsIntent {
    /// Persist validated settings. Confirmation is a new snapshot or feedback.
    Save(Settings),
    /// Store a new token in the OS keyring. The view has already cleared its input.
    SetCredential {
        provider: DebridKind,
        value: SecretString,
    },
    DeleteCredential(DebridKind),
    /// Verify the token already stored in the keyring (never unsaved input) with the provider.
    TestCredential(DebridKind),
    /// Manually remove cached media not used by any session.
    CleanupUnused,
    /// Store a new TMDB API Read Access Token (bearer) in the OS keyring. Input already cleared.
    SetTmdbCredential {
        value: SecretString,
    },
    DeleteTmdbCredential,
    /// Verify the Read Access Token already stored in the keyring, never unsaved input.
    TestTmdbCredential,
}

const TMDB_API_SETTINGS_URL: &str = "https://www.themoviedb.org/settings/api";
const TMDB_AUTH_DOCS_URL: &str = "https://developer.themoviedb.org/docs/authentication-application";
/// Wording required by TMDB's terms for applications using its API.
pub const TMDB_ATTRIBUTION: &str =
    "This product uses the TMDB API but is not endorsed or certified by TMDB.";

// ---------------------------------------------------------------------------------------
// Pure model
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaError {
    Empty,
    Invalid,
    TooPrecise,
    Zero,
    TooLarge,
}

impl QuotaError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Empty => "Enter a cache limit in GiB.",
            Self::Invalid => "Use digits with an optional decimal point, for example 20 or 7.5.",
            Self::TooPrecise => "Use at most three decimal places.",
            Self::Zero => "The cache limit must be greater than zero.",
            Self::TooLarge => "That cache limit is too large.",
        }
    }
}

/// Converts decimal GiB text to bytes exactly (fractions truncate to whole bytes).
pub fn parse_quota_gib(text: &str) -> Result<u64, QuotaError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(QuotaError::Empty);
    }
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
        return Err(QuotaError::Invalid);
    }
    if fraction.len() > 3 {
        return Err(QuotaError::TooPrecise);
    }
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| QuotaError::TooLarge)?
    };
    let fraction_value: u128 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().map_err(|_| QuotaError::Invalid)?
    };
    let scale = 10u128.pow(fraction.len() as u32);
    let fraction_bytes = u64::try_from(fraction_value * u128::from(GIB) / scale)
        .map_err(|_| QuotaError::TooLarge)?;
    let bytes = whole
        .checked_mul(GIB)
        .and_then(|b| b.checked_add(fraction_bytes))
        .ok_or(QuotaError::TooLarge)?;
    if bytes == 0 {
        return Err(QuotaError::Zero);
    }
    Ok(bytes)
}

/// Display text for a byte limit, up to three decimals (truncating).
pub fn format_gib(bytes: u64) -> String {
    let whole = bytes / GIB;
    let milli = u128::from(bytes % GIB) * 1000 / u128::from(GIB);
    if milli == 0 {
        return whole.to_string();
    }
    let text = format!("{whole}.{milli:03}");
    text.trim_end_matches('0').to_owned()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerKind {
    Mpv,
    Vlc,
    Custom,
}

impl PlayerKind {
    const ALL: [Self; 3] = [Self::Mpv, Self::Vlc, Self::Custom];
    fn label(self) -> &'static str {
        match self {
            Self::Mpv => "mpv",
            Self::Vlc => "VLC",
            Self::Custom => "Custom player",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormError {
    Quota(QuotaError),
    PlayerPathEmpty,
    PlayerPathNotAbsolute,
}

impl FormError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Quota(error) => error.message(),
            Self::PlayerPathEmpty => "Enter the full path of the player executable.",
            Self::PlayerPathNotAbsolute => {
                "The custom player must be an absolute path to an executable."
            }
        }
    }
}

/// Editable copy of [`Settings`]. Text fields stay text until validated by [`Self::build`].
#[derive(Clone, Debug)]
pub struct SettingsForm {
    pub indexer: IndexerKind,
    pub debrid: DebridKind,
    pub metadata: MetadataProvider,
    pub quota_text: String,
    pub player: PlayerKind,
    /// Kept while another player kind is selected so switching back does not lose it.
    pub custom_path: String,
    baseline_quota_text: String,
    baseline_quota_bytes: u64,
}

impl SettingsForm {
    pub fn from_settings(settings: &Settings) -> Self {
        let (player, custom_path) = match &settings.player {
            PlayerChoice::Mpv => (PlayerKind::Mpv, String::new()),
            PlayerChoice::Vlc => (PlayerKind::Vlc, String::new()),
            PlayerChoice::Custom(path) => (PlayerKind::Custom, path.to_string_lossy().into_owned()),
        };
        let quota_text = format_gib(settings.media_limit_bytes);
        Self {
            indexer: settings.indexer,
            debrid: settings.debrid,
            metadata: settings.metadata,
            baseline_quota_text: quota_text.clone(),
            baseline_quota_bytes: settings.media_limit_bytes,
            quota_text,
            player,
            custom_path,
        }
    }

    /// Validates the edits. An untouched quota keeps its exact byte value even when the
    /// displayed GiB text had to be rounded.
    pub fn build(&self) -> Result<Settings, FormError> {
        let media_limit_bytes = if self.quota_text.trim() == self.baseline_quota_text {
            self.baseline_quota_bytes
        } else {
            parse_quota_gib(&self.quota_text).map_err(FormError::Quota)?
        };
        let player = match self.player {
            PlayerKind::Mpv => PlayerChoice::Mpv,
            PlayerKind::Vlc => PlayerChoice::Vlc,
            PlayerKind::Custom => {
                let text = self.custom_path.trim();
                if text.is_empty() {
                    return Err(FormError::PlayerPathEmpty);
                }
                let choice = PlayerChoice::Custom(PathBuf::from(text));
                validate_choice(&choice).map_err(|_| FormError::PlayerPathNotAbsolute)?;
                choice
            }
        };
        Ok(Settings {
            indexer: self.indexer,
            debrid: self.debrid,
            metadata: self.metadata,
            media_limit_bytes,
            player,
        })
    }

    /// True when the form differs from `saved`, including edits that are not yet valid.
    pub fn is_dirty(&self, saved: &Settings) -> bool {
        match self.build() {
            Ok(built) => !same_settings(&built, saved),
            Err(_) => true,
        }
    }
}

fn same_settings(a: &Settings, b: &Settings) -> bool {
    a.indexer == b.indexer
        && a.debrid == b.debrid
        && a.metadata == b.metadata
        && a.media_limit_bytes == b.media_limit_bytes
        && a.player == b.player
}

/// Whether a new snapshot should replace the user's form: yes when the form was clean, or
/// when the form already equals the new snapshot (a confirmed save). Otherwise edits win.
pub fn should_reset_form(form: &SettingsForm, old_saved: &Settings, new_saved: &Settings) -> bool {
    !form.is_dirty(old_saved) || !form.is_dirty(new_saved)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SaveStatus {
    Saved,
    Unsaved,
    Saving,
    Invalid(FormError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialTone {
    Good,
    Neutral,
    Problem,
}

/// Distinct text for a present token, a missing token, and unavailable secure storage.
pub fn credential_summary(state: Option<&CredentialState>) -> (String, CredentialTone) {
    match state {
        None => ("Status not loaded yet.".into(), CredentialTone::Neutral),
        Some(CredentialState::Present) => (
            "A token is saved in the OS keyring. It is never shown here.".into(),
            CredentialTone::Good,
        ),
        Some(CredentialState::Missing) => (
            "No token saved. Paste one below and save it.".into(),
            CredentialTone::Neutral,
        ),
        Some(CredentialState::Unavailable(detail)) => {
            let detail: String = detail.chars().take(MAX_DETAIL_CHARS).collect();
            (
                format!(
                    "The OS keyring is unavailable ({detail}). The saved token cannot be checked."
                ),
                CredentialTone::Problem,
            )
        }
    }
}

pub fn provider_name(kind: DebridKind) -> &'static str {
    match kind {
        DebridKind::RealDebrid => "Real-Debrid",
        DebridKind::AllDebrid => "AllDebrid",
    }
}

fn metadata_name(kind: MetadataProvider) -> &'static str {
    match kind {
        MetadataProvider::Cinemeta => "Cinemeta",
        MetadataProvider::Tmdb => "TMDB",
    }
}

const METADATA: [MetadataProvider; 2] = [MetadataProvider::Cinemeta, MetadataProvider::Tmdb];

fn indexer_name(kind: IndexerKind) -> &'static str {
    match kind {
        IndexerKind::Torrentio => "Torrentio",
        IndexerKind::Peerflix => "Peerflix",
    }
}

const INDEXERS: [IndexerKind; 2] = [IndexerKind::Torrentio, IndexerKind::Peerflix];
const PROVIDERS: [DebridKind; 2] = [DebridKind::RealDebrid, DebridKind::AllDebrid];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Busy {
    SavingSettings,
    SavingToken(DebridKind),
    RemovingToken(DebridKind),
    Testing(DebridKind),
    SavingTmdb,
    RemovingTmdb,
    TestingTmdb,
    Cleaning,
}

impl Busy {
    fn label(self) -> String {
        match self {
            Self::SavingSettings => "Saving preferences…".into(),
            Self::SavingToken(p) => format!("Saving {} token…", provider_name(p)),
            Self::RemovingToken(p) => format!("Removing {} token…", provider_name(p)),
            Self::Testing(p) => format!("Testing {} connection…", provider_name(p)),
            Self::SavingTmdb => "Saving TMDB token…".into(),
            Self::RemovingTmdb => "Removing TMDB token…".into(),
            Self::TestingTmdb => "Testing TMDB connection…".into(),
            Self::Cleaning => "Removing unused cached media…".into(),
        }
    }
}

/// Whether a worker snapshot completes the pending operation. Settings saves complete only
/// when the snapshot equals what was sent. Token saves and removals complete on any snapshot,
/// because replacing a token or removing an absent one leaves the status unchanged. A
/// snapshot from an unrelated load may end the busy state early; that only re-enables buttons.
fn snapshot_completes(busy: Busy, sent: Option<&Settings>, snapshot: &Settings) -> bool {
    match busy {
        Busy::SavingSettings => sent.is_some_and(|sent| same_settings(sent, snapshot)),
        Busy::SavingToken(_) | Busy::RemovingToken(_) => true,
        Busy::SavingTmdb | Busy::RemovingTmdb | Busy::TestingTmdb => false,
        Busy::Testing(_) | Busy::Cleaning => false,
    }
}

/// A TMDB status refresh completes a pending TMDB save or removal, whatever the new status.
fn tmdb_status_completes(busy: Busy) -> bool {
    matches!(busy, Busy::SavingTmdb | Busy::RemovingTmdb)
}

// ---------------------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------------------

struct TokenField {
    kind: DebridKind,
    input: Entity<InputState>,
}

pub struct SettingsView {
    saved: Settings,
    form: SettingsForm,
    statuses: Vec<(DebridKind, CredentialState)>,
    quota_input: Entity<InputState>,
    path_input: Entity<InputState>,
    tokens: [TokenField; 2],
    tmdb_input: Entity<InputState>,
    tmdb_status: Option<CredentialState>,
    confirm_remove_tmdb: bool,
    /// Inputs must be rewritten from `form` at the next render (needs a `Window`).
    needs_input_sync: bool,
    busy: Option<Busy>,
    /// Settings value most recently sent, to recognise the confirming snapshot.
    sent: Option<Settings>,
    confirm_remove: Option<DebridKind>,
    feedback: Option<(String, bool)>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsIntent> for SettingsView {}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::build(window, cx))
    }

    fn build(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = Settings::default();
        let form = SettingsForm::from_settings(&saved);
        let quota_input = cx.new(|cx| InputState::new(window, cx).placeholder("20"));
        let path_input = cx
            .new(|cx| InputState::new(window, cx).placeholder("Absolute path, e.g. /usr/bin/mpv"));
        let token = |window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("Paste a new token")
            })
        };
        let tokens = PROVIDERS.map(|kind| TokenField {
            kind,
            input: token(window, cx),
        });
        let tmdb_input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Paste a new Read Access Token")
        });
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(
            &tmdb_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } => this.save_tmdb(window, cx),
                _ => {}
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &quota_input,
            window,
            |this, input, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.form.quota_text = input.read(cx).value().to_string();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.save(cx),
                _ => {}
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &path_input,
            window,
            |this, input, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.form.custom_path = input.read(cx).value().to_string();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.save(cx),
                _ => {}
            },
        ));
        for field in &tokens {
            let kind = field.kind;
            subscriptions.push(cx.subscribe_in(
                &field.input,
                window,
                move |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.save_token(kind, window, cx),
                    _ => {}
                },
            ));
        }
        Self {
            saved,
            form,
            statuses: Vec::new(),
            quota_input,
            path_input,
            tokens,
            tmdb_input,
            tmdb_status: None,
            confirm_remove_tmdb: false,
            needs_input_sync: true,
            busy: None,
            sent: None,
            confirm_remove: None,
            feedback: None,
            _subscriptions: subscriptions,
        }
    }

    /// Controlled update from the worker. Unrelated updates never overwrite user edits.
    pub fn set_snapshot(
        &mut self,
        settings: Settings,
        statuses: Vec<(DebridKind, CredentialState)>,
        cx: &mut Context<Self>,
    ) {
        if should_reset_form(&self.form, &self.saved, &settings) {
            self.form = SettingsForm::from_settings(&settings);
            self.needs_input_sync = true;
        }
        if self
            .busy
            .is_some_and(|busy| snapshot_completes(busy, self.sent.as_ref(), &settings))
        {
            self.busy = None;
            self.sent = None;
        }
        self.saved = settings;
        self.statuses = statuses;
        cx.notify();
    }

    /// Controlled TMDB keyring status. Never touches unsaved preference edits. A pending token
    /// save or removal ends here even when the status is unchanged (replace, or delete of
    /// an absent token).
    pub fn set_tmdb_status(&mut self, state: CredentialState, cx: &mut Context<Self>) {
        if self.busy.is_some_and(tmdb_status_completes) {
            self.busy = None;
        }
        self.tmdb_status = Some(state);
        cx.notify();
    }

    /// The worker reported the stored TMDB token was accepted.
    pub fn tmdb_credential_checked(&mut self, cx: &mut Context<Self>) {
        self.set_feedback(
            "TMDB accepted the saved Read Access Token. Connection works.".into(),
            false,
            cx,
        );
    }

    /// Ends the pending operation without a message, for a success the snapshot already shows.
    pub fn finish_operation(&mut self, cx: &mut Context<Self>) {
        self.busy = None;
        self.sent = None;
        cx.notify();
    }

    /// Worker outcome text. Clears any pending operation. `message` must already be safe.
    pub fn set_feedback(&mut self, message: String, is_error: bool, cx: &mut Context<Self>) {
        self.busy = None;
        self.sent = None;
        self.feedback = Some((message, is_error));
        cx.notify();
    }

    /// Worker reported `CredentialChecked { provider }`: the stored token was accepted.
    pub fn credential_checked(&mut self, provider: DebridKind, cx: &mut Context<Self>) {
        self.set_feedback(
            format!(
                "{} accepted the saved token. Connection works.",
                provider_name(provider)
            ),
            false,
            cx,
        );
    }

    fn save_status(&self) -> SaveStatus {
        if self.busy == Some(Busy::SavingSettings) {
            return SaveStatus::Saving;
        }
        match self.form.build() {
            Err(error) => SaveStatus::Invalid(error),
            Ok(built) if same_settings(&built, &self.saved) => SaveStatus::Saved,
            Ok(_) => SaveStatus::Unsaved,
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let Ok(settings) = self.form.build() else {
            cx.notify();
            return;
        };
        if same_settings(&settings, &self.saved) {
            return;
        }
        self.feedback = None;
        self.busy = Some(Busy::SavingSettings);
        self.sent = Some(settings.clone());
        cx.emit(SettingsIntent::Save(settings));
        cx.notify();
    }

    fn discard(&mut self, cx: &mut Context<Self>) {
        self.form = SettingsForm::from_settings(&self.saved);
        self.needs_input_sync = true;
        self.feedback = None;
        cx.notify();
    }

    fn token_input(&self, kind: DebridKind) -> &Entity<InputState> {
        let index = self.tokens.iter().position(|t| t.kind == kind).unwrap_or(0);
        &self.tokens[index].input
    }

    fn save_token(&mut self, kind: DebridKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let input = self.token_input(kind).clone();
        let text = input.read(cx).value().trim().to_owned();
        if text.is_empty() {
            self.feedback = Some(("Paste a token before saving it.".into(), true));
            cx.notify();
            return;
        }
        let value = SecretString::new(text);
        input.update(cx, |input, cx| input.set_value("", window, cx));
        self.feedback = None;
        self.busy = Some(Busy::SavingToken(kind));
        cx.emit(SettingsIntent::SetCredential {
            provider: kind,
            value,
        });
        cx.notify();
    }

    fn save_tmdb(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let input = self.tmdb_input.clone();
        let text = input.read(cx).value().trim().to_owned();
        if text.is_empty() {
            self.feedback = Some(("Paste a Read Access Token before saving it.".into(), true));
            cx.notify();
            return;
        }
        let value = SecretString::new(text);
        input.update(cx, |input, cx| input.set_value("", window, cx));
        self.feedback = None;
        self.busy = Some(Busy::SavingTmdb);
        cx.emit(SettingsIntent::SetTmdbCredential { value });
        cx.notify();
    }

    fn request_remove_tmdb(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        if self.confirm_remove_tmdb {
            self.confirm_remove_tmdb = false;
            self.feedback = None;
            self.busy = Some(Busy::RemovingTmdb);
            cx.emit(SettingsIntent::DeleteTmdbCredential);
        } else {
            self.confirm_remove_tmdb = true;
        }
        cx.notify();
    }

    fn test_tmdb(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.feedback = None;
        self.busy = Some(Busy::TestingTmdb);
        cx.emit(SettingsIntent::TestTmdbCredential);
        cx.notify();
    }

    fn request_remove(&mut self, kind: DebridKind, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        if self.confirm_remove == Some(kind) {
            self.confirm_remove = None;
            self.feedback = None;
            self.busy = Some(Busy::RemovingToken(kind));
            cx.emit(SettingsIntent::DeleteCredential(kind));
        } else {
            self.confirm_remove = Some(kind);
        }
        cx.notify();
    }

    fn test(&mut self, kind: DebridKind, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.feedback = None;
        self.busy = Some(Busy::Testing(kind));
        cx.emit(SettingsIntent::TestCredential(kind));
        cx.notify();
    }

    fn cleanup(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.feedback = None;
        self.busy = Some(Busy::Cleaning);
        cx.emit(SettingsIntent::CleanupUnused);
        cx.notify();
    }

    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.needs_input_sync) {
            return;
        }
        let quota = self.form.quota_text.clone();
        let path = self.form.custom_path.clone();
        for (input, text) in [(&self.quota_input, quota), (&self.path_input, path)] {
            if input.read(cx).value().as_ref() != text.as_str() {
                input.update(cx, |input, cx| input.set_value(text.as_str(), window, cx));
            }
        }
    }

    // ----- rendering helpers -----

    fn section(
        &self,
        title: &'static str,
        description: &'static str,
        body: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Div {
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
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child(description),
            )
            .child(body)
    }

    fn field_label(&self, text: &'static str) -> Div {
        div().text_size(px(12.)).child(text)
    }

    fn preferences(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.busy.is_some();
        let indexer_ix = INDEXERS.iter().position(|i| *i == self.form.indexer);
        let provider_ix = PROVIDERS.iter().position(|p| *p == self.form.debrid);
        let player_ix = PlayerKind::ALL.iter().position(|p| *p == self.form.player);
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        let indexer = RadioGroup::horizontal("settings-indexer")
            .disabled(busy)
            .selected_index(indexer_ix)
            .children(INDEXERS.map(|i| {
                Radio::new(indexer_name(i))
                    .min_h(px(CONTROL_HEIGHT))
                    .text_size(px(13.))
                    .label(indexer_name(i))
                    .accessibility_label(format!("Source indexer: {}", indexer_name(i)))
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                if let Some(kind) = INDEXERS.get(*ix) {
                    this.form.indexer = *kind;
                    cx.notify();
                }
            }));
        let provider = RadioGroup::horizontal("settings-debrid")
            .disabled(busy)
            .selected_index(provider_ix)
            .children(PROVIDERS.map(|p| {
                Radio::new(provider_name(p))
                    .min_h(px(CONTROL_HEIGHT))
                    .text_size(px(13.))
                    .label(provider_name(p))
                    .accessibility_label(format!("Debrid provider: {}", provider_name(p)))
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                if let Some(kind) = PROVIDERS.get(*ix) {
                    this.form.debrid = *kind;
                    cx.notify();
                }
            }));
        let metadata_ix = METADATA.iter().position(|m| *m == self.form.metadata);
        let metadata = RadioGroup::horizontal("settings-metadata")
            .disabled(busy)
            .selected_index(metadata_ix)
            .children(METADATA.map(|m| {
                Radio::new(metadata_name(m))
                    .min_h(px(CONTROL_HEIGHT))
                    .text_size(px(13.))
                    .label(metadata_name(m))
                    .accessibility_label(format!("Metadata provider: {}", metadata_name(m)))
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                if let Some(kind) = METADATA.get(*ix) {
                    this.form.metadata = *kind;
                    cx.notify();
                }
            }));
        let tmdb_missing = self.form.metadata == MetadataProvider::Tmdb
            && !matches!(self.tmdb_status, Some(CredentialState::Present));
        let player = RadioGroup::horizontal("settings-player")
            .disabled(busy)
            .selected_index(player_ix)
            .children(PlayerKind::ALL.map(|p| {
                Radio::new(p.label())
                    .min_h(px(CONTROL_HEIGHT))
                    .text_size(px(13.))
                    .label(p.label())
                    .accessibility_label(format!("External player: {}", p.label()))
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                if let Some(kind) = PlayerKind::ALL.get(*ix) {
                    this.form.player = *kind;
                    cx.notify();
                }
            }));

        let quota_error = parse_for_display(&self.form);
        let path_error = (self.form.player == PlayerKind::Custom)
            .then(|| match self.form.build() {
                Err(e @ (FormError::PlayerPathEmpty | FormError::PlayerPathNotAbsolute)) => {
                    Some(e.message())
                }
                _ => None,
            })
            .flatten();

        let status = self.save_status();
        let (status_text, status_color) = match &status {
            SaveStatus::Saved => ("All preferences are saved.".to_owned(), cx.theme().success),
            SaveStatus::Unsaved => (
                "Unsaved changes. They apply only after you save.".to_owned(),
                muted,
            ),
            SaveStatus::Saving => ("Saving preferences…".to_owned(), muted),
            SaveStatus::Invalid(error) => (error.message().to_owned(), danger),
        };
        let dirty = !matches!(status, SaveStatus::Saved);
        let can_save = matches!(status, SaveStatus::Unsaved) && !busy;

        let path_note = match path_error {
            Some(message) => note_text(message, danger),
            None => note_text(
                "Only used with Custom player. mpv and VLC are found automatically.",
                muted,
            ),
        };
        let path_block = self.field_row(
            "Custom player executable",
            Input::new(&self.path_input)
                .h(px(CONTROL_HEIGHT))
                .disabled(busy || self.form.player != PlayerKind::Custom),
            path_note,
        );
        let quota_note = match quota_error {
            Some(message) => note_text(message, danger),
            None => note_text(
                "Must be positive. Files larger than the limit cannot be cached; lowering it below the space already reserved is refused.",
                muted,
            ),
        };
        let quota_block = self.field_row(
            "Media cache limit (GiB)",
            div().max_w(px(220.)).child(
                Input::new(&self.quota_input)
                    .h(px(CONTROL_HEIGHT))
                    .disabled(busy),
            ),
            quota_note,
        );

        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.labeled(
                "Source indexer",
                "Where torrent sources are searched.",
                indexer,
                muted,
            ))
            .child({
                let mut block = self.labeled(
                    "Metadata provider",
                    "Source of titles, posters and episodes. Cinemeta is the default and needs no \
                     account; TMDB needs its own token below. There is no automatic fallback: the \
                     provider you choose is the only one used.",
                    metadata,
                    muted,
                );
                if tmdb_missing {
                    block = block.child(self.field_row(
                        "",
                        note_text(
                            "TMDB is selected but no working token is saved. Save and test a Read \
                             Access Token below, or choose Cinemeta.",
                            danger,
                        ),
                        div(),
                    ));
                }
                block
            })
            .child(self.labeled(
                "Debrid provider",
                "The service that prepares torrents. It needs its own saved token below.",
                provider,
                muted,
            ))
            .child(self.labeled(
                "External player",
                "Launched with a local loopback address.",
                player,
                muted,
            ))
            .child(path_block)
            .child(quota_block)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .ml(px(LABEL_WIDTH + 12.))
                    .child(
                        Button::new("settings-save")
                            .primary()
                            .h(px(CONTROL_HEIGHT))
                            .label("Save preferences")
                            .disabled(!can_save)
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    )
                    .child(
                        Button::new("settings-discard")
                            .outline()
                            .h(px(CONTROL_HEIGHT))
                            .label("Discard changes")
                            .disabled(!dirty || busy)
                            .on_click(cx.listener(|this, _, _, cx| this.discard(cx))),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(status_color)
                            .child(status_text),
                    ),
            );
        self.section(
            "Preferences",
            "Provider, player and cache choices. Changes are applied only after saving.",
            body,
            cx,
        )
    }

    /// Aligned settings row: fixed label column, flexible control, 11px note.
    fn field_row(
        &self,
        title: &'static str,
        control: impl IntoElement,
        note: impl IntoElement,
    ) -> Div {
        div()
            .flex()
            .items_start()
            .gap_3()
            .child(
                div()
                    .w(px(LABEL_WIDTH))
                    .flex_none()
                    .min_h(px(if title.is_empty() { 0. } else { CONTROL_HEIGHT }))
                    .flex()
                    .items_center()
                    .child(self.field_label(title)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(control)
                    .child(note),
            )
    }

    fn labeled(
        &self,
        title: &'static str,
        hint: &'static str,
        control: impl IntoElement,
        muted: Hsla,
    ) -> Div {
        self.field_row(title, control, note_text(hint, muted))
    }

    fn credential_section(&self, kind: DebridKind, cx: &mut Context<Self>) -> Div {
        let state = self
            .statuses
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, s)| s);
        let (summary, tone) = credential_summary(state);
        let tone_color = match tone {
            CredentialTone::Good => cx.theme().success,
            CredentialTone::Neutral => cx.theme().muted_foreground,
            CredentialTone::Problem => cx.theme().danger,
        };
        let busy = self.busy.is_some();
        let present = matches!(state, Some(CredentialState::Present));
        let name = provider_name(kind);
        let input = self.token_input(kind).clone();
        let typed = !input.read(cx).value().trim().is_empty();
        let removing = self.confirm_remove == Some(kind);
        let active = self.form.debrid == kind;
        let key = if kind == DebridKind::RealDebrid {
            "rd"
        } else {
            "ad"
        };

        let mut buttons = div().flex().flex_wrap().items_center().gap_2();
        buttons = buttons
            .child(
                Button::new(SharedString::from(format!("token-save-{key}")))
                    .primary()
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label("Save token")
                    .disabled(busy || !typed)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.save_token(kind, window, cx)),
                    ),
            )
            .child(
                Button::new(SharedString::from(format!("token-test-{key}")))
                    .outline()
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label("Test connection")
                    .disabled(busy || !present)
                    .on_click(cx.listener(move |this, _, _, cx| this.test(kind, cx))),
            )
            .child(
                Button::new(SharedString::from(format!("token-remove-{key}")))
                    .outline()
                    .text_color(cx.theme().danger)
                    .border_color(cx.theme().danger)
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label(if removing {
                        "Confirm remove"
                    } else {
                        "Remove token"
                    })
                    .disabled(busy || !present)
                    .on_click(cx.listener(move |this, _, _, cx| this.request_remove(kind, cx))),
            );
        if removing {
            buttons = buttons.child(
                Button::new(SharedString::from(format!("token-keep-{key}")))
                    .ghost()
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label("Keep token")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_remove = None;
                        cx.notify();
                    })),
            );
        }

        let mut column = div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if active {
                        format!("{name} (selected provider)")
                    } else {
                        name.to_owned()
                    }),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(tone_color)
                    .child(summary),
            )
            .child(
                self.field_row(
                    "New token",
                    Input::new(&input)
                        .h(px(CONTROL_HEIGHT))
                        .mask_toggle()
                        .disabled(busy),
                    buttons,
                ),
            );
        if typed {
            column = column.child(
                div()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child("This text is not saved yet, and Test connection ignores it."),
            );
        }
        column
    }

    fn credentials(&self, cx: &mut Context<Self>) -> Div {
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Save stores the pasted token in the OS keyring and clears the field. \
                         Test connection then checks the token already saved there with the provider. \
                         Saving does not test, and tokens are never displayed or written to settings.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(self.credential_section(DebridKind::RealDebrid, cx))
                    .child(self.credential_section(DebridKind::AllDebrid, cx)),
            );
        self.section(
            "Debrid credentials",
            "API tokens for Real-Debrid and AllDebrid.",
            body,
            cx,
        )
    }

    fn tmdb_section(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let (summary, tone) = credential_summary(self.tmdb_status.as_ref());
        let tone_color = match tone {
            CredentialTone::Good => cx.theme().success,
            CredentialTone::Neutral => muted,
            CredentialTone::Problem => cx.theme().danger,
        };
        let busy = self.busy.is_some();
        let present = matches!(self.tmdb_status, Some(CredentialState::Present));
        let typed = !self.tmdb_input.read(cx).value().trim().is_empty();
        let removing = self.confirm_remove_tmdb;
        let selected = self.form.metadata == MetadataProvider::Tmdb;

        let mut buttons = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                Button::new("tmdb-save")
                    .primary()
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label("Save token")
                    .disabled(busy || !typed)
                    .on_click(cx.listener(|this, _, window, cx| this.save_tmdb(window, cx))),
            )
            .child(
                Button::new("tmdb-test")
                    .outline()
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label("Test connection")
                    .disabled(busy || !present)
                    .on_click(cx.listener(|this, _, _, cx| this.test_tmdb(cx))),
            )
            .child(
                Button::new("tmdb-remove")
                    .outline()
                    .text_color(cx.theme().danger)
                    .border_color(cx.theme().danger)
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label(if removing {
                        "Confirm remove"
                    } else {
                        "Remove token"
                    })
                    .disabled(busy || !present)
                    .on_click(cx.listener(|this, _, _, cx| this.request_remove_tmdb(cx))),
            );
        if removing {
            buttons = buttons.child(
                Button::new("tmdb-keep")
                    .ghost()
                    .small()
                    .h(px(24.))
                    .text_size(px(12.))
                    .label("Keep token")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_remove_tmdb = false;
                        cx.notify();
                    })),
            );
        }
        let mut column = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if selected {
                        "TMDB API Read Access Token (selected provider)"
                    } else {
                        "TMDB API Read Access Token"
                    }),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(tone_color)
                    .child(summary),
            )
            .child(
                self.field_row(
                    "New Read Access Token",
                    Input::new(&self.tmdb_input)
                        .h(px(CONTROL_HEIGHT))
                        .mask_toggle()
                        .disabled(busy),
                    buttons,
                ),
            );
        if typed {
            column = column.child(
                div()
                    .text_size(px(12.))
                    .text_color(muted)
                    .child("This text is not saved yet, and Test connection ignores it."),
            );
        }
        let help = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_size(px(12.)).text_color(muted).child(
                "Use the long \"API Read Access Token\" from your TMDB account, which is sent as \
                 a bearer token in the Authorization header. Do not paste the short \"API Key\": \
                 it is a different credential and is not used here. Save stores the token in the \
                 OS keyring and clears the field; Test connection then checks the saved token. \
                 It is never shown or written to settings.",
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("tmdb-open-settings")
                            .ghost()
                            .small()
                            .h(px(24.))
                            .text_size(px(12.))
                            .label("Open TMDB API settings")
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.open_url(TMDB_API_SETTINGS_URL);
                            })),
                    )
                    .child(
                        Button::new("tmdb-open-docs")
                            .ghost()
                            .small()
                            .h(px(24.))
                            .text_size(px(12.))
                            .label("How TMDB authentication works")
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.open_url(TMDB_AUTH_DOCS_URL);
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(muted)
                    .child(TMDB_ATTRIBUTION),
            );
        self.section(
            "Metadata credentials (TMDB)",
            "Optional. Needed only when TMDB is the selected metadata provider.",
            div().flex().flex_col().gap_3().child(help).child(column),
            cx,
        )
    }

    fn storage(&self, cx: &mut Context<Self>) -> Div {
        let body = div().flex().flex_wrap().items_center().gap_3().child(
            Button::new("settings-cleanup")
                .h(px(CONTROL_HEIGHT))
                .outline()
                .label("Remove unused cached media")
                .disabled(self.busy.is_some())
                .on_click(cx.listener(|this, _, _, cx| this.cleanup(cx))),
        );
        self.section(
            "Storage",
            "Cleanup is manual and removes only cached files that no active session uses. Nothing is evicted automatically.",
            body,
            cx,
        )
    }
}

/// Quota validation message to show inline, if the quota text itself is invalid.
fn parse_for_display(form: &SettingsForm) -> Option<&'static str> {
    match form.build() {
        Err(FormError::Quota(error)) => Some(error.message()),
        Err(_) => parse_quota_gib(&form.quota_text)
            .err()
            .filter(|_| form.quota_text.trim() != form.baseline_quota_text)
            .map(QuotaError::message),
        Ok(_) => None,
    }
}

/// Ordinary control height and settings label column width (design system).
const CONTROL_HEIGHT: f32 = 28.0;
const LABEL_WIDTH: f32 = 160.0;

fn note_text(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .text_size(px(11.))
        .text_color(color)
        .child(text.into())
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(window, cx);
        let mut root = div()
            .id("settings-view")
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
                    .child("Settings"),
            );
        if let Some(busy) = self.busy {
            root = root.child(notice("settings-busy", busy.label(), NoticeTone::Info, cx));
        }
        if let Some((message, is_error)) = &self.feedback {
            root = root.child(if *is_error {
                notice("settings-feedback", message.clone(), NoticeTone::Error, cx)
            } else {
                notice(
                    "settings-feedback",
                    message.clone(),
                    NoticeTone::Success,
                    cx,
                )
            });
        }
        root.child(self.preferences(cx))
            .child(self.credentials(cx))
            .child(self.tmdb_section(cx))
            .child(self.storage(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` also exports a `test` attribute; keep the standard one.
    use std::prelude::v1::test;

    #[test]
    fn quota_parses_exact_values() {
        assert_eq!(parse_quota_gib("20"), Ok(20 * GIB));
        assert_eq!(parse_quota_gib(" 0.5 "), Ok(GIB / 2));
        assert_eq!(parse_quota_gib(".25"), Ok(GIB / 4));
        assert_eq!(parse_quota_gib("1.000"), Ok(GIB));
        assert_eq!(parse_quota_gib("0.001"), Ok(GIB / 1000));
    }

    #[test]
    fn quota_rejects_bad_input() {
        assert_eq!(parse_quota_gib(""), Err(QuotaError::Empty));
        assert_eq!(parse_quota_gib("  "), Err(QuotaError::Empty));
        assert_eq!(parse_quota_gib("0"), Err(QuotaError::Zero));
        assert_eq!(parse_quota_gib("0.000"), Err(QuotaError::Zero));
        assert_eq!(parse_quota_gib("-1"), Err(QuotaError::Invalid));
        assert_eq!(parse_quota_gib("+1"), Err(QuotaError::Invalid));
        assert_eq!(parse_quota_gib("1e3"), Err(QuotaError::Invalid));
        assert_eq!(parse_quota_gib("."), Err(QuotaError::Invalid));
        assert_eq!(parse_quota_gib("1,5"), Err(QuotaError::Invalid));
        assert_eq!(parse_quota_gib("1.0001"), Err(QuotaError::TooPrecise));
    }

    #[test]
    fn quota_overflow_is_rejected_not_wrapped() {
        let max_whole = u64::MAX / GIB;
        assert_eq!(parse_quota_gib(&max_whole.to_string()), Ok(max_whole * GIB));
        assert_eq!(
            parse_quota_gib(&(max_whole + 1).to_string()),
            Err(QuotaError::TooLarge)
        );
        assert_eq!(
            parse_quota_gib("99999999999999999999999"),
            Err(QuotaError::TooLarge)
        );
    }

    #[test]
    fn gib_formatting() {
        assert_eq!(format_gib(20 * GIB), "20");
        assert_eq!(format_gib(GIB / 2), "0.5");
        assert_eq!(format_gib(GIB + GIB / 4), "1.25");
    }

    fn settings(limit: u64, player: PlayerChoice) -> Settings {
        Settings {
            media_limit_bytes: limit,
            player,
            ..Settings::default()
        }
    }

    #[test]
    fn untouched_inexact_quota_keeps_exact_bytes() {
        let saved = settings(GIB + 1, PlayerChoice::Mpv);
        let form = SettingsForm::from_settings(&saved);
        assert_eq!(
            form.build().map(|s| s.media_limit_bytes).ok(),
            Some(GIB + 1)
        );
        assert!(!form.is_dirty(&saved));
    }

    #[test]
    fn form_validates_player_paths() {
        let mut form = SettingsForm::from_settings(&Settings::default());
        form.player = PlayerKind::Custom;
        assert_eq!(form.build().err(), Some(FormError::PlayerPathEmpty));
        form.custom_path = "relative/player".into();
        assert_eq!(form.build().err(), Some(FormError::PlayerPathNotAbsolute));
        let absolute = std::env::temp_dir().join("my player");
        form.custom_path = format!("  {}\n", absolute.display());
        assert_eq!(
            form.build().map(|s| s.player).ok(),
            Some(PlayerChoice::Custom(absolute))
        );
    }

    #[test]
    fn hidden_custom_path_is_ignored_for_builtin_players() {
        let saved = Settings::default();
        let mut form = SettingsForm::from_settings(&saved);
        form.custom_path = "garbage".into();
        assert!(!form.is_dirty(&saved));
        form.player = PlayerKind::Vlc;
        assert!(form.is_dirty(&saved));
    }

    #[test]
    fn invalid_edit_counts_as_dirty() {
        let saved = Settings::default();
        let mut form = SettingsForm::from_settings(&saved);
        form.quota_text = "0".into();
        assert!(form.is_dirty(&saved));
        assert_eq!(form.build().err(), Some(FormError::Quota(QuotaError::Zero)));
    }

    #[test]
    fn snapshots_do_not_clobber_edits_but_confirm_saves() {
        let old = Settings::default();
        let mut form = SettingsForm::from_settings(&old);
        // Clean form follows any snapshot.
        let other = settings(5 * GIB, PlayerChoice::Mpv);
        assert!(should_reset_form(&form, &old, &other));
        // Dirty form keeps edits on an unrelated snapshot.
        form.indexer = IndexerKind::Peerflix;
        assert!(!should_reset_form(&form, &old, &other));
        // But accepts the snapshot that equals its edits.
        let confirmed = Settings {
            indexer: IndexerKind::Peerflix,
            ..old.clone()
        };
        assert!(should_reset_form(&form, &old, &confirmed));
    }

    #[test]
    fn credential_states_read_differently() {
        let (present, t1) = credential_summary(Some(&CredentialState::Present));
        let (missing, t2) = credential_summary(Some(&CredentialState::Missing));
        let (unavailable, t3) =
            credential_summary(Some(&CredentialState::Unavailable("no service".into())));
        assert!(present != missing && missing != unavailable && present != unavailable);
        assert!(unavailable.contains("no service") && unavailable.contains("unavailable"));
        assert_eq!(
            (t1, t2, t3),
            (
                CredentialTone::Good,
                CredentialTone::Neutral,
                CredentialTone::Problem
            )
        );
        assert_eq!(credential_summary(None).1, CredentialTone::Neutral);
    }

    #[test]
    fn same_state_token_operations_complete_on_snapshot() {
        let current = Settings::default();
        // Present->Present replacement and Missing->Missing delete change no status.
        for busy in [
            Busy::SavingToken(DebridKind::RealDebrid),
            Busy::RemovingToken(DebridKind::AllDebrid),
        ] {
            assert!(snapshot_completes(busy, None, &current));
        }
    }

    #[test]
    fn settings_save_waits_for_the_matching_snapshot() {
        let sent = settings(5 * GIB, PlayerChoice::Mpv);
        let old = Settings::default();
        assert!(!snapshot_completes(Busy::SavingSettings, Some(&sent), &old));
        assert!(!snapshot_completes(Busy::SavingSettings, None, &sent));
        assert!(snapshot_completes(Busy::SavingSettings, Some(&sent), &sent));
    }

    #[test]
    fn tmdb_token_busy_completes_via_status_not_settings_snapshot() {
        let current = Settings::default();
        // Settings snapshots do not end TMDB operations; set_tmdb_status does, even when the
        // status is the same (replace Present->Present, delete Missing->Missing).
        for busy in [Busy::SavingTmdb, Busy::RemovingTmdb, Busy::TestingTmdb] {
            assert!(!snapshot_completes(busy, None, &current));
        }
        assert!(tmdb_status_completes(Busy::SavingTmdb));
        assert!(tmdb_status_completes(Busy::RemovingTmdb));
        assert!(!tmdb_status_completes(Busy::TestingTmdb));
        assert!(!tmdb_status_completes(Busy::Cleaning));
    }

    #[test]
    fn tmdb_intents_and_debug_never_expose_the_token() {
        let intent = SettingsIntent::SetTmdbCredential {
            value: SecretString::new("eyJ-super-secret-token".into()),
        };
        assert!(!format!("{intent:?}").contains("super-secret"));
        for busy in [Busy::SavingTmdb, Busy::RemovingTmdb, Busy::TestingTmdb] {
            assert!(!busy.label().contains("secret"));
        }
        assert!(TMDB_ATTRIBUTION.starts_with("This product uses the TMDB API"));
    }

    #[test]
    fn metadata_choice_round_trips_and_default_is_cinemeta() {
        let saved = Settings::default();
        assert_eq!(saved.metadata, MetadataProvider::Cinemeta);
        let mut form = SettingsForm::from_settings(&saved);
        assert!(!form.is_dirty(&saved));
        form.metadata = MetadataProvider::Tmdb;
        assert!(form.is_dirty(&saved));
        let built = form.build().map(|s| s.metadata).ok();
        assert_eq!(built, Some(MetadataProvider::Tmdb));
        // Existing preferences are preserved when only the metadata choice changes.
        let old = Settings {
            indexer: IndexerKind::Peerflix,
            debrid: DebridKind::AllDebrid,
            media_limit_bytes: 7 * GIB,
            player: PlayerChoice::Vlc,
            ..Settings::default()
        };
        let mut form = SettingsForm::from_settings(&old);
        form.metadata = MetadataProvider::Tmdb;
        let rebuilt = form.build().ok();
        assert!(rebuilt.is_some_and(|s| s.indexer == IndexerKind::Peerflix
            && s.debrid == DebridKind::AllDebrid
            && s.media_limit_bytes == 7 * GIB
            && s.player == PlayerChoice::Vlc
            && s.metadata == MetadataProvider::Tmdb));
    }

    #[test]
    fn token_status_refresh_does_not_reset_unsaved_edits() {
        // Status refreshes never feed should_reset_form; only settings snapshots do.
        let saved = Settings::default();
        let mut form = SettingsForm::from_settings(&saved);
        form.metadata = MetadataProvider::Tmdb;
        let unrelated = Settings {
            media_limit_bytes: 3 * GIB,
            ..Settings::default()
        };
        assert!(!should_reset_form(&form, &saved, &unrelated));
    }

    #[test]
    fn test_and_cleanup_need_explicit_outcomes() {
        let current = Settings::default();
        assert!(!snapshot_completes(
            Busy::Testing(DebridKind::RealDebrid),
            None,
            &current
        ));
        assert!(!snapshot_completes(Busy::Cleaning, None, &current));
    }
}
