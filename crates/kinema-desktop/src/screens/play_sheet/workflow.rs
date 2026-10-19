//! Preparation ownership, account invalidation, and worker-event transitions.
use super::*;

impl PlaySheet {
    pub(crate) fn new(
        coordinator: Entity<AppCoordinator>,
        title: String,
        context: PlayContext,
        target: PlaybackTarget,
        candidate: SourceCandidate,
        cx: &mut Context<Self>,
    ) -> Self {
        // Navigation/window teardown can drop the view without a Sheet dismiss event.
        cx.on_release(|this, cx| this.cancel_owned_work(cx))
            .detach();
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
                        if let Some(choice) = player_coordinator
                            .read(cx)
                            .settings
                            .as_ref()
                            .map(|s| s.player.clone())
                        {
                            this.player.update(cx, |p, cx| p.set_value(choice, cx));
                        }
                    }
                    cx.notify();
                }
            },
        );
        let subscription = cx.subscribe(&coordinator, |this, _, event: &CoordinatorEvent, cx| {
            if let CoordinatorEvent::App(event) = event {
                this.apply_event(event, cx);
            }
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
                format!(
                    "No {} credential is saved. Preparation has not started.",
                    provider_name(provider)
                )
            }
            Some(CredentialState::Unavailable(message)) => {
                format!("Secure storage unavailable: {message}. Preparation has not started.")
            }
            None => "Account status unavailable. Close and retry after settings load.".into(),
        };
        let missing = !credential_notice.is_empty();
        let shown = candidate.clone();
        let pending = if missing {
            None
        } else {
            coordinator.update(cx, |c, cx| c.prepare(title.clone(), target, candidate, cx))
        };
        Self {
            coordinator,
            title,
            context,
            candidate: shown,
            provider,
            player,
            _player_subscription: player_subscription,
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
    fn apply_event(&mut self, event: &crate::app::AppEvent, cx: &mut Context<Self>) {
        if let AppOutcome::Settings { settings, .. }
        | AppOutcome::SettingsWarning { settings, .. } = &event.outcome
        {
            self.provider = settings.debrid;
        }
        if matches!(event.outcome, AppOutcome::Invalidated { .. }) {
            self.files.clear();
            self.chosen = None;
            self.preparation = None;
            self.pending = None;
            self.error = None;
            self.missing = true;
            self.credential_notice =
                "Account changed. Close and prepare again with the current account.".into();
            cx.notify();
            return;
        }
        if Some(event.generation) == self.player_pending {
            self.player_pending = None;
            if !matches!(
                event.outcome,
                AppOutcome::Settings { .. } | AppOutcome::SettingsWarning { .. }
            ) {
                self.error = Some(match &event.outcome {
                    AppOutcome::Failed { message } => message.clone(),
                    _ => "Player preference could not be saved.".into(),
                });
                if let Some(choice) = self
                    .coordinator
                    .read(cx)
                    .settings
                    .as_ref()
                    .map(|settings| settings.player.clone())
                {
                    self.player
                        .update(cx, |player, cx| player.set_value(choice, cx));
                }
            }
            cx.notify();
            return;
        }
        if Some(event.generation) != self.pending {
            return;
        }
        self.pending = None;
        match &event.outcome {
            AppOutcome::Prepared { prepared, .. } => {
                self.preparation = Some(event.generation);
                self.files = prepared.files.clone();
                self.chosen = auto_choice(&self.files);
            }
            AppOutcome::PlaybackStarted { .. } => {
                if let Some(preparation) = self.preparation.take() {
                    self.coordinator.update(cx, |coordinator, cx| {
                        coordinator.cancel_preparation(preparation, cx);
                    });
                }
                cx.emit(DismissEvent);
            }
            AppOutcome::Failed { message } | AppOutcome::CacheFull { message } => {
                self.error = Some(message.clone())
            }
            AppOutcome::CacheLimitNeeded { size, limit } => {
                self.error = Some(format!(
                    "File is {}; cache limit is {}. Open Storage settings to increase it.",
                    format_bytes(*size),
                    format_bytes(*limit)
                ))
            }
            AppOutcome::Busy { label } => self.error = Some(label.clone()),
            _ => {}
        }
        cx.notify();
    }
    fn cancel_owned_work(&mut self, cx: &mut App) {
        let preparation = self.preparation.or(self.pending);
        let launch = self.preparation.and(self.pending);
        self.coordinator.update(cx, |c, cx| {
            if let Some(generation) = preparation {
                c.cancel_preparation(generation, cx);
            }
            if let Some(generation) = launch {
                c.cancel_launch(generation, cx);
            }
        });
        self.pending = None;
        self.preparation = None;
        self.files.clear();
        self.chosen = None;
    }
    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.cancel_owned_work(cx);
        cx.emit(DismissEvent);
    }
}
