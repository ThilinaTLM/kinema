//! Explicit-file playback workflow. Subscribe before issuing any command.
use super::coordinator::{AppCoordinator, CoordinatorEvent};
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
fn can_play(chosen: Option<&str>, files: &[RemoteFile], pending: bool) -> bool {
    !pending && chosen.is_some_and(|id| files.iter().any(|f| f.id == id))
}
impl PlaySheet {
    pub(crate) fn new(
        coordinator: Entity<AppCoordinator>,
        title: String,
        target: PlaybackTarget,
        candidate: SourceCandidate,
        cx: &mut Context<Self>,
    ) -> Self {
        let choice = coordinator
            .read(cx)
            .settings
            .as_ref()
            .map(|s| s.player.clone())
            .unwrap_or(PlayerChoice::Mpv);
        let mut options = vec![
            (PlayerChoice::Mpv, "mpv".into()),
            (PlayerChoice::Vlc, "VLC".into()),
        ];
        if let PlayerChoice::Custom(_) = &choice {
            options.push((choice.clone(), "Custom player".into()));
        }
        let player = cx.new(|cx| Select::new(choice, options, cx));
        let player_coordinator = coordinator.clone();
        let player_subscription = cx.subscribe(
            &player,
            move |this, _, event: &SelectEvent<PlayerChoice>, cx| {
                let SelectEvent::Changed(choice) = event;
                let settings = player_coordinator.read(cx).settings.clone();
                if let Some(mut settings) = settings {
                    settings.player = choice.clone();
                    this.player_pending = player_coordinator
                        .update(cx, |c, cx| c.command(AppAction::SaveSettings(settings), cx));
                    if this.player_pending.is_none() {
                        this.error = Some("Player preference could not be saved. Retry.".into());
                    }
                    cx.notify();
                }
            },
        );
        let subscription = cx.subscribe(&coordinator, |this, _, event: &CoordinatorEvent, cx| {
            let CoordinatorEvent::App(event) = event else { return };
            if Some(event.generation)==this.player_pending {
                this.player_pending=None;
                if !matches!(event.outcome,AppOutcome::Settings{..}|AppOutcome::SettingsWarning{..}) {
                    this.error=Some(match &event.outcome{AppOutcome::Failed{message}=>message.clone(),_=>"Player preference could not be saved.".into()});
                    if let Some(choice)=this.coordinator.read(cx).settings.as_ref().map(|s|s.player.clone()){this.player.update(cx,|p,cx|p.set_value(choice,cx));}
                }
                cx.notify();return;
            }
            if Some(event.generation) != this.pending { return; }
            this.pending = None;
            match &event.outcome {
                AppOutcome::Prepared { prepared, .. } => { this.preparation = Some(event.generation); this.files = prepared.files.clone(); }
                AppOutcome::PlaybackStarted { .. } => cx.emit(DismissEvent),
                AppOutcome::Failed { message } | AppOutcome::CacheFull { message } => this.error = Some(message.clone()),
                AppOutcome::CacheLimitNeeded { size, limit } => this.error = Some(format!("File is {size} bytes; cache limit is {limit} bytes. Open Storage settings to increase it.")),
                AppOutcome::Busy { label } => this.error = Some(label.clone()),
                _ => {}
            }
            cx.notify();
        });
        let c = coordinator.read(cx);
        let provider = c
            .settings
            .as_ref()
            .map(|s| s.debrid)
            .unwrap_or(kinema_core::sources::DebridKind::RealDebrid);
        let state = c
            .credentials
            .iter()
            .find(|(p, _)| *p == provider)
            .map(|(_, s)| s);
        let credential_notice = match state {
            Some(CredentialState::Present) => String::new(),
            Some(CredentialState::Missing) => {
                format!("No {provider:?} credential is saved. Preparation has not started.")
            }
            Some(CredentialState::Unavailable(message)) => {
                format!("Secure storage unavailable: {message}. Preparation has not started.")
            }
            None => "Account status unavailable. Close and retry after settings load.".into(),
        };
        let missing = !credential_notice.is_empty();
        let release = candidate.name.clone();
        let pending = if missing {
            None
        } else {
            coordinator.update(cx, |c, cx| {
                c.command(AppAction::Prepare { target, candidate }, cx)
            })
        };
        Self {
            coordinator,
            title,
            player,
            _player_subscription: player_subscription,
            release,
            preparation: None,
            pending,
            player_pending: None,
            files: vec![],
            chosen: None,
            error: if !missing && pending.is_none() {
                Some("Application service unavailable or busy. Close and retry.".into())
            } else {
                None
            },
            missing,
            credential_notice,
            cancel: cx.focus_handle().tab_stop(true),
            _subscription: subscription,
        }
    }
    #[cfg(feature = "ui-probe")]
    pub(crate) fn fixture(
        coordinator: Entity<AppCoordinator>,
        state: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        use kinema_core::{
            catalog::ImdbId,
            sources::{InfoHash, TorrentSource},
        };
        let target = PlaybackTarget::Movie(
            ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("fixture id: {e}")),
        );
        let candidate = SourceCandidate {
            name: "Quiet.Orbit.1080p.WEB.mkv".into(),
            quality: Some("1080p".into()),
            size: Some(4_200_000_000),
            seeders: Some(412),
            torrent: TorrentSource {
                hash: InfoHash::new("a".repeat(40)).unwrap_or_else(|e| panic!("fixture hash: {e}")),
                file_index: None,
                filename: None,
            },
        };
        let mut this = Self::new(coordinator, "The Quiet Orbit".into(), target, candidate, cx);
        this.missing = state == "play-live-missing-creds";
        this.credential_notice =
            "No Real-Debrid credential is saved. Preparation has not started.".into();
        this.pending = (state == "play-live-preparing").then_some(100);
        this.error = (state == "play-live-failure").then(|| {
            "Player launch failed: executable not found. Your file choice is preserved.".into()
        });
        if !this.missing && this.pending.is_none() {
            this.files = vec![
                RemoteFile {
                    id: "feature".into(),
                    path: "Quiet.Orbit.mkv".into(),
                    size: 4_200_000_000,
                },
                RemoteFile {
                    id: "extra".into(),
                    path: "Extras/Interview.mkv".into(),
                    size: 400_000_000,
                },
            ];
            this.preparation = Some(99);
            if matches!(state, "play-live-ready" | "play-live-failure") {
                this.chosen = Some("feature".into());
            }
        }
        this
    }
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
            .child(div().type_style(t::MONO).child(self.release.clone()));
        if self.missing {
            let coordinator = self.coordinator.clone();
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
                        coordinator.update(cx, |c, cx| c.open_settings(cx))
                    }),
                );
        } else if self.pending.is_some() {
            body = body
                .child(if self.preparation.is_some() {
                    "Requesting player launch…"
                } else {
                    "Preparing with debrid service…"
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
                    format!("{} · {} bytes", file.path, file.size),
                    self.chosen.as_ref() == Some(&file.id),
                )
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
                .action(
                    Button::new("play-storage", "Storage settings…").on_click(move |_, _, cx| {
                        coordinator.update(cx, |c, cx| c.open_settings(cx))
                    }),
                ),
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
            div().child(format!("Player: {:?}", self.player.read(cx).value()))
        });
        // Give the modal's scroll body a definite measure; nested flex content
        // otherwise contributes only its first line to GPUI's intrinsic size.
        let body_height = (110.
            + self.files.len() as f32 * 40.
            + if self.missing { 110. } else { 0. }
            + if self.error.is_some() { 54. } else { 0. })
        .min(420.);
        let body = body.id("play-body").h(px(body_height)).overflow_y_scroll();
        Sheet::new("play-sheet", format!("Play · {}", self.title))
            .initial_focus(self.cancel.clone())
            .body(body)
            .on_dismiss(move |_, cx| {
                let _ = dismiss.update(cx, |_, cx| cx.emit(DismissEvent));
            })
            .footer(
                Button::new("play-cancel", "Cancel")
                    .focus_handle(self.cancel.clone())
                    .on_click(move |_, _, cx| {
                        let _ = cancel.update(cx, |_, cx| cx.emit(DismissEvent));
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
#[cfg(test)]
mod tests {
    use super::can_play;
    use kinema_core::sources::RemoteFile;
    #[test]
    fn explicit_choice_even_for_one_file() {
        let files = vec![RemoteFile {
            id: "a".into(),
            path: "movie.mkv".into(),
            size: 1,
        }];
        assert!(!can_play(None, &files, false));
        assert!(can_play(Some("a"), &files, false));
        assert!(!can_play(Some("a"), &files, true));
        assert!(!can_play(Some("stale"), &files, false));
    }
}
