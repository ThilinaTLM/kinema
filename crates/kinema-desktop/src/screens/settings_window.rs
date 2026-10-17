//! Direct-GPUI Settings/About. Bootstrap calls `install` once with the shared
//! main-window coordinator. No worker, keyring, filesystem or player is created by this view.
use crate::screens::coordinator::{AppCoordinator, CoordinatorEvent};
use crate::{
    app::{AppAction, AppOutcome, CredentialState},
    credentials::SecretString,
    player::PlayerChoice,
    settings::{MetadataProvider, MotionChoice, PosterSize, Settings, ThemeChoice},
    widgets::{
        button::Button,
        menu::{Select, SelectEvent},
        segmented::{self, Segmented},
        switch::Switch,
        text::TypeExt,
        text_input::{TextInput, TextInputEvent},
        tokens::{self, space as s, typeface as t},
    },
};
use gpui::{prelude::*, *};
use kinema_core::sources::{DebridKind, IndexerKind};
use std::{collections::HashMap, path::PathBuf};

actions!(settings_window, [NextFocus, PreviousFocus]);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NextFocus, Some("KinemaSettings")),
        KeyBinding::new("shift-tab", PreviousFocus, Some("KinemaSettings")),
    ]);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    General,
    Accounts,
    Playback,
    Storage,
    Appearance,
    About,
}
impl Section {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Accounts,
        Self::Playback,
        Self::Storage,
        Self::Appearance,
        Self::About,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Accounts => "Accounts",
            Self::Playback => "Playback",
            Self::Storage => "Storage",
            Self::Appearance => "Appearance",
            Self::About => "About",
        }
    }
    fn navigate(self, key: &str) -> Option<Self> {
        let index = Self::ALL.iter().position(|s| *s == self)?;
        segmented::navigate(index, Self::ALL.len(), key).map(|i| Self::ALL[i])
    }
}

#[derive(Default)]
struct SettingsWindows {
    window: Option<WindowHandle<SettingsWindow>>,
    installed: bool,
    pending_save: Option<u64>,
}
impl Global for SettingsWindows {}

/// Subscribe BEFORE submitting commands. Call once with the shared coordinator.
/// Settings close has no quit/session handler; the shared coordinator outlives it.
pub fn install(coordinator: &Entity<AppCoordinator>, cx: &mut App) {
    if !cx.has_global::<SettingsWindows>() {
        cx.set_global(SettingsWindows::default());
    }
    if cx.global::<SettingsWindows>().installed {
        return;
    }
    cx.global_mut::<SettingsWindows>().installed = true;
    bind_keys(cx);
    cx.subscribe(coordinator, |coordinator, event, cx| match event {
        CoordinatorEvent::OpenSettings => open(coordinator, Section::General, cx),
        CoordinatorEvent::About => open(coordinator, Section::About, cx),
        CoordinatorEvent::App(event) => {
            if cx.global::<SettingsWindows>().pending_save == Some(event.generation) {
                cx.global_mut::<SettingsWindows>().pending_save = None;
                // This survives closing Settings while a save is in flight.
                if let Some(settings) = &coordinator.read(cx).settings {
                    tokens::apply_preferences(settings.theme, settings.reduce_motion, cx);
                }
            }
        }
    })
    .detach();
    let mut previous = coordinator
        .read(cx)
        .settings
        .as_ref()
        .map(|s| (s.theme, s.reduce_motion));
    cx.observe(coordinator, move |coordinator, cx| {
        if cx.global::<SettingsWindows>().pending_save.is_some() {
            return;
        }
        if let Some(settings) = &coordinator.read(cx).settings {
            let preferences = (settings.theme, settings.reduce_motion);
            if previous != Some(preferences) {
                previous = Some(preferences);
                tokens::apply_preferences(preferences.0, preferences.1, cx);
            }
        }
    })
    .detach();
    if let Some(settings) = &coordinator.read(cx).settings {
        tokens::apply_preferences(settings.theme, settings.reduce_motion, cx);
    }
    // Defer so this can also be installed while MainWindow is being built.
    cx.defer(|cx| {
        for handle in cx.windows() {
            let _ = handle.update(cx, |_, window, _| {
                tokens::observe_appearance(window).detach()
            });
        }
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenDecision {
    Activate,
    Create,
}
fn open_decision(existing_window_is_live: bool) -> OpenDecision {
    if existing_window_is_live {
        OpenDecision::Activate
    } else {
        OpenDecision::Create
    }
}

pub fn open(coordinator: Entity<AppCoordinator>, section: Section, cx: &mut App) {
    if !cx.has_global::<SettingsWindows>() {
        cx.set_global(SettingsWindows::default());
    }
    let handle = cx.global::<SettingsWindows>().window;
    let activated = handle.is_some_and(|handle| {
        handle
            .update(cx, |view, window, cx| {
                view.section = section;
                window.activate_window();
                view.sidebar.focus(window, cx);
                cx.notify();
            })
            .is_ok()
    });
    if open_decision(activated) == OpenDecision::Activate {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(860.), px(600.)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Kinema Settings".into()),
            ..Default::default()
        }),
        window_min_size: Some(size(px(720.), px(480.))),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        cx.new(|cx| SettingsWindow::new(coordinator, section, window, cx))
    }) {
        Ok(handle) => cx.global_mut::<SettingsWindows>().window = Some(handle),
        Err(error) => tracing::error!(%error, "Could not open Settings window"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Account {
    Tmdb,
    RealDebrid,
    AllDebrid,
}
impl Account {
    const ALL: [Self; 3] = [Self::Tmdb, Self::RealDebrid, Self::AllDebrid];
    fn label(self) -> &'static str {
        match self {
            Self::Tmdb => "TMDB token",
            Self::RealDebrid => "Real-Debrid",
            Self::AllDebrid => "AllDebrid",
        }
    }
    fn provider(self) -> Option<DebridKind> {
        match self {
            Self::Tmdb => None,
            Self::RealDebrid => Some(DebridKind::RealDebrid),
            Self::AllDebrid => Some(DebridKind::AllDebrid),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum CredentialOperation {
    Save,
    Remove,
    Test,
}
#[derive(Clone, Copy)]
enum Pending {
    Save(&'static str),
    Credential(Account, CredentialOperation),
    Cleanup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountStatus {
    Loading,
    Missing,
    Saved,
    Verified,
    Rejected,
    ServiceFailure,
    Unavailable,
}
impl AccountStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Loading => "Loading status…",
            Self::Missing => "Missing",
            Self::Saved => "Saved (not verified)",
            Self::Verified => "Verified",
            Self::Rejected => "Rejected",
            Self::ServiceFailure => "Service failure",
            Self::Unavailable => "Secure storage unavailable",
        }
    }
    fn snapshot(state: Option<&CredentialState>) -> Self {
        match state {
            None => Self::Loading,
            Some(CredentialState::Present) => Self::Saved,
            Some(CredentialState::Missing) => Self::Missing,
            Some(CredentialState::Unavailable(_)) => Self::Unavailable,
        }
    }
    fn failure(message: &str) -> Self {
        // The gateway currently returns text errors, not typed credential failures.
        // Compare exact public adapter messages; unknown errors remain failures.
        if message == crate::providers::DebridError::CredentialRejected.to_string()
            || message == crate::tmdb::TmdbError::RejectedCredential.to_string()
        {
            Self::Rejected
        } else if message == crate::credentials::CredentialError::Missing.to_string() {
            Self::Missing
        } else if message == crate::credentials::CredentialError::Unavailable.to_string()
            || message == crate::credentials::CredentialError::AccessDenied.to_string()
        {
            Self::Unavailable
        } else {
            Self::ServiceFailure
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerKind {
    Mpv,
    Vlc,
    Custom,
}
#[derive(Clone, Copy)]
enum SettingChange {
    Theme(ThemeChoice),
    Poster(PosterSize),
    Motion(MotionChoice),
    Metadata(MetadataProvider),
    Indexer(IndexerKind),
    Debrid(DebridKind),
}
fn apply_change(settings: &mut Settings, change: SettingChange) {
    match change {
        SettingChange::Theme(v) => settings.theme = v,
        SettingChange::Poster(v) => settings.poster_size = v,
        SettingChange::Motion(v) => settings.reduce_motion = v,
        SettingChange::Metadata(v) => settings.metadata = v,
        SettingChange::Indexer(v) => settings.indexer = v,
        SettingChange::Debrid(v) => settings.debrid = v,
    }
}

pub struct SettingsWindow {
    coordinator: Entity<AppCoordinator>,
    section: Section,
    sidebar: FocusHandle,
    settings: Settings,
    settings_loaded: bool,
    metadata: Entity<Select<MetadataProvider>>,
    indexer: Entity<Select<IndexerKind>>,
    debrid: Entity<Select<DebridKind>>,
    player: Entity<Select<PlayerKind>>,
    path: Entity<TextInput>,
    limit: Entity<TextInput>,
    secret: Entity<TextInput>,
    editing: Option<Account>,
    status: HashMap<Account, AccountStatus>,
    snapshots: HashMap<Account, Option<CredentialState>>,
    errors: HashMap<&'static str, String>,
    pending: HashMap<u64, Pending>,
    latest_account: HashMap<Account, u64>,
    save_pending: bool,
    fixture: bool,
    subscriptions: Vec<Subscription>,
}
impl SettingsWindow {
    pub fn new(
        coordinator: Entity<AppCoordinator>,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = coordinator.read(cx).settings.clone().unwrap_or_default();
        let metadata = cx.new(|cx| {
            Select::new(
                settings.metadata,
                vec![
                    (MetadataProvider::Cinemeta, "Cinemeta".into()),
                    (MetadataProvider::Tmdb, "TMDB".into()),
                ],
                cx,
            )
        });
        let indexer = cx.new(|cx| {
            Select::new(
                settings.indexer,
                vec![
                    (IndexerKind::Torrentio, "Torrentio".into()),
                    (IndexerKind::Peerflix, "Peerflix".into()),
                ],
                cx,
            )
        });
        let debrid = cx.new(|cx| {
            Select::new(
                settings.debrid,
                vec![
                    (DebridKind::RealDebrid, "Real-Debrid".into()),
                    (DebridKind::AllDebrid, "AllDebrid".into()),
                ],
                cx,
            )
        });
        let kind = match settings.player {
            PlayerChoice::Mpv => PlayerKind::Mpv,
            PlayerChoice::Vlc => PlayerKind::Vlc,
            PlayerChoice::Custom(_) => PlayerKind::Custom,
        };
        let player = cx.new(|cx| {
            Select::new(
                kind,
                vec![
                    (PlayerKind::Mpv, "mpv".into()),
                    (PlayerKind::Vlc, "VLC".into()),
                    (PlayerKind::Custom, "Custom path".into()),
                ],
                cx,
            )
        });
        let path = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_placeholder("Absolute executable path", cx);
            if let PlayerChoice::Custom(path) = &settings.player {
                input.set_text(path.to_string_lossy().into_owned(), cx);
            }
            input
        });
        let limit = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_text(
                (settings.media_limit_bytes / 1024 / 1024 / 1024).to_string(),
                cx,
            );
            input
        });
        let secret = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_secret(true, cx);
            input.set_placeholder("New token — stored only in OS secure storage", cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe(&coordinator, |this, _, event, cx| {
                if let CoordinatorEvent::App(event) = event {
                    this.outcome(event, cx);
                }
            }),
            cx.observe(&coordinator, |this, _, cx| {
                this.sync(cx);
                cx.notify();
            }),
            cx.subscribe(&metadata, |this, _, event, cx| {
                let SelectEvent::Changed(value) = event;
                this.change("Metadata", SettingChange::Metadata(*value), cx);
            }),
            cx.subscribe(&indexer, |this, _, event, cx| {
                let SelectEvent::Changed(value) = event;
                this.change("Streams", SettingChange::Indexer(*value), cx);
            }),
            cx.subscribe(&debrid, |this, _, event, cx| {
                let SelectEvent::Changed(value) = event;
                this.change("Debrid", SettingChange::Debrid(*value), cx);
            }),
            cx.subscribe(&player, |this, _, _, cx| this.apply_player(cx)),
            cx.subscribe(&path, |this, _, event, cx| {
                if matches!(event, TextInputEvent::Submitted | TextInputEvent::Blurred) {
                    this.apply_player(cx);
                }
            }),
            cx.subscribe(&limit, |this, _, event, cx| {
                if matches!(event, TextInputEvent::Submitted | TextInputEvent::Blurred) {
                    this.apply_limit(cx);
                }
            }),
            cx.subscribe(&secret, |this, _, event, cx| {
                if matches!(event, TextInputEvent::Submitted) {
                    this.submit_secret(cx);
                }
            }),
            tokens::observe_appearance(window),
        ];
        let settings_loaded = coordinator.read(cx).settings.is_some();
        let mut this = Self {
            coordinator,
            section,
            sidebar: cx.focus_handle().tab_stop(true),
            settings,
            settings_loaded,
            metadata,
            indexer,
            debrid,
            player,
            path,
            limit,
            secret,
            editing: None,
            status: HashMap::new(),
            snapshots: HashMap::new(),
            errors: HashMap::new(),
            pending: HashMap::new(),
            latest_account: HashMap::new(),
            save_pending: false,
            fixture: false,
            subscriptions,
        };
        this.sync(cx);
        this.sidebar.focus(window, cx);
        this
    }
    fn sync(&mut self, cx: &mut Context<Self>) {
        let state = self.coordinator.read(cx);
        for account in Account::ALL {
            if self.latest_account.contains_key(&account) {
                continue;
            }
            let snapshot = if let Some(provider) = account.provider() {
                state
                    .credentials
                    .iter()
                    .find(|(p, _)| *p == provider)
                    .map(|(_, s)| s)
            } else {
                state.tmdb_credential.as_ref()
            };
            let snapshot = snapshot.cloned();
            if self.snapshots.get(&account) != Some(&snapshot) {
                self.status
                    .insert(account, AccountStatus::snapshot(snapshot.as_ref()));
                self.snapshots.insert(account, snapshot);
            }
        }
        if !self.save_pending
            && let Some(settings) = state.settings.clone()
        {
            self.settings = settings;
            if !self.settings_loaded {
                self.settings_loaded = true;
                self.limit.update(cx, |input, cx| {
                    input.set_text(
                        (self.settings.media_limit_bytes / 1024 / 1024 / 1024).to_string(),
                        cx,
                    )
                });
                let kind = match &self.settings.player {
                    PlayerChoice::Mpv => PlayerKind::Mpv,
                    PlayerChoice::Vlc => PlayerKind::Vlc,
                    PlayerChoice::Custom(path) => {
                        self.path.update(cx, |input, cx| {
                            input.set_text(path.to_string_lossy().into_owned(), cx)
                        });
                        PlayerKind::Custom
                    }
                };
                self.player
                    .update(cx, |select, cx| select.set_value(kind, cx));
            }
            self.metadata.update(cx, |select, cx| {
                select.set_value(self.settings.metadata, cx)
            });
            self.indexer
                .update(cx, |select, cx| select.set_value(self.settings.indexer, cx));
            self.debrid
                .update(cx, |select, cx| select.set_value(self.settings.debrid, cx));
        }
    }
    fn change(&mut self, field: &'static str, change: SettingChange, cx: &mut Context<Self>) {
        apply_change(&mut self.settings, change);
        self.save(field, cx);
    }
    fn save(&mut self, field: &'static str, cx: &mut Context<Self>) {
        self.errors.remove(field);
        if !self.settings_loaded && !self.fixture {
            self.errors.insert(
                field,
                "Settings are still loading or unavailable; no settings were saved.".into(),
            );
            cx.notify();
            return;
        }
        tokens::apply_preferences(self.settings.theme, self.settings.reduce_motion, cx);
        if self.fixture {
            cx.notify();
            return;
        }
        // Submit every change immediately, even if this window is about to
        // close. The coordinator/worker owns completion; this view only accepts
        // its latest save's outcome (older durable snapshots remain authoritative
        // in the coordinator until a newer save actually completes).
        if let Some(g) = self.coordinator.update(cx, |state, cx| {
            state.command(AppAction::SaveSettings(self.settings.clone()), cx)
        }) {
            self.pending
                .retain(|_, pending| !matches!(pending, Pending::Save(_)));
            self.pending.insert(g, Pending::Save(field));
            self.save_pending = true;
            if cx.has_global::<SettingsWindows>() {
                cx.global_mut::<SettingsWindows>().pending_save = Some(g);
            }
        } else {
            self.errors.insert(
                field,
                "Application service unavailable or busy. Retry this change.".into(),
            );
        }
        cx.notify();
    }
    fn apply_player(&mut self, cx: &mut Context<Self>) {
        let choice = match self.player.read(cx).value() {
            PlayerKind::Mpv => PlayerChoice::Mpv,
            PlayerKind::Vlc => PlayerChoice::Vlc,
            PlayerKind::Custom => PlayerChoice::Custom(PathBuf::from(self.path.read(cx).text())),
        };
        if let Err(error) = crate::player::validate_choice(&choice) {
            self.errors.insert("Player", error.to_string());
            cx.notify();
            return;
        }
        self.errors.remove("Player");
        if self.settings.player == choice {
            cx.notify();
            return;
        }
        self.settings.player = choice;
        self.save("Player", cx);
    }
    fn apply_limit(&mut self, cx: &mut Context<Self>) {
        let bytes = self
            .limit
            .read(cx)
            .text()
            .parse::<u64>()
            .ok()
            .and_then(|v| v.checked_mul(1024 * 1024 * 1024))
            .filter(|v| *v > 0);
        let Some(bytes) = bytes else {
            self.errors.insert(
                "Cache limit",
                "Enter a positive whole number of GiB.".into(),
            );
            cx.notify();
            return;
        };
        self.errors.remove("Cache limit");
        if bytes == self.settings.media_limit_bytes {
            cx.notify();
            return;
        }
        self.settings.media_limit_bytes = bytes;
        self.save("Cache limit", cx);
    }
    fn credential(
        &mut self,
        account: Account,
        operation: CredentialOperation,
        value: Option<SecretString>,
        cx: &mut Context<Self>,
    ) {
        if self.fixture {
            self.status.insert(
                account,
                match operation {
                    CredentialOperation::Save => AccountStatus::Saved,
                    CredentialOperation::Remove => AccountStatus::Missing,
                    CredentialOperation::Test => AccountStatus::Verified,
                },
            );
            cx.notify();
            return;
        }
        let action = match (account.provider(), operation, value) {
            (Some(provider), CredentialOperation::Save, Some(value)) => {
                AppAction::SetCredential { provider, value }
            }
            (None, CredentialOperation::Save, Some(value)) => {
                AppAction::SetTmdbCredential { value }
            }
            (_, CredentialOperation::Save, None) => return,
            (Some(provider), CredentialOperation::Remove, _) => {
                AppAction::DeleteCredential(provider)
            }
            (None, CredentialOperation::Remove, _) => AppAction::DeleteTmdbCredential,
            (Some(provider), CredentialOperation::Test, _) => AppAction::TestCredential(provider),
            (None, CredentialOperation::Test, _) => AppAction::TestTmdbCredential,
        };
        self.errors.remove(account.label());
        if let Some(g) = self
            .coordinator
            .update(cx, |state, cx| state.command(action, cx))
        {
            self.pending
                .insert(g, Pending::Credential(account, operation));
            self.latest_account.insert(account, g);
        } else {
            self.errors.insert(
                account.label(),
                "Application service unavailable or busy. Retry.".into(),
            );
        }
        cx.notify();
    }
    fn submit_secret(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.editing.take() else {
            return;
        };
        let value = self.secret.update(cx, take_secret);
        if value.expose().trim().is_empty() {
            self.errors
                .insert(account.label(), "Token cannot be empty.".into());
            cx.notify();
            return;
        }
        self.credential(account, CredentialOperation::Save, Some(value), cx);
    }
    fn outcome(&mut self, event: &crate::app::AppEvent, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.remove(&event.generation) else {
            return;
        };
        let failure = match &event.outcome {
            AppOutcome::Failed { message } | AppOutcome::SettingsWarning { message, .. } => {
                Some(message.clone())
            }
            AppOutcome::Busy { label } => Some(format!("Busy: {label}")),
            _ => None,
        };
        match pending {
            Pending::Save(field) => {
                self.save_pending = false;
                if let Some(message) = failure {
                    self.errors.insert(field, message);
                }
                // Reconcile the preview with the accepted snapshot. Durability
                // warnings carry the newly replaced settings and are not rolled
                // back; rejected saves restore the last accepted appearance.
                self.sync(cx);
                tokens::apply_preferences(self.settings.theme, self.settings.reduce_motion, cx);
            }
            Pending::Credential(account, operation) => {
                if self.latest_account.get(&account) != Some(&event.generation) {
                    return;
                }
                self.latest_account.remove(&account);
                let state = self.coordinator.read(cx);
                let snapshot = if let Some(provider) = account.provider() {
                    state
                        .credentials
                        .iter()
                        .find(|(p, _)| *p == provider)
                        .map(|(_, s)| s.clone())
                } else {
                    state.tmdb_credential.clone()
                };
                self.snapshots.insert(account, snapshot);
                let status = if let Some(message) = failure {
                    let status = AccountStatus::failure(&message);
                    self.errors.insert(account.label(), message);
                    status
                } else {
                    match operation {
                        CredentialOperation::Save => AccountStatus::Saved,
                        CredentialOperation::Remove => AccountStatus::Missing,
                        CredentialOperation::Test => AccountStatus::Verified,
                    }
                };
                self.status.insert(account, status);
            }
            Pending::Cleanup => {
                self.errors.insert("Cleanup", failure.unwrap_or_else(|| match &event.outcome { AppOutcome::Cleaned { bytes } => format!("Freed {}. Active/protected files were kept; original local files are never removed.", bytes_label(*bytes)), _ => "Cleanup completed.".into() }));
            }
        }
        cx.notify();
    }
    fn account_row(&self, account: Account, cx: &Context<Self>) -> AnyElement {
        let p = *tokens::palette(cx);
        let status = self
            .status
            .get(&account)
            .copied()
            .unwrap_or(AccountStatus::Loading);
        let ink = match status {
            AccountStatus::Verified => p.status_ok,
            AccountStatus::Saved => p.status_warn,
            AccountStatus::Rejected
            | AccountStatus::ServiceFailure
            | AccountStatus::Unavailable => p.status_error,
            AccountStatus::Loading | AccountStatus::Missing => p.text_muted,
        };
        let pending = self.latest_account.contains_key(&account);
        let weak = cx.entity().downgrade();
        let remove = weak.clone();
        let test = weak.clone();
        div()
            .flex()
            .flex_col()
            .gap(s::S8)
            .pb(s::S16)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S8)
                    .child(
                        div()
                            .w(s::S48 * 2. + s::S16)
                            .flex_none()
                            .child(account.label()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(s::S8)
                            .type_style(t::SMALL)
                            .text_color(ink)
                            .child(div().size(s::S8).flex_none().bg(ink))
                            .child(status.label()),
                    )
                    .child(
                        Button::new(
                            ("credential-edit", account as usize),
                            if status == AccountStatus::Missing {
                                "Add…"
                            } else {
                                "Replace"
                            },
                        )
                        .disabled(pending)
                        .on_click(move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.editing = Some(account);
                                this.secret.update(cx, |input, cx| {
                                    clear_secret(input, cx);
                                    input.focus(window, cx);
                                });
                                cx.notify();
                            });
                        }),
                    )
                    .child(
                        Button::new(("credential-remove", account as usize), "Remove")
                            .disabled(pending || status == AccountStatus::Missing)
                            .on_click(move |_, _, cx| {
                                let _ = remove.update(cx, |this, cx| {
                                    this.credential(account, CredentialOperation::Remove, None, cx)
                                });
                            }),
                    )
                    .child(
                        Button::new(("credential-test", account as usize), "Test")
                            .disabled(
                                pending
                                    || matches!(
                                        status,
                                        AccountStatus::Missing | AccountStatus::Loading
                                    ),
                            )
                            .on_click(move |_, _, cx| {
                                let _ = test.update(cx, |this, cx| {
                                    this.credential(account, CredentialOperation::Test, None, cx)
                                });
                            }),
                    ),
            )
            .into_any_element()
    }
    fn content(&self, cx: &Context<Self>) -> AnyElement {
        let p = *tokens::palette(cx);
        let weak = cx.entity().downgrade();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(s::S24)
            .child(div().type_style(t::TITLE).child(self.section.label()));
        match self.section {
            Section::General => {
                body = body.child(text("Startup collection: Movies · provider order", p.text_primary)).child(text("Search stays within the selected kind (Movies or Series). Provider collections are not configurable in this scaffold.", p.text_secondary));
            }
            Section::Accounts => {
                body = body
                    .child(row("Metadata", self.metadata.clone()))
                    .child(row("Streams", self.indexer.clone()))
                    .child(row("Debrid", self.debrid.clone()));
                let mut accounts = div()
                    .flex()
                    .flex_col()
                    .border_t(s::HAIRLINE)
                    .border_color(p.line_hairline)
                    .pt(s::S16)
                    .child(
                        div()
                            .mb(s::S16)
                            .type_style(t::SMALL_MEDIUM)
                            .text_color(p.text_secondary)
                            .child("Credentials"),
                    );
                for account in Account::ALL {
                    accounts = accounts.child(self.account_row(account, cx));
                }
                body = body.child(accounts);
                if let Some(account) = self.editing {
                    let cancel = weak.clone();
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(s::S8)
                            .p(s::S12)
                            .bg(p.bg_raised)
                            .border(s::HAIRLINE)
                            .border_color(p.line_hairline)
                            .child(format!("Replace {}", account.label()))
                            .child(div().w(s::CONTROL_HEIGHT * 10.).child(self.secret.clone()))
                            .child(
                                div()
                                    .flex()
                                    .gap(s::S8)
                                    .child(Button::new("submit-secret", "Store token").on_click(
                                        move |_, _, cx| {
                                            let _ =
                                                weak.update(cx, |this, cx| this.submit_secret(cx));
                                        },
                                    ))
                                    .child(Button::new("cancel-secret", "Cancel").on_click(
                                        move |_, _, cx| {
                                            let _ = cancel.update(cx, |this, cx| {
                                                this.editing = None;
                                                this.secret.update(cx, clear_secret);
                                                cx.notify();
                                            });
                                        },
                                    )),
                            ),
                    );
                }
            }
            Section::Playback => {
                body = body
                    .child(row("External player", self.player.clone()))
                    .child(row("Custom path", self.path.clone()))
                    .child(text(
                        "Path and quota edits apply on Enter or leaving the field.",
                        p.text_muted,
                    ))
                    .child(text(
                        "Argument preview (separate arguments; no shell):",
                        p.text_secondary,
                    ))
                    .child(
                        div()
                            .type_style(t::MONO)
                            .child(player_preview(&self.settings.player)),
                    );
            }
            Section::Storage => {
                let usage = self.coordinator.read(cx).usage.as_ref();
                body = body.child(text(usage.map(|u| format!("Downloaded {} · reserved {} · limit {}", bytes_label(u.downloaded_bytes), bytes_label(u.reserved_bytes), bytes_label(u.limit_bytes))).unwrap_or_else(|| "Cache usage unavailable (not an empty cache).".into()), p.text_secondary))
                    .child(row("Limit (GiB)", self.limit.clone()))
                    .child(Button::new("cleanup-unused", "Clean unused").disabled(self.pending.values().any(|p| matches!(p, Pending::Cleanup))).on_click(move |_, _, cx| { let _ = weak.update(cx, |this, cx| {
                        if this.fixture { this.errors.insert("Cleanup", "Fixture: freed 1.0 GiB; protected files kept.".into()); }
                        else if let Some(g) = this.coordinator.update(cx, |state, cx| state.command(AppAction::CleanupUnused, cx)) { this.pending.insert(g, Pending::Cleanup); }
                        else { this.errors.insert("Cleanup", "Application service unavailable or busy. Retry.".into()); }
                        cx.notify();
                    }); }))
                    .child(text("Only inactive managed media is removed. Original local files and protected sessions are kept.", p.text_muted))
                    .child(text("Location: application-managed OS cache directory. Exact path is not exposed by the current service API.", p.text_muted));
            }
            Section::Appearance => {
                let poster = weak.clone();
                let motion = weak.clone();
                body = body.child(row("Theme", Segmented::new("settings-theme", [(ThemeChoice::Dark, "Dark".into()), (ThemeChoice::Light, "Light".into()), (ThemeChoice::System, "System".into())], self.settings.theme).on_change(move |value, _, cx| { let _ = weak.update(cx, |this, cx| this.change("Theme", SettingChange::Theme(*value), cx)); })))
                    .child(row("Poster size", Segmented::new("settings-poster", [(PosterSize::Small, "S".into()), (PosterSize::Medium, "M".into()), (PosterSize::Large, "L".into())], self.settings.poster_size).on_change(move |value, _, cx| { let _ = poster.update(cx, |this, cx| this.change("Poster size", SettingChange::Poster(*value), cx)); })))
                    .child(row("Motion", Switch::new("settings-motion", "Reduce animations", self.settings.reduce_motion == MotionChoice::Always).on_change(move |value, _, cx| { let _ = motion.update(cx, |this, cx| this.change("Reduce motion", SettingChange::Motion(if value { MotionChoice::Always } else { MotionChoice::System }), cx)); })))
                    .child(text("Kinema can't read your system motion setting yet, so animations play unless this is on.", p.text_muted));
            }
            Section::About => {
                body = body.child(div().type_style(t::HEADING).child(concat!("Kinema ", env!("CARGO_PKG_VERSION"))))
                    .child(text("Pre-1.0 scaffold — not the complete MVP. Provider, playback and OS support are not implied by this interface.", p.text_secondary))
                    .child(text("Inter — SIL Open Font License 1.1\nJetBrains Mono — SIL Open Font License 1.1\nLucide — ISC License\nGPUI — Apache License 2.0", p.text_secondary))
                    .child(img("kinema/tmdb-logo.svg").w(s::S48 * 3.).h(s::S32).object_fit(ObjectFit::Contain))
                    .child(text("This product uses the TMDB API but is not endorsed or certified by TMDB.", p.text_secondary));
            }
        }
        for (field, message) in &self.errors {
            body = body.child(
                div()
                    .type_style(t::SMALL)
                    .text_color(if *field == "Cleanup" {
                        p.text_secondary
                    } else {
                        p.status_error
                    })
                    .child(format!("{field}: {message}")),
            );
        }
        if self.save_pending {
            body = body.child(text("Applying settings…", p.text_muted));
        }
        body.into_any_element()
    }
}
fn take_secret(input: &mut TextInput, cx: &mut Context<TextInput>) -> SecretString {
    let value = SecretString::new(input.text().to_owned());
    clear_secret(input, cx);
    value
}
fn clear_secret(input: &mut TextInput, cx: &mut Context<TextInput>) {
    // Force replacement even if the user deleted all text: set_text("") alone
    // is a no-op on an empty buffer and could leave secrets in undo history.
    input.set_text(" ", cx);
    input.set_text(String::new(), cx);
}
fn player_preview(player: &PlayerChoice) -> String {
    let executable = match player {
        PlayerChoice::Mpv => "mpv".into(),
        PlayerChoice::Vlc => "vlc".into(),
        PlayerChoice::Custom(path) => path.to_string_lossy(),
    };
    format!("[{executable:?}, \"--\", \"http://127.0.0.1:<port>/<local-media>\"]")
}
fn bytes_label(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1024. * 1024. * 1024.))
}
fn text(value: impl Into<SharedString>, color: Hsla) -> AnyElement {
    div()
        .text_color(color)
        .child(value.into())
        .into_any_element()
}
fn row(label: &'static str, control: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(s::S16)
        .child(div().w(s::S48 * 2. + s::S16).flex_none().child(label))
        .child(div().w(s::CONTROL_HEIGHT * 10.).flex_none().child(control))
        .into_any_element()
}
impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let sidebar_focus = self.sidebar.is_focused(window) && window.last_input_was_keyboard();
        let _ = self.subscriptions.len(); // Own all subscriptions for the view lifetime.
        let selected = self.section;
        div()
            .key_context("KinemaSettings")
            .on_action(|_: &NextFocus, window, cx| window.focus_next(cx))
            .on_action(|_: &PreviousFocus, window, cx| window.focus_prev(cx))
            .size_full()
            .flex()
            .bg(p.bg_canvas)
            .text_color(p.text_primary)
            .font_family(t::UI_FAMILY)
            .type_style(t::BODY)
            .child(
                div()
                    .id("settings-sidebar")
                    .track_focus(&self.sidebar)
                    .tab_stop(true)
                    .role(Role::TabList)
                    .aria_label("Settings sections")
                    .w(s::S48 * 3. + s::S32)
                    .h_full()
                    .flex_none()
                    .py(s::S16)
                    .bg(p.bg_surface)
                    .border_r(s::HAIRLINE)
                    .border_color(p.line_hairline)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if let Some(section) = this.section.navigate(&event.keystroke.key) {
                            this.section = section;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .children(Section::ALL.into_iter().map(|section| {
                        div()
                            .id(("settings-section", section as usize))
                            .role(Role::Tab)
                            .aria_selected(section == selected)
                            .h(s::ROW)
                            .px(s::S16)
                            .flex()
                            .items_center()
                            .border_l(s::S2)
                            .border_color(if section == selected {
                                p.accent
                            } else {
                                transparent_black()
                            })
                            .when(section == selected, |el| {
                                el.bg(p.accent_tint).text_color(p.accent_text)
                            })
                            .when(section == selected && sidebar_focus, |el| {
                                el.border(s::FOCUS_RING).border_color(p.focus_ring)
                            })
                            .hover(|el| el.bg(p.ctl_hover))
                            .child(section.label())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.section = section;
                                this.sidebar.focus(window, cx);
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .id("settings-content")
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .overflow_y_scroll()
                    .p(s::S32)
                    .child(self.content(cx)),
            )
    }
}

#[cfg(feature = "ui-probe")]
impl SettingsWindow {
    /// Offline seam: coordinator must also be a fixture; no secure store access.
    pub fn fixture(
        coordinator: Entity<AppCoordinator>,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        bind_keys(cx);
        let mut view = Self::new(coordinator, section, window, cx);
        view.fixture = true;
        view.status.insert(Account::Tmdb, AccountStatus::Saved);
        view.status
            .insert(Account::RealDebrid, AccountStatus::Verified);
        view.status
            .insert(Account::AllDebrid, AccountStatus::Missing);
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[test]
    fn section_roving_navigation() {
        assert_eq!(Section::General.navigate("up"), Some(Section::About));
        assert_eq!(Section::About.navigate("down"), Some(Section::General));
        assert_eq!(Section::Accounts.navigate("home"), Some(Section::General));
        assert_eq!(Section::Playback.navigate("end"), Some(Section::About));
        assert_eq!(Section::Storage.navigate("a"), None);
    }
    #[test]
    fn single_instance_reopens_after_close() {
        assert_eq!(open_decision(true), OpenDecision::Activate);
        assert_eq!(open_decision(false), OpenDecision::Create);
    }
    #[test]
    fn immediate_changes_preserve_unrelated_fields() {
        let mut settings = Settings::default();
        apply_change(&mut settings, SettingChange::Theme(ThemeChoice::Light));
        apply_change(&mut settings, SettingChange::Poster(PosterSize::Large));
        apply_change(&mut settings, SettingChange::Motion(MotionChoice::Always));
        assert_eq!(settings.theme, ThemeChoice::Light);
        assert_eq!(settings.poster_size.preferred_width(), 212.);
        assert_eq!(settings.reduce_motion, MotionChoice::Always);
        assert_eq!(settings.metadata, MetadataProvider::Cinemeta);
        assert_eq!(
            settings.media_limit_bytes,
            Settings::default().media_limit_bytes
        );
    }
    #[test]
    fn credential_failure_is_not_empty_or_missing() {
        assert_eq!(
            AccountStatus::failure("unknown failure"),
            AccountStatus::ServiceFailure
        );
        assert_eq!(
            AccountStatus::failure(&crate::providers::DebridError::CredentialRejected.to_string()),
            AccountStatus::Rejected
        );
        assert_eq!(
            AccountStatus::snapshot(Some(&CredentialState::Missing)),
            AccountStatus::Missing
        );
        assert_eq!(
            AccountStatus::snapshot(Some(&CredentialState::Unavailable("unavailable".into()))),
            AccountStatus::Unavailable
        );
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used)]
mod interaction_tests {
    use super::*;
    use ::core::prelude::v1::test;

    #[gpui::test]
    fn secret_is_cleared_on_submission_and_backpressure(cx: &mut TestAppContext) {
        cx.update(tokens::init);
        let handle = cx.add_window(|window, cx| {
            let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
            SettingsWindow::fixture(coordinator, Section::Accounts, window, cx)
        });
        handle
            .update(cx, |view, _, cx| {
                view.editing = Some(Account::Tmdb);
                view.secret
                    .update(cx, |input, cx| input.set_text("fixture-only-value", cx));
                view.submit_secret(cx);
                assert_eq!(view.secret.read(cx).text(), "");
                assert!(view.editing.is_none());
                assert_eq!(view.status.get(&Account::Tmdb), Some(&AccountStatus::Saved));
                // The workerless coordinator rejects this command, yet text must
                // already be gone. A retry requires re-entering the token.
                view.fixture = false;
                view.editing = Some(Account::RealDebrid);
                view.secret
                    .update(cx, |input, cx| input.set_text("fixture-only-value", cx));
                view.submit_secret(cx);
                assert_eq!(view.secret.read(cx).text(), "");
                assert!(view.errors.contains_key(Account::RealDebrid.label()));
            })
            .unwrap();
    }

    #[gpui::test]
    fn stale_credential_results_are_ignored(cx: &mut TestAppContext) {
        cx.update(tokens::init);
        let handle = cx.add_window(|window, cx| {
            let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
            SettingsWindow::fixture(coordinator, Section::Accounts, window, cx)
        });
        handle
            .update(cx, |view, _, cx| {
                for generation in [10, 11] {
                    view.pending.insert(
                        generation,
                        Pending::Credential(Account::Tmdb, CredentialOperation::Test),
                    );
                }
                view.latest_account.insert(Account::Tmdb, 11);
                view.outcome(
                    &crate::app::AppEvent {
                        generation: 10,
                        outcome: AppOutcome::Failed {
                            message: crate::tmdb::TmdbError::RejectedCredential.to_string(),
                        },
                    },
                    cx,
                );
                assert_eq!(view.status.get(&Account::Tmdb), Some(&AccountStatus::Saved));
                view.outcome(
                    &crate::app::AppEvent {
                        generation: 11,
                        outcome: AppOutcome::TmdbCredentialChecked,
                    },
                    cx,
                );
                assert_eq!(
                    view.status.get(&Account::Tmdb),
                    Some(&AccountStatus::Verified)
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn failed_save_reconciles_preview_without_a_settings_view(cx: &mut TestAppContext) {
        cx.update(tokens::init);
        let coordinator = cx.new(|cx| {
            let mut state = AppCoordinator::fixture(Vec::new(), None, cx);
            state.settings = Some(Settings::default());
            state
        });
        cx.update(|cx| {
            install(&coordinator, cx);
            cx.global_mut::<SettingsWindows>().pending_save = Some(17);
            tokens::apply_preferences(ThemeChoice::Light, MotionChoice::Always, cx);
        });
        coordinator.update(cx, |_, cx| {
            cx.emit(CoordinatorEvent::App(crate::app::AppEvent {
                generation: 17,
                outcome: AppOutcome::Failed {
                    message: "fixture save failure".into(),
                },
            }))
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(tokens::theme(cx).mode, tokens::Mode::Dark);
            assert!(!tokens::theme(cx).reduce_motion);
            assert_eq!(cx.global::<SettingsWindows>().pending_save, None);
        });
    }

    #[gpui::test]
    fn settings_window_is_reused_and_closing_does_not_quit_main(cx: &mut TestAppContext) {
        cx.update(tokens::init);
        let main = cx.add_window(TextInput::new);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
        cx.update(|cx| {
            install(&coordinator, cx);
            open(coordinator.clone(), Section::Accounts, cx);
            let first = cx.global::<SettingsWindows>().window.unwrap();
            open(coordinator.clone(), Section::About, cx);
            assert_eq!(first, cx.global::<SettingsWindows>().window.unwrap());
            first
                .update(cx, |view, window, cx| {
                    assert_eq!(view.section, Section::About);
                    window.remove_window();
                    cx.notify();
                })
                .unwrap();
        });
        cx.run_until_parked();
        assert!(main.update(cx, |_, _, _| ()).is_ok());
        assert!(!coordinator.read_with(cx, |state, _| state.quit_requested));
        cx.update(|cx| {
            open(coordinator.clone(), Section::Storage, cx);
            cx.global::<SettingsWindows>()
                .window
                .unwrap()
                .update(cx, |view, _, _| {
                    assert_eq!(view.section, Section::Storage);
                })
                .unwrap();
        });
    }
}
