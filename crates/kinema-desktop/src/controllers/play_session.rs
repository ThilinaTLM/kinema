//! Non-modal explicit-file playback workflow, shown inline in the Streams
//! inspector footer (or the narrow-window play dock). Subscribe before
//! issuing any command.
//!
//! The session owns one preparation/launch; dropping it (Cancel, Dismiss,
//! leaving the page, or starting another stream) cancels that work. It emits
//! [`DismissEvent`] when playback started or the user dismissed it, and
//! [`PlaySessionEvent::Retry`] when the user asks to try the same stream again.
use super::coordinator::{AppCoordinator, CoordinatorEvent, PlaybackIntent, SettingsDestination};
use crate::player::PlayerChoice;
use crate::{
    app::{AppAction, AppOutcome, CredentialState},
    settings::Settings,
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::primitives::alert::{Alert, AlertTone},
    ui::primitives::button::{Button, ButtonSize, ButtonVariant},
    ui::primitives::progress::Progress,
    ui::theme::typography::TypeExt,
    ui::theme::{self, space as s, typeface as t},
};
use gpui::{prelude::*, *};
use kinema_core::sources::{
    DebridKind, FileSelection, PlaybackTarget, RemoteFile, SourceCandidate,
};

pub(crate) struct PlaySession {
    coordinator: Entity<AppCoordinator>,
    provider: DebridKind,
    preparation: Option<u64>,
    pending: Option<u64>,
    /// Settings save that activates the selected account before preparation.
    switch: Option<u64>,
    /// Set when the prepared file is larger than the cache limit.
    limit_exceeded: Option<u64>,
    error: Option<String>,
    missing: bool,
    credential_notice: String,
    _subscription: Subscription,
}
impl EventEmitter<DismissEvent> for PlaySession {}
/// The user asked to prepare the same stream again after a failure.
pub(crate) enum PlaySessionEvent {
    Retry,
}
impl EventEmitter<PlaySessionEvent> for PlaySession {}
#[cfg(feature = "ui-probe")]
mod fixtures;
#[cfg(test)]
mod tests;
mod workflow;

/// What to do once the debrid service has prepared the torrent.
#[derive(Debug, PartialEq, Eq)]
enum AfterPrepare {
    /// Play this remote file immediately; there is no file picker.
    Play(String),
    /// Selection was not unique: tell the user to try another stream.
    Message(String),
}
fn after_prepare(selection: &FileSelection, files: &[RemoteFile]) -> AfterPrepare {
    const FALLBACK: &str = "Couldn't identify the file in this torrent — try another stream.";
    match selection {
        FileSelection::Unique(index) => match files.get(*index) {
            Some(file) => AfterPrepare::Play(file.id.clone()),
            None => AfterPrepare::Message(FALLBACK.into()),
        },
        other => AfterPrepare::Message(other.error_message().unwrap_or(FALLBACK).into()),
    }
}

/// Visible phase of a session, derived from its workflow fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlayPhase {
    /// Waiting for the debrid service to prepare the torrent.
    Preparing,
    /// Prepared; the player launch is in flight.
    Starting,
    /// Preparation or launch failed; the message is user-facing.
    Failed(String),
    /// Nothing was started: the account is missing, unavailable or changed.
    Blocked(String),
}

impl PlaySession {
    pub(crate) fn phase(&self) -> PlayPhase {
        if self.missing {
            PlayPhase::Blocked(self.credential_notice.clone())
        } else if let Some(error) = &self.error {
            PlayPhase::Failed(error.clone())
        } else if self.preparation.is_none() {
            PlayPhase::Preparing
        } else {
            PlayPhase::Starting
        }
    }
    /// True while preparation or launch work is in flight.
    pub(crate) fn busy(&self) -> bool {
        matches!(self.phase(), PlayPhase::Preparing | PlayPhase::Starting)
    }
    fn step(&self, label: String, state: StepState, cx: &App) -> Div {
        let p = *theme::palette(cx);
        let (icon, ink, text) = match state {
            StepState::Done => (IconName::CircleCheck, p.status_ok, p.text_secondary),
            StepState::Now => (IconName::CircleDot, p.accent_text, p.text_primary),
            StepState::Later => (IconName::Circle, p.text_muted, p.text_muted),
        };
        div()
            .flex()
            .items_center()
            .gap(s::S8)
            .type_style(if state == StepState::Now {
                t::SMALL_MEDIUM
            } else {
                t::SMALL
            })
            .text_color(text)
            .child(Icon::new(icon).size(s::S12 + s::S2).color(ink))
            .child(label)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepState {
    Done,
    Now,
    Later,
}

/// Full-width button in a fixed-height slot: the Button's focus-ring margins
/// would otherwise under-measure a stacked column.
pub(crate) fn button_slot(height: Pixels, button: impl IntoElement) -> Div {
    div()
        .w_full()
        .h(height)
        .flex_none()
        .flex()
        .flex_col()
        .child(button)
}

impl Render for PlaySession {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = player_label(self.coordinator.read(cx).settings.as_ref());
        let mut body = div()
            .id("play-session")
            .w_full()
            .flex()
            .flex_col()
            .gap(s::S12);
        match self.phase() {
            phase @ (PlayPhase::Preparing | PlayPhase::Starting) => {
                let preparing = phase == PlayPhase::Preparing;
                let launch = match player {
                    Some(player) => format!("Starting playback in {player}"),
                    None => "Starting playback".to_owned(),
                };
                body = body
                    .child(
                        div()
                            .id("play-progress")
                            .flex()
                            .flex_col()
                            .gap(s::S6)
                            .child(self.step(
                                format!("Preparing with {}…", provider_name(self.provider)),
                                if preparing {
                                    StepState::Now
                                } else {
                                    StepState::Done
                                },
                                cx,
                            ))
                            .child(self.step(
                                launch,
                                if preparing {
                                    StepState::Later
                                } else {
                                    StepState::Now
                                },
                                cx,
                            )),
                    )
                    .child(Progress::indeterminate("prepare-progress"))
                    .child(button_slot(
                        s::CONTROL_HEIGHT_LARGE,
                        Button::new("play-cancel", "Cancel")
                            .size(ButtonSize::Md)
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    ));
            }
            PlayPhase::Failed(message) => {
                let mut notice =
                    Alert::new(AlertTone::Error, "Couldn’t play this stream").body(message);
                if let Some(limit) = self.limit_exceeded {
                    let coordinator = self.coordinator.clone();
                    notice = Alert::new(
                        AlertTone::Warning,
                        format!("File exceeds the cache limit ({})", format_bytes(limit)),
                    )
                    .body(self.error.clone().unwrap_or_default())
                    .action(
                        Button::new("play-storage", "Storage settings").on_click(
                            move |_, _, cx| {
                                coordinator.update(cx, |c, cx| {
                                    c.open_settings_at(SettingsDestination::Storage, cx)
                                })
                            },
                        ),
                    );
                }
                body = body.child(notice).child(
                    div()
                        .w_full()
                        .flex()
                        .gap(s::S8)
                        .child(
                            div().flex_1().child(button_slot(
                                s::CONTROL_HEIGHT_LARGE,
                                Button::new("play-retry", "Try again")
                                    .size(ButtonSize::Md)
                                    .icon(IconName::Refresh)
                                    .on_click(
                                        cx.listener(|_, _, _, cx| cx.emit(PlaySessionEvent::Retry)),
                                    ),
                            )),
                        )
                        .child(
                            div().flex_1().child(button_slot(
                                s::CONTROL_HEIGHT_LARGE,
                                Button::new("play-dismiss", "Dismiss")
                                    .size(ButtonSize::Md)
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                            )),
                        ),
                );
            }
            PlayPhase::Blocked(message) => {
                let coordinator = self.coordinator.clone();
                let destination = SettingsDestination::Account(self.provider);
                body = body
                    .child(
                        Alert::new(AlertTone::Warning, "This stream can’t be prepared yet")
                            .body(message),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .gap(s::S8)
                            .child(
                                div().flex_1().child(button_slot(
                                    s::CONTROL_HEIGHT_LARGE,
                                    Button::new("play-settings", "Open Settings")
                                        .size(ButtonSize::Md)
                                        .icon(IconName::Settings)
                                        .on_click(move |_, _, cx| {
                                            coordinator.update(cx, |c, cx| {
                                                c.open_settings_at(destination, cx)
                                            })
                                        }),
                                )),
                            )
                            .child(
                                div().flex_1().child(button_slot(
                                    s::CONTROL_HEIGHT_LARGE,
                                    Button::new("play-dismiss", "Dismiss")
                                        .size(ButtonSize::Md)
                                        .variant(ButtonVariant::Ghost)
                                        .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                                )),
                            ),
                    );
            }
        }
        body
    }
}

/// Human name of the configured player, e.g. for "Opens in Embedded MPV".
pub(crate) fn player_label(settings: Option<&Settings>) -> Option<&'static str> {
    settings.map(|s| match &s.player {
        PlayerChoice::Embedded => "Embedded MPV",
        PlayerChoice::Mpv => "External MPV",
        PlayerChoice::Vlc => "External VLC",
        PlayerChoice::Custom(_) => "Custom player",
    })
}
pub(crate) fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GiB", bytes as f64 / 1_073_741_824.)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MiB", bytes as f64 / 1_048_576.)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.)
    } else {
        format!("{bytes} B")
    }
}
pub(crate) fn provider_name(provider: DebridKind) -> &'static str {
    match provider {
        DebridKind::RealDebrid => "Real-Debrid",
        DebridKind::AllDebrid => "AllDebrid",
    }
}
