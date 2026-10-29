//! Explicit-file playback workflow. Subscribe before issuing any command.
use super::{
    coordinator::{AppCoordinator, CoordinatorEvent, PlaybackIntent, SettingsDestination},
    detail::{StreamBadges, stream_badges, stream_summary},
};
use crate::player::PlayerChoice;
use crate::{
    app::{AppAction, AppOutcome, CredentialState},
    widgets::{
        button::{Button, ButtonSize},
        notice::{Notice, NoticeKind},
        patterns::{Badge, BadgeTone},
        progress::Progress,
        sheet::Sheet,
        text::TypeExt,
        tokens::{self, radius, space as s, typeface as t},
        tooltip::tooltip,
    },
};
use gpui::{prelude::*, *};
use kinema_core::sources::{FileSelection, PlaybackTarget, RemoteFile, SourceCandidate};
use std::sync::Arc;

// Play-sheet geometry; shared spacing lives in widgets::tokens.
const POSTER_WIDTH: Pixels = px(44.);
const POSTER_HEIGHT: Pixels = px(66.);

/// Presentation context captured when the sheet opens.
pub(crate) struct PlayContext {
    /// `S01E02`-style label for episodes.
    pub episode: Option<String>,
    /// Catalog episode name (not parsed from the release name).
    pub episode_title: Option<String>,
    pub poster: Option<Arc<RenderImage>>,
}

pub(crate) struct PlaySheet {
    coordinator: Entity<AppCoordinator>,
    title: String,
    context: PlayContext,
    candidate: SourceCandidate,
    provider: kinema_core::sources::DebridKind,
    preparation: Option<u64>,
    pending: Option<u64>,
    /// Settings save that makes `provider` the worker's active account. Until its
    /// outcome arrives, invalidations come from that switch, not a later change.
    switch: Option<u64>,
    /// Set when the prepared file is larger than the cache limit.
    limit_exceeded: Option<u64>,
    error: Option<String>,
    missing: bool,
    credential_notice: String,
    cancel: FocusHandle,
    _subscription: Subscription,
}
impl EventEmitter<DismissEvent> for PlaySheet {}
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

impl PlaySheet {
    fn header(&self, cx: &App) -> Div {
        let p = *tokens::palette(cx);
        let mut poster = div()
            .w(POSTER_WIDTH)
            .h(POSTER_HEIGHT)
            .flex_none()
            .rounded(radius::XS)
            .overflow_hidden()
            .bg(p.bg_placeholder)
            .border(s::HAIRLINE)
            .border_color(p.poster_edge);
        if let Some(image) = &self.context.poster {
            poster = poster.child(img(image.clone()).size_full().object_fit(ObjectFit::Cover));
        }
        div().flex().items_center().gap(s::S12).child(poster).child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(s::S2)
                .child(
                    div()
                        .type_style(t::HEADING)
                        .text_color(p.text_primary)
                        .clamp_lines(2)
                        .child(self.title.clone()),
                )
                .when_some(self.context.episode.clone(), |el, episode| {
                    el.child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .child(episode),
                    )
                }),
        )
    }

    /// Badges and facts first; the raw release name is muted and truncated.
    fn stream_summary_block(&self, cx: &App) -> Div {
        let p = *tokens::palette(cx);
        let StreamBadges {
            resolution,
            dynamic_range,
            codec,
        } = stream_badges(&self.candidate);
        let badges: Vec<_> = [
            resolution.map(|l| {
                Badge::new(l).tone(if l == "4K" {
                    BadgeTone::Accent
                } else {
                    BadgeTone::Neutral
                })
            }),
            dynamic_range.map(|l| Badge::new(l).tone(BadgeTone::Info)),
            codec.map(Badge::new),
        ]
        .into_iter()
        .flatten()
        .collect();
        div()
            .w_full()
            .p(s::S12)
            .rounded(radius::MD)
            .bg(p.bg_raised)
            .border(s::HAIRLINE)
            .border_color(p.line_subtle)
            .flex()
            .flex_col()
            .gap(s::S6)
            .when(!badges.is_empty(), |el| {
                el.child(div().flex().flex_wrap().gap(s::S4).children(badges))
            })
            .child(
                div()
                    .type_style(t::BODY)
                    .text_color(p.text_primary)
                    .child(stream_summary(
                        &self.candidate,
                        &format!("via {}", provider_name(self.provider)),
                    )),
            )
            .child(
                div()
                    .id("play-release")
                    .min_w_0()
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .truncate()
                    .tooltip(tooltip(self.candidate.name.clone()))
                    .child(self.candidate.name.clone()),
            )
    }
}

impl Render for PlaySheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let dismiss = cx.entity().downgrade();
        let cancel = cx.entity().downgrade();
        let busy = self.pending.is_some();
        let mut body = div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(self.header(cx))
            .child(self.stream_summary_block(cx));
        if self.missing {
            let coordinator = self.coordinator.clone();
            let destination = SettingsDestination::Account(self.provider);
            body = body.child(
                Notice::new(NoticeKind::Warning, "This stream can’t be prepared yet")
                    .body(self.credential_notice.clone())
                    .action(Button::new("play-settings", "Open Settings").on_click(
                        move |_, _, cx| {
                            coordinator.update(cx, |c, cx| c.open_settings_at(destination, cx))
                        },
                    )),
            );
        } else if busy {
            // One inline progress state: preparing, then starting playback.
            let label = if self.preparation.is_none() {
                format!("Preparing with {}…", provider_name(self.provider))
            } else {
                "Starting playback…".to_owned()
            };
            body = body.child(
                div()
                    .id("play-progress")
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(s::S8)
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .child(label),
                    )
                    .child(Progress::indeterminate("prepare-progress")),
            );
        }
        if let Some(error) = &self.error {
            body = body.child(
                Notice::new(NoticeKind::Error, "Couldn’t play this stream").body(error.clone()),
            );
        }
        if let Some(limit) = self.limit_exceeded {
            let coordinator = self.coordinator.clone();
            body = body.child(
                Notice::new(
                    NoticeKind::Warning,
                    format!("File exceeds the cache limit ({})", format_bytes(limit)),
                )
                .action(Button::new("play-storage", "Storage settings").on_click(
                    move |_, _, cx| {
                        coordinator.update(cx, |c, cx| {
                            c.open_settings_at(SettingsDestination::Storage, cx)
                        })
                    },
                )),
            );
        }
        let player = self
            .coordinator
            .read(cx)
            .settings
            .as_ref()
            .map(|s| match &s.player {
                PlayerChoice::Embedded => "Embedded MPV",
                PlayerChoice::Mpv => "External MPV",
                PlayerChoice::Vlc => "External VLC",
                PlayerChoice::Custom(_) => "Custom player",
            });
        if let Some(player) = player {
            body = body.child(
                div()
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(format!("Opens in {player}")),
            );
        }
        Sheet::new("play-sheet", "Play")
            .initial_focus(self.cancel.clone())
            .body(body)
            .on_dismiss(move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| this.dismiss(cx));
            })
            .footer(
                Button::new("play-cancel", if busy { "Cancel" } else { "Close" })
                    .size(ButtonSize::Large)
                    .focus_handle(self.cancel.clone())
                    .on_click(move |_, _, cx| {
                        let _ = cancel.update(cx, |this, cx| this.dismiss(cx));
                    }),
            )
    }
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
fn provider_name(provider: kinema_core::sources::DebridKind) -> &'static str {
    match provider {
        kinema_core::sources::DebridKind::RealDebrid => "Real-Debrid",
        kinema_core::sources::DebridKind::AllDebrid => "AllDebrid",
    }
}
