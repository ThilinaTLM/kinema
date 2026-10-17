//! Explicit-file playback workflow. Subscribe before issuing any command.
use super::coordinator::{AppCoordinator, CoordinatorEvent, SettingsDestination};
use crate::{
    app::{AppAction, AppOutcome, CredentialState},
    widgets::{
        button::Button,
        sheet::Sheet,
        switch::Radio,
        text::TypeExt,
        tokens::{self, space as s, typeface as t},
    },
};
use crate::{
    player::PlayerChoice,
    widgets::menu::{Select, SelectEvent},
};
use gpui::{prelude::*, *};
use kinema_core::sources::{PlaybackTarget, RemoteFile, SourceCandidate};

pub(crate) struct PlaySheet {
    coordinator: Entity<AppCoordinator>,
    title: String,
    provider: kinema_core::sources::DebridKind,
    player: Entity<Select<PlayerChoice>>,
    _player_subscription: Subscription,
    release: String,
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
impl Render for PlaySheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let dismiss = cx.entity().downgrade();
        let cancel = cx.entity().downgrade();
        let mut body = div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(div().type_style(t::MONO).child(self.release.clone()))
            .child(
                div()
                    .type_style(t::SMALL)
                    .text_color(p.text_secondary)
                    .child(format!("via {}", provider_name(self.provider))),
            );
        if self.missing {
            let coordinator = self.coordinator.clone();
            let destination = SettingsDestination::Account(self.provider);
            body = body
                .child(
                    div()
                        .w_full()
                        .min_h(px(54.))
                        .flex_none()
                        .child(self.credential_notice.clone()),
                )
                .child(
                    Button::new("play-settings", "Open Settings…").on_click(move |_, _, cx| {
                        coordinator.update(cx, |c, cx| c.open_settings_at(destination, cx))
                    }),
                );
        } else if self.pending.is_some() {
            body = body
                .child(if self.preparation.is_some() {
                    "Requesting player launch…".to_owned()
                } else {
                    format!("Preparing with {}…", provider_name(self.provider))
                })
                .child(crate::widgets::progress::Progress::indeterminate(
                    "prepare-progress",
                ));
        }
        if !self.files.is_empty() {
            body = body.child("Choose a file explicitly");
        }
        for (index, file) in self.files.iter().enumerate() {
            let id = file.id.clone();
            let weak = cx.entity().downgrade();
            body = body.child(
                Radio::new(
                    ("play-file", index),
                    format!("{} · {}", file.path, format_bytes(file.size)),
                    self.chosen.as_ref() == Some(&file.id),
                )
                .typography(t::MONO)
                .on_select(move |_, cx| {
                    let _ = weak.update(cx, |s, cx| {
                        s.chosen = Some(id.clone());
                        cx.notify();
                    });
                }),
            );
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
                crate::widgets::notice::Notice::new(
                    crate::widgets::notice::NoticeKind::Warning,
                    "File exceeds the cache limit",
                )
                .action(Button::new("play-storage", "Storage settings…").on_click(
                    move |_, _, cx| {
                        coordinator.update(cx, |c, cx| {
                            c.open_settings_at(SettingsDestination::Storage, cx)
                        })
                    },
                )),
            );
        }
        if let Some(error) = &self.error {
            body = body.child(div().text_color(p.status_error).child(error.clone()));
        }
        body = body.child(if self.pending.is_none() && self.player_pending.is_none() {
            div()
                .flex()
                .items_center()
                .child("Player: ")
                .child(self.player.clone())
        } else {
            let label = match self.player.read(cx).value() {
                PlayerChoice::Mpv => "mpv",
                PlayerChoice::Vlc => "VLC",
                PlayerChoice::Custom(_) => "Custom player",
            };
            div().child(format!("Player: {label}"))
        });
        // Give the modal's scroll body a definite measure; nested flex content
        // otherwise contributes only its first line to GPUI's intrinsic size.
        let body_height = (s::S48 * 3.
            + s::S48 * self.files.len() as f32
            + if self.missing { s::S48 * 2. } else { px(0.) }
            + if self.error.is_some() {
                s::S48 + s::S8
            } else {
                px(0.)
            })
        .min(s::S48 * 9.);
        let body = body.id("play-body").h(body_height).overflow_y_scroll();
        Sheet::new("play-sheet", format!("Play · {}", self.title))
            .initial_focus(self.cancel.clone())
            .body(body)
            .on_dismiss(move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| this.dismiss(cx));
            })
            .footer(
                Button::new("play-cancel", "Cancel")
                    .focus_handle(self.cancel.clone())
                    .on_click(move |_, _, cx| {
                        let _ = cancel.update(cx, |this, cx| this.dismiss(cx));
                    }),
            )
            .footer(
                Button::new("play-launch", "Play")
                    .variant(crate::widgets::button::ButtonVariant::Primary)
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
