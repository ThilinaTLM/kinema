//! Direct-GPUI Settings/About. Bootstrap calls `install` once with the shared
//! main-window coordinator. No worker, keyring, filesystem or player is created by this view.
use crate::controllers::coordinator::{AppCoordinator, CoordinatorEvent};
use crate::{
    app::{AppAction, AppOutcome, CredentialState},
    credentials::SecretString,
    player::PlayerChoice,
    settings::{MotionChoice, PosterSize, Settings, ThemeChoice},
    ui::primitives::button::Button,
    ui::primitives::icon::IconName,
    ui::primitives::menu::{Select, SelectEvent},
    ui::primitives::segmented::{self, Segmented},
    ui::primitives::switch::Switch,
    ui::primitives::text_input::{TextInput, TextInputEvent},
    ui::theme::typography::{self, TextSize, TypeExt, Weight},
    ui::theme::{self, size, space as s},
};
mod about;
mod accounts;
mod appearance;
mod general;
#[cfg(all(test, feature = "ui-probe-test"))]
mod interaction_tests;
mod keyboard;
mod playback;
mod storage;
#[cfg(test)]
mod tests;

use accounts::*;
use gpui::{prelude::*, *};
use kinema_core::provider::{ProviderId, ProviderPriority};
use kinema_core::sources::{DebridKind, IndexerKind};
use std::collections::HashMap;
use storage::bytes_label;

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
    KeyboardMouse,
    Storage,
    Appearance,
    About,
}
impl Section {
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Accounts,
        Self::Playback,
        Self::KeyboardMouse,
        Self::Storage,
        Self::Appearance,
        Self::About,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Accounts => "Accounts",
            Self::Playback => "Playback",
            Self::KeyboardMouse => "Keyboard & Mouse",
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
        CoordinatorEvent::OpenSettingsAt(destination) => open_at(coordinator, *destination, cx),
        CoordinatorEvent::About => open(coordinator, Section::About, cx),
        CoordinatorEvent::App(event) => {
            if cx.global::<SettingsWindows>().pending_save == Some(event.generation) {
                cx.global_mut::<SettingsWindows>().pending_save = None;
                // This survives closing Settings while a save is in flight.
                if let Some(settings) = &coordinator.read(cx).settings {
                    theme::apply_preferences(settings.theme, settings.reduce_motion, cx);
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
                theme::apply_preferences(preferences.0, preferences.1, cx);
            }
        }
    })
    .detach();
    if let Some(settings) = &coordinator.read(cx).settings {
        theme::apply_preferences(settings.theme, settings.reduce_motion, cx);
    }
    // Defer so this can also be installed while MainWindow is being built.
    cx.defer(|cx| {
        for handle in cx.windows() {
            let _ = handle.update(cx, |_, window, _| {
                theme::observe_appearance(window).detach()
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

pub fn open_at(
    coordinator: Entity<AppCoordinator>,
    destination: crate::controllers::coordinator::SettingsDestination,
    cx: &mut App,
) {
    use crate::controllers::coordinator::SettingsDestination;
    let section = match destination {
        SettingsDestination::General => Section::General,
        SettingsDestination::Account(_) => Section::Accounts,
        SettingsDestination::Storage => Section::Storage,
    };
    open(coordinator, section, cx);
    if let SettingsDestination::Account(provider) = destination {
        let account = match provider {
            DebridKind::RealDebrid => Account::RealDebrid,
            DebridKind::AllDebrid => Account::AllDebrid,
        };
        if let Some(handle) = cx.global::<SettingsWindows>().window {
            let _ = handle.update(cx, |view, window, cx| {
                if let Some(focus) = view.account_focus.get(&account) {
                    focus.focus(window, cx);
                }
                cx.notify();
            });
        }
    }
}

pub fn open(coordinator: Entity<AppCoordinator>, section: Section, cx: &mut App) {
    tracing::debug!(section = section.label(), "UI settings window requested");
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
            size(
                theme::layout::settings::WINDOW_WIDTH,
                theme::layout::settings::WINDOW_HEIGHT,
            ),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Kinema Settings".into()),
            ..Default::default()
        }),
        window_min_size: Some(size(
            theme::layout::settings::MIN_WIDTH,
            theme::layout::settings::MIN_HEIGHT,
        )),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        cx.new(|cx| SettingsWindow::new(coordinator, section, window, cx))
    }) {
        Ok(handle) => cx.global_mut::<SettingsWindows>().window = Some(handle),
        Err(error) => tracing::error!(%error, "Could not open Settings window"),
    }
}

#[derive(Clone, Copy)]
enum SettingChange {
    Theme(ThemeChoice),
    Poster(PosterSize),
    Motion(MotionChoice),
    ProviderUp(ProviderId),
    ProviderDown(ProviderId),
    Indexer(IndexerKind),
    Debrid(DebridKind),
}
fn apply_change(settings: &mut Settings, change: SettingChange) {
    match change {
        SettingChange::Theme(v) => settings.theme = v,
        SettingChange::Poster(v) => settings.poster_size = v,
        SettingChange::Motion(v) => settings.reduce_motion = v,
        SettingChange::ProviderUp(id) => settings.metadata_priority.move_up(id),
        SettingChange::ProviderDown(id) => settings.metadata_priority.move_down(id),
        SettingChange::Indexer(v) => settings.indexer = v,
        SettingChange::Debrid(v) => settings.debrid = v,
    }
}

pub struct SettingsWindow {
    coordinator: Entity<AppCoordinator>,
    section: Section,
    sidebar: FocusHandle,
    account_focus: HashMap<Account, FocusHandle>,
    settings: Settings,
    settings_loaded: bool,
    indexer: Entity<Select<IndexerKind>>,
    debrid: Entity<Select<DebridKind>>,
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
        let limit = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_label("Media cache limit (GB)", cx);
            input.set_text(
                (settings.media_limit_bytes / 1024 / 1024 / 1024).to_string(),
                cx,
            );
            input
        });
        let secret = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_secret(true, cx);
            input.set_label("Debrid API token", cx);
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
            cx.subscribe(&indexer, |this, _, event, cx| {
                let SelectEvent::Changed(value) = event;
                this.change("Streams", SettingChange::Indexer(*value), cx);
            }),
            cx.subscribe(&debrid, |this, _, event, cx| {
                let SelectEvent::Changed(value) = event;
                this.change("Debrid", SettingChange::Debrid(*value), cx);
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
            theme::observe_appearance(window),
        ];
        let settings_loaded = coordinator.read(cx).settings.is_some();
        let mut this = Self {
            coordinator,
            section,
            sidebar: cx.focus_handle().tab_stop(true),
            account_focus: Account::ALL
                .into_iter()
                .map(|account| (account, cx.focus_handle().tab_stop(true)))
                .collect(),
            settings,
            settings_loaded,
            indexer,
            debrid,
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
            }
            self.indexer
                .update(cx, |select, cx| select.set_value(self.settings.indexer, cx));
            self.debrid
                .update(cx, |select, cx| select.set_value(self.settings.debrid, cx));
        }
    }
    fn show_error(&mut self, field: &'static str, message: String) -> Option<String> {
        tracing::warn!(field, message, "UI settings error displayed");
        self.errors.insert(field, message)
    }
    fn change(&mut self, field: &'static str, change: SettingChange, cx: &mut Context<Self>) {
        apply_change(&mut self.settings, change);
        self.save(field, cx);
    }
    fn save(&mut self, field: &'static str, cx: &mut Context<Self>) {
        tracing::debug!(field, "UI settings save requested");
        self.errors.remove(field);
        if !self.settings_loaded && !self.fixture {
            self.show_error(
                field,
                "Settings are still loading or unavailable; no settings were saved.".into(),
            );
            cx.notify();
            return;
        }
        theme::apply_preferences(self.settings.theme, self.settings.reduce_motion, cx);
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
            self.show_error(
                field,
                "Application service unavailable or busy. Retry this change.".into(),
            );
        }
        cx.notify();
    }
    fn apply_player(&mut self, choice: PlayerChoice, cx: &mut Context<Self>) {
        if let Err(error) = crate::player::validate_choice(&choice) {
            self.show_error("Player", error.to_string());
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
            self.show_error(
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
    fn content(&self, cx: &Context<Self>) -> AnyElement {
        let p = *theme::palette(cx);
        let weak = cx.entity().downgrade();
        let mut body = div().flex().flex_col().gap(s::S24).child(
            div()
                .text(TextSize::Xl)
                .weight(Weight::Semibold)
                .child(self.section.label()),
        );
        body = match self.section {
            Section::General => self.general(body, p),
            Section::Accounts => self.accounts_section(body, p, weak, cx),
            Section::Playback => self.playback(body, p, weak),
            Section::KeyboardMouse => self.keyboard_mouse(body, p),
            Section::Storage => self.storage(body, p, weak, cx),
            Section::Appearance => self.appearance(body, p, weak),
            Section::About => self.about(body, p),
        };
        for (field, message) in &self.errors {
            body = body.child(
                div()
                    .text(TextSize::Sm)
                    .text_color(if *field == "Cleanup" {
                        p.muted_foreground
                    } else {
                        p.destructive
                    })
                    .child(format!("{field}: {message}")),
            );
        }
        if self.save_pending {
            body = body.child(text("Applying settings…", p.subtle_foreground));
        }
        body.into_any_element()
    }
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
        .child(div().w(size::CONTROL_SM * 10.).flex_none().child(control))
        .into_any_element()
}
impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *theme::palette(cx);
        let sidebar_focus = self.sidebar.is_focused(window) && window.last_input_was_keyboard();
        let _ = self.subscriptions.len(); // Own all subscriptions for the view lifetime.
        let selected = self.section;
        div()
            .key_context("KinemaSettings")
            .on_action(|_: &NextFocus, window, cx| window.focus_next(cx))
            .on_action(|_: &PreviousFocus, window, cx| window.focus_prev(cx))
            .size_full()
            .flex()
            .bg(p.background)
            .text_color(p.foreground)
            .font_family(typography::UI_FAMILY)
            .text(TextSize::Base)
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
                    .bg(p.surface)
                    .border_r(size::HAIRLINE)
                    .border_color(p.border)
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
                            .h(size::ROW)
                            .px(s::S16)
                            .flex()
                            .items_center()
                            .border_l(s::S2)
                            .border_color(if section == selected {
                                p.primary
                            } else {
                                transparent_black()
                            })
                            .when(section == selected, |el| {
                                el.bg(p.selection).text_color(p.primary_text)
                            })
                            .when(section == selected && sidebar_focus, |el| {
                                el.border(size::FOCUS_RING).border_color(p.ring)
                            })
                            .hover(|el| el.bg(p.accent))
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
    /// Fixture variant: no saved TMDB token.
    pub fn fixture_tmdb_missing(&mut self) {
        self.status.insert(Account::Tmdb, AccountStatus::Missing);
    }
}
