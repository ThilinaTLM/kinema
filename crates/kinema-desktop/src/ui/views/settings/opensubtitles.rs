//! OpenSubtitles accounts: two ordinary credential rows (API key, login) that
//! reuse the Credentials entry flow. Values go straight to secure storage through
//! the coordinator; the view keeps only each row's status.
use super::*;
use crate::{
    app::{OpenSubtitlesAllowance, OpenSubtitlesEdit, OpenSubtitlesPart},
    credentials::OpenSubtitlesStatus,
};

/// The coordinator action for a row operation; `None` for a save without a value.
pub(super) fn action(
    part: OpenSubtitlesPart,
    operation: CredentialOperation,
    value: Option<SecretString>,
    username: Option<SecretString>,
) -> Option<AppAction> {
    Some(match (part, operation) {
        (OpenSubtitlesPart::ApiKey, CredentialOperation::Save) => {
            AppAction::EditOpenSubtitles(OpenSubtitlesEdit::ApiKey(value?))
        }
        (OpenSubtitlesPart::Login, CredentialOperation::Save) => {
            AppAction::EditOpenSubtitles(OpenSubtitlesEdit::Login {
                username: username?,
                password: value?,
            })
        }
        (OpenSubtitlesPart::ApiKey, CredentialOperation::Remove) => {
            AppAction::EditOpenSubtitles(OpenSubtitlesEdit::RemoveApiKey)
        }
        (OpenSubtitlesPart::Login, CredentialOperation::Remove) => {
            AppAction::EditOpenSubtitles(OpenSubtitlesEdit::RemoveLogin)
        }
        (part, CredentialOperation::Test) => AppAction::CheckOpenSubtitles(part),
    })
}

fn stored(present: bool) -> AccountStatus {
    if present {
        AccountStatus::Saved
    } else {
        AccountStatus::Missing
    }
}

/// Status of `account` after a successful save, remove or test.
pub(super) fn success_status(
    account: Account,
    operation: CredentialOperation,
    outcome: &AppOutcome,
) -> AccountStatus {
    match (outcome, account.opensubtitles()) {
        (AppOutcome::OpenSubtitles { status }, Some(OpenSubtitlesPart::ApiKey)) => {
            stored(status.api_key)
        }
        (AppOutcome::OpenSubtitles { status }, Some(OpenSubtitlesPart::Login)) => {
            stored(status.login)
        }
        (AppOutcome::OpenSubtitlesChecked { .. }, _) => AccountStatus::Verified,
        _ => match operation {
            CredentialOperation::Save => AccountStatus::Saved,
            CredentialOperation::Remove => AccountStatus::Missing,
            CredentialOperation::Test => AccountStatus::Verified,
        },
    }
}

/// "Signed in. 17 of 20 downloads left today." for a login test.
pub(super) fn allowance_note(allowance: &OpenSubtitlesAllowance) -> String {
    match (allowance.remaining, allowance.allowed) {
        (Some(left), Some(total)) => format!("Signed in. {left} of {total} downloads left today."),
        (Some(left), None) => format!("Signed in. {left} downloads left today."),
        _ => "Signed in.".into(),
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
            self.pending.insert(g, Pending::OpenSubtitlesLoad);
        }
    }

    pub(super) fn opensubtitles_load_outcome(
        &mut self,
        event: &crate::app::AppEvent,
        cx: &mut Context<Self>,
    ) {
        let statuses = match &event.outcome {
            AppOutcome::OpenSubtitles { status } => Some(*status),
            AppOutcome::Failed { message } => {
                let failure = AccountStatus::failure(message);
                for account in Account::OPENSUBTITLES {
                    self.status.insert(account, failure);
                }
                self.show_error(Account::OpenSubtitlesKey.label(), message.clone());
                None
            }
            _ => None,
        };
        if let Some(OpenSubtitlesStatus { api_key, login }) = statuses {
            for (account, present) in [
                (Account::OpenSubtitlesKey, api_key),
                (Account::OpenSubtitlesLogin, login),
            ] {
                // An edit or test already in flight owns its row.
                if !self.latest_account.contains_key(&account) {
                    self.status.insert(account, stored(present));
                }
            }
        }
        cx.notify();
    }

    /// Applies an OpenSubtitles row result; returns the failure to display, if any.
    pub(super) fn opensubtitles_result(
        &mut self,
        account: Account,
        operation: CredentialOperation,
        event: &crate::app::AppEvent,
        failure: Option<String>,
    ) -> AccountStatus {
        self.os_note = None;
        if let Some(message) = failure {
            let mut status = AccountStatus::failure(&message);
            // A rejected key found while testing the login belongs to the key row.
            if account == Account::OpenSubtitlesLogin
                && message
                    == crate::providers::opensubtitles::OpenSubtitlesError::ApiKeyRejected
                        .to_string()
            {
                self.status
                    .insert(Account::OpenSubtitlesKey, AccountStatus::Rejected);
                status = AccountStatus::Saved;
            }
            self.show_error(account.label(), message);
            return status;
        }
        if let AppOutcome::OpenSubtitlesChecked {
            allowance: Some(allowance),
            ..
        } = &event.outcome
        {
            self.os_note = Some(allowance_note(allowance));
        }
        success_status(account, operation, &event.outcome)
    }
}
