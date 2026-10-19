//! Explicit-file playback workflow. Subscribe before issuing any command.
use super::{
    coordinator::{AppCoordinator, CoordinatorEvent, SettingsDestination},
    detail::{StreamBadges, stream_badges, stream_summary},
};
use crate::{
    app::{AppAction, AppOutcome, CredentialState},
    widgets::{
        button::{Button, ButtonSize, ButtonVariant},
        icon::{Icon, IconName},
        notice::{Notice, NoticeKind},
        patterns::{Badge, BadgeTone, ListRow, Skeleton, StateView, ViewState},
        progress::Progress,
        sheet::Sheet,
        text::TypeExt,
        tokens::{self, radius, space as s, typeface as t},
        tooltip::tooltip,
    },
};
use crate::{
    player::PlayerChoice,
    widgets::menu::{Select, SelectEvent},
};
use gpui::{prelude::*, *};
use kinema_core::sources::{PlaybackTarget, RemoteFile, SourceCandidate};
use std::sync::Arc;

// Play-sheet geometry; shared spacing lives in widgets::tokens.
const POSTER_WIDTH: Pixels = px(44.);
const POSTER_HEIGHT: Pixels = px(66.);
/// Visible file rows before the list scrolls.
const FILE_ROWS_VISIBLE: f32 = 4.;
const FILE_SIZE_WIDTH: Pixels = px(72.);

/// Presentation context captured when the sheet opens.
pub(crate) struct PlayContext {
    /// `S01E02`-style label for episodes.
    pub episode: Option<String>,
    pub poster: Option<Arc<RenderImage>>,
}

pub(crate) struct PlaySheet {
    coordinator: Entity<AppCoordinator>,
    title: String,
    context: PlayContext,
    candidate: SourceCandidate,
    provider: kinema_core::sources::DebridKind,
    player: Entity<Select<PlayerChoice>>,
    _player_subscription: Subscription,
    preparation: Option<u64>,
    pending: Option<u64>,
    player_pending: Option<u64>,
    files: Vec<RemoteFile>,
    chosen: Option<String>,
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
fn can_play(chosen: Option<&str>, files: &[RemoteFile], pending: bool) -> bool {
    !pending && chosen.is_some_and(|id| files.iter().any(|f| f.id == id))
}
/// A lone file needs no decision; several must be chosen explicitly.
fn auto_choice(files: &[RemoteFile]) -> Option<String> {
    match files {
        [only] => Some(only.id.clone()),
        _ => None,
    }
}
/// `Extras/Interview.mkv` -> (`Interview.mkv`, Some(`Extras`)).
fn split_path(path: &str) -> (&str, Option<&str>) {
    match path.rsplit_once(['/', '\\']) {
        Some((dir, name)) if !name.is_empty() => (name, Some(dir).filter(|d| !d.is_empty())),
        _ => (path, None),
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
                        .line_clamp(2)
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

    /// Placeholder while the debrid service resolves the files.
    fn preparing_skeleton(&self) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(s::S8)
            .child(
                div()
                    .type_style(t::SMALL)
                    .child(format!("Preparing with {}…", provider_name(self.provider))),
            )
            .child(Progress::indeterminate("prepare-progress"))
            .child(Skeleton::block().h(s::ROW_TWO_LINE))
            .into_any_element()
    }

    /// File choice; only when there is something to choose between.
    fn file_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = *tokens::palette(cx);
        let locked = self.pending.is_some() || self.player_pending.is_some();
        let rows = self.files.iter().enumerate().map(|(index, file)| {
            let chosen = self.chosen.as_ref() == Some(&file.id);
            let (name, dir) = split_path(&file.path);
            let id = file.id.clone();
            let weak = cx.entity().downgrade();
            let mut row = ListRow::new(("play-file", index), name.to_owned())
                .leading(
                    Icon::new(if chosen {
                        IconName::CircleDot
                    } else {
                        IconName::Circle
                    })
                    .color(if chosen { p.accent } else { p.text_muted }),
                )
                .trailing(
                    div()
                        .w(FILE_SIZE_WIDTH)
                        .text_right()
                        .type_style(t::SMALL)
                        .text_color(p.text_secondary)
                        .child(format_bytes(file.size)),
                )
                .selected(chosen);
            if let Some(dir) = dir {
                row = row.subtitle(dir.to_owned());
            }
            if !locked {
                row = row.on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |s, cx| {
                        s.chosen = Some(id.clone());
                        cx.notify();
                    });
                });
            }
            row.into_any_element()
        });
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(s::S4)
            .child(
                div()
                    .type_style(t::SMALL_MEDIUM)
                    .text_color(p.text_secondary)
                    .child("Choose a file"),
            )
            .child(
                div()
                    .id("play-files")
                    .w_full()
                    .max_h(s::ROW_TWO_LINE * FILE_ROWS_VISIBLE)
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(rows),
            )
            .into_any_element()
    }
}

impl Render for PlaySheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let dismiss = cx.entity().downgrade();
        let cancel = cx.entity().downgrade();
        let preparing = self.pending.is_some() && self.preparation.is_none();
        let launching = self.pending.is_some() && self.preparation.is_some();
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
                    .action(Button::new("play-settings", "Open Settings…").on_click(
                        move |_, _, cx| {
                            coordinator.update(cx, |c, cx| c.open_settings_at(destination, cx))
                        },
                    )),
            );
        } else {
            // Preparing shows the shape of the file list; once files exist a
            // lone file is chosen for the user and only real choices are listed.
            let state = if preparing {
                ViewState::Loading
            } else {
                ViewState::Content
            };
            let content = if self.files.len() > 1 {
                self.file_list(cx)
            } else {
                div().into_any_element()
            };
            body = body.child(
                StateView::new("play-files-state", state)
                    .skeleton(self.preparing_skeleton())
                    .content(content),
            );
            if launching {
                body = body
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .child("Requesting player launch…"),
                    )
                    .child(Progress::indeterminate("launch-progress"));
            }
        }
        let limit = self
            .coordinator
            .read(cx)
            .settings
            .as_ref()
            .map(|s| s.media_limit_bytes);
        if let Some(limit) = limit
            && self
                .chosen
                .as_ref()
                .and_then(|id| self.files.iter().find(|f| &f.id == id))
                .is_some_and(|f| f.size > limit)
        {
            let coordinator = self.coordinator.clone();
            body = body.child(
                Notice::new(NoticeKind::Warning, "File exceeds the cache limit").action(
                    Button::new("play-storage", "Storage settings…").on_click(move |_, _, cx| {
                        coordinator.update(cx, |c, cx| {
                            c.open_settings_at(SettingsDestination::Storage, cx)
                        })
                    }),
                ),
            );
        }
        if let Some(error) = &self.error {
            body = body.child(
                Notice::new(NoticeKind::Error, "Couldn’t play this stream").body(error.clone()),
            );
        }
        let player = if self.pending.is_none() && self.player_pending.is_none() {
            div().child(self.player.clone()).into_any_element()
        } else {
            let label = match self.player.read(cx).value() {
                PlayerChoice::Mpv => "mpv",
                PlayerChoice::Vlc => "VLC",
                PlayerChoice::Custom(_) => "Custom player",
            };
            div()
                .type_style(t::BODY)
                .text_color(p.text_secondary)
                .child(label)
                .into_any_element()
        };
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(s::S12)
                .child(
                    div()
                        .type_style(t::SMALL_MEDIUM)
                        .text_color(p.text_secondary)
                        .child("Player"),
                )
                .child(player),
        );
        Sheet::new("play-sheet", "Play")
            .initial_focus(self.cancel.clone())
            .body(body)
            .on_dismiss(move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| this.dismiss(cx));
            })
            .footer(
                Button::new("play-cancel", "Cancel")
                    .size(ButtonSize::Large)
                    .focus_handle(self.cancel.clone())
                    .on_click(move |_, _, cx| {
                        let _ = cancel.update(cx, |this, cx| this.dismiss(cx));
                    }),
            )
            .footer(
                Button::new("play-launch", "Play")
                    .variant(ButtonVariant::Primary)
                    .size(ButtonSize::Large)
                    .icon(IconName::Play)
                    .busy(launching)
                    .disabled(!can_play(
                        self.chosen.as_deref(),
                        &self.files,
                        self.pending.is_some() || self.player_pending.is_some(),
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let (Some(preparation), Some(file_id)) =
                            (this.preparation, this.chosen.clone())
                        {
                            this.error = None;
                            this.pending = this.coordinator.update(cx, |c, cx| {
                                c.command(
                                    AppAction::PlayFile {
                                        preparation,
                                        file_id,
                                    },
                                    cx,
                                )
                            });
                            if this.pending.is_none() {
                                this.error =
                                    Some("Application service unavailable or busy. Retry.".into());
                            }
                            cx.notify();
                        }
                    })),
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
