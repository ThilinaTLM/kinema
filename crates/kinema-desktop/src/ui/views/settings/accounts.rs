//! Accounts section: credentials, provider order and TMDB status.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Account {
    Tmdb,
    RealDebrid,
    AllDebrid,
}
impl Account {
    pub(super) const ALL: [Self; 3] = [Self::Tmdb, Self::RealDebrid, Self::AllDebrid];
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Tmdb => "TMDB token",
            Self::RealDebrid => "Real-Debrid",
            Self::AllDebrid => "AllDebrid",
        }
    }
    pub(super) fn provider(self) -> Option<DebridKind> {
        match self {
            Self::Tmdb => None,
            Self::RealDebrid => Some(DebridKind::RealDebrid),
            Self::AllDebrid => Some(DebridKind::AllDebrid),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CredentialOperation {
    Save,
    Remove,
    Test,
}
#[derive(Clone, Copy)]
pub(super) enum Pending {
    Save(&'static str),
    Credential(Account, CredentialOperation),
    Cleanup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AccountStatus {
    Loading,
    Missing,
    Saved,
    Verified,
    Rejected,
    ServiceFailure,
    Unavailable,
}
impl AccountStatus {
    pub(super) fn label(self) -> &'static str {
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
    pub(super) fn snapshot(state: Option<&CredentialState>) -> Self {
        match state {
            None => Self::Loading,
            Some(CredentialState::Present) => Self::Saved,
            Some(CredentialState::Missing) => Self::Missing,
            Some(CredentialState::Unavailable(_)) => Self::Unavailable,
        }
    }
    pub(super) fn failure(message: &str) -> Self {
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

impl SettingsWindow {
    pub(super) fn credential(
        &mut self,
        account: Account,
        operation: CredentialOperation,
        value: Option<SecretString>,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(
            account = account.label(),
            operation = match operation {
                CredentialOperation::Save => "save",
                CredentialOperation::Remove => "remove",
                CredentialOperation::Test => "test",
            },
            "UI credential action requested"
        );
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
            self.show_error(
                account.label(),
                "Application service unavailable or busy. Retry.".into(),
            );
        }
        cx.notify();
    }
    pub(super) fn submit_secret(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.editing.take() else {
            return;
        };
        let value = self.secret.update(cx, take_secret);
        if value.expose().trim().is_empty() {
            self.show_error(account.label(), "Token cannot be empty.".into());
            cx.notify();
            return;
        }
        self.credential(account, CredentialOperation::Save, Some(value), cx);
    }
    pub(super) fn outcome(&mut self, event: &crate::app::AppEvent, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.remove(&event.generation) else {
            return;
        };
        tracing::debug!(generation = event.generation, "UI settings result received");
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
                    self.show_error(field, message);
                }
                // Reconcile the preview with the accepted snapshot. Durability
                // warnings carry the newly replaced settings and are not rolled
                // back; rejected saves restore the last accepted appearance.
                self.sync(cx);
                theme::apply_preferences(self.settings.theme, self.settings.reduce_motion, cx);
            }
            Pending::Credential(account, operation) => {
                if self.latest_account.get(&account) != Some(&event.generation) {
                    tracing::debug!(
                        generation = event.generation,
                        outcome = "rejected_stale",
                        "UI credential result ignored"
                    );
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
                    self.show_error(account.label(), message);
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
                if let Some(message) = failure {
                    self.show_error("Cleanup", message);
                } else {
                    tracing::debug!("UI settings cleanup completed");
                    let message = match &event.outcome {
                        AppOutcome::Cleaned { bytes, .. } => format!(
                            "Freed {}. Active/protected files were kept; original local files are never removed.",
                            bytes_label(*bytes)
                        ),
                        _ => "Cleanup completed.".into(),
                    };
                    self.errors.insert("Cleanup", message);
                }
            }
        }
        cx.notify();
    }
    pub(super) fn account_row(&self, account: Account, cx: &Context<Self>) -> AnyElement {
        let p = *theme::palette(cx);
        let status = self
            .status
            .get(&account)
            .copied()
            .unwrap_or(AccountStatus::Loading);
        let ink = match status {
            AccountStatus::Verified => p.success,
            AccountStatus::Saved => p.warning,
            AccountStatus::Rejected
            | AccountStatus::ServiceFailure
            | AccountStatus::Unavailable => p.destructive,
            AccountStatus::Loading | AccountStatus::Missing => p.subtle_foreground,
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
                            .text(TextSize::Sm)
                            .text_color(ink)
                            .child(div().size(s::S8).flex_none().bg(ink))
                            .child(status.label()),
                    )
                    .child(
                        Button::new(
                            ("credential-edit", account as usize),
                            if status == AccountStatus::Missing {
                                "Add"
                            } else {
                                "Replace"
                            },
                        )
                        .disabled(pending)
                        .focus_handle(self.account_focus[&account].clone())
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
    /// Whether the TMDB token is saved; a still-loading status counts as inactive.
    pub(super) fn tmdb_active(&self) -> bool {
        tmdb_token_active(self.status.get(&Account::Tmdb).copied())
    }
    pub(super) fn provider_priority(&self, cx: &Context<Self>) -> AnyElement {
        let p = *theme::palette(cx);
        let weak = cx.entity().downgrade();
        let roles = provider_roles(&self.settings.metadata_priority, self.tmdb_active());
        let last = roles.len().saturating_sub(1);
        let mut list = div()
            .flex()
            .flex_col()
            .gap(s::S4)
            .w(size::CONTROL_SM * 14.)
            .child(
                div()
                    .text(TextSize::Sm)
                    .text_color(p.muted_foreground)
                    .child("Metadata providers, highest priority first"),
            );
        for (index, (id, role)) in roles.into_iter().enumerate() {
            let tmdb_hint = id == ProviderId::Tmdb
                && matches!(
                    self.status.get(&Account::Tmdb),
                    Some(AccountStatus::Missing | AccountStatus::Unavailable)
                );
            let ink = if role == ProviderRole::Inactive {
                p.subtle_foreground
            } else {
                p.foreground
            };
            let (up, down) = (weak.clone(), weak.clone());
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S8)
                    .px(s::S12)
                    .py(s::S4)
                    .min_h(size::CONTROL_SM * 1.5)
                    .bg(p.card)
                    .border(size::HAIRLINE)
                    .border_color(p.border)
                    .child(
                        div()
                            .w(s::S48 * 2.)
                            .flex_none()
                            .text(TextSize::Sm)
                            .text_color(p.muted_foreground)
                            .child(role.label()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .text_color(ink)
                            .child(id.display_name())
                            .when(tmdb_hint, |el| {
                                el.child(
                                    div()
                                        .text(TextSize::Sm)
                                        .text_color(p.warning)
                                        .child("Inactive — needs token"),
                                )
                            }),
                    )
                    .child(
                        Button::icon_button(
                            ("provider-up", index),
                            IconName::ChevronUp,
                            format!("Move {} up", id.display_name()),
                        )
                        .disabled(index == 0)
                        .on_click(move |_, _, cx| {
                            let _ = up.update(cx, |this, cx| {
                                this.change("Metadata", SettingChange::ProviderUp(id), cx)
                            });
                        }),
                    )
                    .child(
                        Button::icon_button(
                            ("provider-down", index),
                            IconName::ChevronDown,
                            format!("Move {} down", id.display_name()),
                        )
                        .disabled(index == last)
                        .on_click(move |_, _, cx| {
                            let _ = down.update(cx, |this, cx| {
                                this.change("Metadata", SettingChange::ProviderDown(id), cx)
                            });
                        }),
                    ),
            );
        }
        list.into_any_element()
    }
}
pub(super) fn take_secret(input: &mut TextInput, cx: &mut Context<TextInput>) -> SecretString {
    let value = SecretString::new(input.text().to_owned());
    clear_secret(input, cx);
    value
}
pub(super) fn clear_secret(input: &mut TextInput, cx: &mut Context<TextInput>) {
    // Force replacement even if the user deleted all text: set_text("") alone
    // is a no-op on an empty buffer and could leave secrets in undo history.
    input.set_text(" ", cx);
    input.set_text(String::new(), cx);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProviderRole {
    Primary,
    Enrichment,
    Inactive,
}
impl ProviderRole {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Enrichment => "Enrichment",
            Self::Inactive => "Inactive",
        }
    }
}
/// TMDB is usable once a token exists in secure storage (even if untested).
pub(super) fn tmdb_token_active(status: Option<AccountStatus>) -> bool {
    matches!(
        status,
        Some(
            AccountStatus::Saved
                | AccountStatus::Verified
                | AccountStatus::Rejected
                | AccountStatus::ServiceFailure
        )
    )
}
pub(super) fn provider_roles(
    priority: &ProviderPriority,
    tmdb_active: bool,
) -> Vec<(ProviderId, ProviderRole)> {
    let active = |id: ProviderId| id != ProviderId::Tmdb || tmdb_active;
    let primary = priority.primary(active);
    priority
        .order()
        .iter()
        .map(|&id| {
            let role = if !active(id) {
                ProviderRole::Inactive
            } else if Some(id) == primary {
                ProviderRole::Primary
            } else {
                ProviderRole::Enrichment
            };
            (id, role)
        })
        .collect()
}

impl SettingsWindow {
    pub(super) fn accounts_section(
        &self,
        mut body: Div,
        p: theme::Palette,
        weak: WeakEntity<Self>,
        cx: &Context<Self>,
    ) -> Div {
        body = body
            .child(self.provider_priority(cx))
            .child(row("Streams", self.indexer.clone()))
            .child(row("Debrid", self.debrid.clone()));
        let mut accounts = div()
            .flex()
            .flex_col()
            .border_t(size::HAIRLINE)
            .border_color(p.border)
            .pt(s::S16)
            .child(
                div()
                    .mb(s::S16)
                    .text(TextSize::Sm)
                    .weight(Weight::Medium)
                    .text_color(p.muted_foreground)
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
                    .bg(p.card)
                    .border(size::HAIRLINE)
                    .border_color(p.border)
                    .child(format!("Replace {}", account.label()))
                    .child(div().w(size::CONTROL_SM * 10.).child(self.secret.clone()))
                    .child(
                        div()
                            .flex()
                            .gap(s::S8)
                            .child(Button::new("submit-secret", "Store token").on_click(
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| this.submit_secret(cx));
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
        body
    }
}
