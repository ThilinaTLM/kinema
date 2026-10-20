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
        tracing::debug!(?provider, ?target, "UI prepare/play sheet opened");
        let missing = !credential_notice.is_empty();
        if missing {
            tracing::warn!(
                message = credential_notice,
                "UI preparation blocked by account state"
            );
        }
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
            preparation: None,
            pending,
            limit_exceeded: None,
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
            self.preparation = None;
            self.pending = None;
            self.error = None;
            self.missing = true;
            self.credential_notice =
                "Account changed. Close and prepare again with the current account.".into();
            cx.notify();
            return;
        }
        if Some(event.generation) != self.pending {
            if event.generation != 0 {
                tracing::debug!(
                    generation = event.generation,
                    outcome = "rejected_stale",
                    "UI play-sheet result ignored"
                );
            }
            return;
        }
        tracing::debug!(
            generation = event.generation,
            outcome = "applied",
            "UI play-sheet result applied"
        );
        self.pending = None;
        match &event.outcome {
            AppOutcome::Prepared {
                prepared,
                selection,
                ..
            } => {
                self.preparation = Some(event.generation);
                match after_prepare(selection, &prepared.files) {
                    AfterPrepare::Play(file_id) => self.play_file(event.generation, file_id, cx),
                    AfterPrepare::Message(message) => self.error = Some(message),
                }
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
                self.limit_exceeded = Some(*limit);
                self.error = Some(format!(
                    "File is {}; cache limit is {}. Open Storage settings to increase it.",
                    format_bytes(*size),
                    format_bytes(*limit)
                ))
            }
            AppOutcome::Busy { label } => self.error = Some(label.clone()),
            _ => {}
        }
        if let Some(message) = &self.error {
            tracing::warn!(message, "UI preparation/playback error displayed");
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
    }
    /// Submit the auto-selected file; the sheet stays open until playback starts.
    fn play_file(&mut self, preparation: u64, file_id: String, cx: &mut Context<Self>) {
        tracing::debug!(preparation, provider = ?self.provider, "UI selected-file play requested");
        self.error = None;
        self.pending = self.coordinator.update(cx, |c, cx| {
            c.command(
                AppAction::PlayFile {
                    preparation,
                    file_id,
                },
                cx,
            )
        });
        if self.pending.is_none() {
            tracing::warn!(
                category = "worker_backpressure",
                "UI selected-file play could not be queued"
            );
            self.error = Some("Application service unavailable or busy. Retry.".into());
        }
    }
    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.cancel_owned_work(cx);
        cx.emit(DismissEvent);
    }
}
