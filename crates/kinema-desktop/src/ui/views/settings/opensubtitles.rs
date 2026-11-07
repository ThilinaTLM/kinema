//! OpenSubtitles account (Accounts page): the user's own API key, optional login.
//! Values go straight to secure storage through the coordinator; the view keeps
//! only which parts exist.
use super::*;
use crate::{
    app::{OpenSubtitlesAllowance, OpenSubtitlesEdit},
    credentials::OpenSubtitlesStatus,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OpenSubtitlesPart {
    ApiKey,
    Login,
}

/// What the view knows about OpenSubtitles credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OpenSubtitlesState {
    Loading,
    Ready(OpenSubtitlesStatus),
    /// Secure storage could not be read; distinct from "nothing saved".
    Unavailable,
}

pub(super) const OPENSUBTITLES_ERROR: &str = "OpenSubtitles";

fn part_status(
    state: OpenSubtitlesState,
    present: impl Fn(OpenSubtitlesStatus) -> bool,
) -> (&'static str, bool) {
    match state {
        OpenSubtitlesState::Loading => ("Loading status…", false),
        OpenSubtitlesState::Unavailable => ("Secure storage unavailable", false),
        OpenSubtitlesState::Ready(status) if present(status) => ("Saved (not checked)", true),
        OpenSubtitlesState::Ready(_) => ("Missing", false),
    }
}

impl SettingsWindow {
    /// Reads which credentials exist, on demand (never at startup).
    pub(super) fn load_opensubtitles(&mut self, cx: &mut Context<Self>) {
        if self.fixture {
            return;
        }
        if let Some(g) = self.coordinator.update(cx, |state, cx| {
            state.command(AppAction::LoadOpenSubtitles, cx)
        }) {
            self.pending.insert(g, Pending::OpenSubtitles(false));
        }
    }

    pub(super) fn opensubtitles_edit(
        &mut self,
        part: OpenSubtitlesPart,
        remove: bool,
        cx: &mut Context<Self>,
    ) {
        self.errors.remove(OPENSUBTITLES_ERROR);
        self.os_note = None;
        let edit = match (part, remove) {
            (OpenSubtitlesPart::ApiKey, true) => OpenSubtitlesEdit::RemoveApiKey,
            (OpenSubtitlesPart::Login, true) => OpenSubtitlesEdit::RemoveLogin,
            (OpenSubtitlesPart::ApiKey, false) => {
                // Taking the value clears the field whether or not it is accepted.
                let key = self.os_api_key.update(cx, take_secret);
                if key.expose().trim().is_empty() {
                    self.show_error(OPENSUBTITLES_ERROR, "API key cannot be empty.".into());
                    cx.notify();
                    return;
                }
                OpenSubtitlesEdit::ApiKey(SecretString::new(key.expose().trim().to_owned()))
            }
            (OpenSubtitlesPart::Login, false) => {
                let username = self.os_username.update(cx, take_secret);
                let password = self.os_password.update(cx, take_secret);
                if username.expose().trim().is_empty() || password.expose().is_empty() {
                    self.show_error(
                        OPENSUBTITLES_ERROR,
                        "Enter both a username and a password.".into(),
                    );
                    cx.notify();
                    return;
                }
                OpenSubtitlesEdit::Login {
                    username: SecretString::new(username.expose().trim().to_owned()),
                    password,
                }
            }
        };
        if self.fixture {
            if let OpenSubtitlesState::Ready(mut status) = self.os_state {
                match (part, remove) {
                    (OpenSubtitlesPart::ApiKey, remove) => status.api_key = !remove,
                    (OpenSubtitlesPart::Login, remove) => status.login = !remove,
                }
                self.os_state = OpenSubtitlesState::Ready(status);
            }
            cx.notify();
            return;
        }
        if let Some(g) = self.coordinator.update(cx, |state, cx| {
            state.command(AppAction::EditOpenSubtitles(edit), cx)
        }) {
            self.pending.insert(g, Pending::OpenSubtitles(true));
            self.os_busy += 1;
        } else {
            self.show_error(
                OPENSUBTITLES_ERROR,
                "Application service unavailable or busy. Retry.".into(),
            );
        }
        cx.notify();
    }

    /// Runs `AppAction::CheckOpenSubtitles` (login + allowance, or a key-proving
    /// search) on the coordinator worker; the result arrives as `Pending::OpenSubtitlesCheck`.
    pub(super) fn check_opensubtitles(&mut self, cx: &mut Context<Self>) {
        self.errors.remove(OPENSUBTITLES_ERROR);
        self.os_note = None;
        if self.fixture {
            self.os_note = Some("Fixture: key accepted. 18 of 20 downloads left today.".into());
            cx.notify();
            return;
        }
        if let Some(g) = self.coordinator.update(cx, |state, cx| {
            state.command(AppAction::CheckOpenSubtitles, cx)
        }) {
            self.pending.insert(g, Pending::OpenSubtitlesCheck);
            self.os_checking = true;
        } else {
            self.show_error(
                OPENSUBTITLES_ERROR,
                "Application service unavailable or busy. Retry.".into(),
            );
        }
        cx.notify();
    }

    pub(super) fn opensubtitles_check_outcome(
        &mut self,
        event: &crate::app::AppEvent,
        cx: &mut Context<Self>,
    ) {
        self.os_checking = false;
        match &event.outcome {
            AppOutcome::OpenSubtitlesChecked { allowance } => {
                self.os_note = Some(match allowance {
                    Some(OpenSubtitlesAllowance {
                        remaining: Some(left),
                        allowed: Some(total),
                    }) => format!("Signed in. {left} of {total} downloads left today."),
                    Some(OpenSubtitlesAllowance {
                        remaining: Some(left),
                        ..
                    }) => format!("Signed in. {left} downloads left today."),
                    Some(_) => "Signed in.".into(),
                    None => "API key accepted.".into(),
                });
            }
            AppOutcome::Failed { message } => {
                self.show_error(OPENSUBTITLES_ERROR, message.clone());
            }
            AppOutcome::Busy { label } => {
                self.show_error(OPENSUBTITLES_ERROR, format!("Busy: {label}"));
            }
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn opensubtitles_outcome(
        &mut self,
        event: &crate::app::AppEvent,
        edit: bool,
        cx: &mut Context<Self>,
    ) {
        if edit {
            self.os_busy = self.os_busy.saturating_sub(1);
        }
        // Results arrive in submission order; ignore anything older than applied.
        if event.generation < self.os_applied {
            return;
        }
        match &event.outcome {
            AppOutcome::OpenSubtitles { status } => {
                self.os_applied = event.generation;
                self.os_state = OpenSubtitlesState::Ready(*status);
            }
            AppOutcome::Failed { message } => {
                if AccountStatus::failure(message) == AccountStatus::Unavailable {
                    self.os_state = OpenSubtitlesState::Unavailable;
                }
                self.show_error(OPENSUBTITLES_ERROR, message.clone());
            }
            AppOutcome::Busy { label } => {
                self.show_error(OPENSUBTITLES_ERROR, format!("Busy: {label}"));
            }
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn opensubtitles_block(&self, p: theme::Palette, cx: &Context<Self>) -> Div {
        let weak = cx.entity().downgrade();
        let busy = self.os_busy > 0;
        let status_line =
            |label: &'static str, input: &Entity<TextInput>, text_ok: (&'static str, bool)| {
                let (status, saved) = text_ok;
                div()
                    .flex()
                    .items_center()
                    .gap(s::S16)
                    .child(div().w(s::S48 * 2. + s::S16).flex_none().child(label))
                    .child(
                        div()
                            .w(size::CONTROL_SM * 10.)
                            .flex_none()
                            .child(input.clone()),
                    )
                    .child(
                        div()
                            .text(TextSize::Sm)
                            .text_color(if saved {
                                p.warning
                            } else {
                                p.subtle_foreground
                            })
                            .child(status),
                    )
            };
        let key_status = part_status(self.os_state, |s| s.api_key);
        let login_status = part_status(self.os_state, |s| s.login);
        let (key_save, key_remove, login_save, login_remove, check) =
            (weak.clone(), weak.clone(), weak.clone(), weak.clone(), weak);
        let button_row = |save: Button, remove: Button| {
            div()
                .flex()
                .gap(s::S8)
                .ml(s::S48 * 2. + s::S32)
                .child(save)
                .child(remove)
        };
        div()
            .flex()
            .flex_col()
            .gap(s::S12)
            .border_t(size::HAIRLINE)
            .border_color(p.border)
            .pt(s::S16)
            .child(
                div()
                    .text(TextSize::Sm)
                    .weight(Weight::Medium)
                    .text_color(p.muted_foreground)
                    .child("OpenSubtitles"),
            )
            .child(text(
                "Online subtitle search needs your own API key. Create one at opensubtitles.com → API Consumers. A login is optional and gives you a personal download allowance.",
                p.subtle_foreground,
            ))
            .child(status_line("API key", &self.os_api_key, key_status))
            .child(button_row(
                Button::new("opensubtitles-key-save", "Save key")
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        let _ = key_save.update(cx, |this, cx| {
                            this.opensubtitles_edit(OpenSubtitlesPart::ApiKey, false, cx)
                        });
                    }),
                Button::new("opensubtitles-key-remove", "Remove key")
                    .disabled(busy || !key_status.1)
                    .on_click(move |_, _, cx| {
                        let _ = key_remove.update(cx, |this, cx| {
                            this.opensubtitles_edit(OpenSubtitlesPart::ApiKey, true, cx)
                        });
                    }),
            ))
            .child(status_line("Username", &self.os_username, login_status))
            .child(status_line("Password", &self.os_password, ("", false)))
            .child(button_row(
                Button::new("opensubtitles-login-save", "Save login")
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        let _ = login_save.update(cx, |this, cx| {
                            this.opensubtitles_edit(OpenSubtitlesPart::Login, false, cx)
                        });
                    }),
                Button::new("opensubtitles-login-remove", "Remove login")
                    .disabled(busy || !login_status.1)
                    .on_click(move |_, _, cx| {
                        let _ = login_remove.update(cx, |this, cx| {
                            this.opensubtitles_edit(OpenSubtitlesPart::Login, true, cx)
                        });
                    }),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S12)
                    .child(
                        Button::new("opensubtitles-check", "Check")
                            .disabled(busy || self.os_checking || !key_status.1)
                            .busy(self.os_checking)
                            .on_click(move |_, _, cx| {
                                let _ = check.update(cx, |this, cx| this.check_opensubtitles(cx));
                            }),
                    )
                    .children(self.os_note.clone().map(|note| {
                        div()
                            .text(TextSize::Sm)
                            .text_color(p.success)
                            .child(note)
                    })),
            )
    }
}
