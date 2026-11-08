#![allow(clippy::unwrap_used)]
use super::*;
use ::core::prelude::v1::test;

#[gpui::test]
fn secret_is_cleared_on_submission_and_backpressure(cx: &mut TestAppContext) {
    cx.update(theme::init);
    let handle = cx.add_window(|window, cx| {
        let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
        SettingsWindow::fixture(coordinator, Section::Accounts, window, cx)
    });
    handle
        .update(cx, |view, _, cx| {
            view.editing = Some(Account::Tmdb);
            view.secret
                .update(cx, |input, cx| input.set_text("fixture-only-value", cx));
            view.submit_secret(cx);
            assert_eq!(view.secret.read(cx).text(), "");
            assert!(view.editing.is_none());
            assert_eq!(view.status.get(&Account::Tmdb), Some(&AccountStatus::Saved));
            // The workerless coordinator rejects this command, yet text must
            // already be gone. A retry requires re-entering the token.
            view.fixture = false;
            view.editing = Some(Account::RealDebrid);
            view.secret
                .update(cx, |input, cx| input.set_text("fixture-only-value", cx));
            view.submit_secret(cx);
            assert_eq!(view.secret.read(cx).text(), "");
            assert!(view.errors.contains_key(Account::RealDebrid.label()));
        })
        .unwrap();
}

#[gpui::test]
fn stale_credential_results_are_ignored(cx: &mut TestAppContext) {
    cx.update(theme::init);
    let handle = cx.add_window(|window, cx| {
        let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
        SettingsWindow::fixture(coordinator, Section::Accounts, window, cx)
    });
    handle
        .update(cx, |view, _, cx| {
            for generation in [10, 11] {
                view.pending.insert(
                    generation,
                    Pending::Credential(Account::Tmdb, CredentialOperation::Test),
                );
            }
            view.latest_account.insert(Account::Tmdb, 11);
            view.outcome(
                &crate::app::AppEvent {
                    generation: 10,
                    outcome: AppOutcome::Failed {
                        message: crate::tmdb::TmdbError::RejectedCredential.to_string(),
                    },
                },
                cx,
            );
            assert_eq!(view.status.get(&Account::Tmdb), Some(&AccountStatus::Saved));
            view.outcome(
                &crate::app::AppEvent {
                    generation: 11,
                    outcome: AppOutcome::TmdbCredentialChecked,
                },
                cx,
            );
            assert_eq!(
                view.status.get(&Account::Tmdb),
                Some(&AccountStatus::Verified)
            );
        })
        .unwrap();
}

#[gpui::test]
fn failed_save_reconciles_preview_without_a_settings_view(cx: &mut TestAppContext) {
    cx.update(theme::init);
    let coordinator = cx.new(|cx| {
        let mut state = AppCoordinator::fixture(Vec::new(), None, cx);
        state.settings = Some(Settings::default());
        state
    });
    cx.update(|cx| {
        install(&coordinator, cx);
        cx.global_mut::<SettingsWindows>().pending_save = Some(17);
        theme::apply_preferences(ThemeChoice::Light, MotionChoice::Always, cx);
    });
    coordinator.update(cx, |_, cx| {
        cx.emit(CoordinatorEvent::App(crate::app::AppEvent {
            generation: 17,
            outcome: AppOutcome::Failed {
                message: "fixture save failure".into(),
            },
        }))
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(theme::theme(cx).mode, theme::Mode::Dark);
        assert!(!theme::theme(cx).reduce_motion);
        assert_eq!(cx.global::<SettingsWindows>().pending_save, None);
    });
}

#[gpui::test]
fn settings_window_is_reused_and_closing_does_not_quit_main(cx: &mut TestAppContext) {
    cx.update(theme::init);
    let main = cx.add_window(TextInput::new);
    let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
    cx.update(|cx| {
        install(&coordinator, cx);
        open(coordinator.clone(), Section::Accounts, cx);
        let first = cx.global::<SettingsWindows>().window.unwrap();
        open(coordinator.clone(), Section::About, cx);
        assert_eq!(first, cx.global::<SettingsWindows>().window.unwrap());
        first
            .update(cx, |view, window, cx| {
                assert_eq!(view.section, Section::About);
                window.remove_window();
                cx.notify();
            })
            .unwrap();
    });
    cx.run_until_parked();
    assert!(main.update(cx, |_, _, _| ()).is_ok());
    assert!(!coordinator.read_with(cx, |state, _| state.quit_requested));
    cx.update(|cx| {
        open(coordinator.clone(), Section::Storage, cx);
        cx.global::<SettingsWindows>()
            .window
            .unwrap()
            .update(cx, |view, _, _| {
                assert_eq!(view.section, Section::Storage);
            })
            .unwrap();
    });
}

fn os_event(generation: u64, outcome: AppOutcome) -> crate::app::AppEvent {
    crate::app::AppEvent {
        generation,
        outcome,
    }
}
fn os_failed(error: impl ToString) -> AppOutcome {
    AppOutcome::Failed {
        message: error.to_string(),
    }
}

#[gpui::test]
fn opensubtitles_test_results_set_each_rows_status(cx: &mut TestAppContext) {
    use crate::app::{OpenSubtitlesAllowance, OpenSubtitlesPart};
    use crate::credentials::CredentialError;
    use crate::providers::opensubtitles::OpenSubtitlesError as E;
    cx.update(theme::init);
    let handle = cx.add_window(|window, cx| {
        let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
        SettingsWindow::fixture(coordinator, Section::Accounts, window, cx)
    });
    handle
        .update(cx, |view, _, cx| {
            let key = Account::OpenSubtitlesKey;
            let login = Account::OpenSubtitlesLogin;
            let mut generation = 100;
            let mut run = |view: &mut SettingsWindow,
                           account,
                           operation,
                           outcome: AppOutcome,
                           cx: &mut Context<SettingsWindow>| {
                generation += 1;
                view.pending
                    .insert(generation, Pending::Credential(account, operation));
                view.latest_account.insert(account, generation);
                view.outcome(&os_event(generation, outcome), cx);
                assert!(!view.latest_account.contains_key(&account));
            };
            let checked = |part, allowance| AppOutcome::OpenSubtitlesChecked { part, allowance };
            view.status.insert(key, AccountStatus::Saved);
            view.status.insert(login, AccountStatus::Saved);

            run(
                view,
                key,
                CredentialOperation::Test,
                checked(OpenSubtitlesPart::ApiKey, None),
                cx,
            );
            assert_eq!(view.status[&key], AccountStatus::Verified);
            assert!(view.os_note.is_none());

            let allowance = OpenSubtitlesAllowance {
                remaining: Some(17),
                allowed: Some(20),
            };
            run(
                view,
                login,
                CredentialOperation::Test,
                checked(OpenSubtitlesPart::Login, Some(allowance)),
                cx,
            );
            assert_eq!(view.status[&login], AccountStatus::Verified);
            assert_eq!(
                view.os_note.as_deref(),
                Some("Signed in. 17 of 20 downloads left today.")
            );

            // Each failure keeps its own meaning.
            run(
                view,
                key,
                CredentialOperation::Test,
                os_failed(E::ApiKeyRejected),
                cx,
            );
            assert_eq!(view.status[&key], AccountStatus::Rejected);
            run(
                view,
                login,
                CredentialOperation::Test,
                os_failed(E::LoginRejected),
                cx,
            );
            assert_eq!(view.status[&login], AccountStatus::Rejected);
            assert!(view.os_note.is_none());
            run(
                view,
                key,
                CredentialOperation::Test,
                os_failed(E::ServiceFailure),
                cx,
            );
            assert_eq!(view.status[&key], AccountStatus::ServiceFailure);
            run(
                view,
                key,
                CredentialOperation::Test,
                os_failed(E::Network),
                cx,
            );
            assert_eq!(view.status[&key], AccountStatus::ServiceFailure);
            run(
                view,
                key,
                CredentialOperation::Test,
                os_failed(CredentialError::Missing),
                cx,
            );
            assert_eq!(view.status[&key], AccountStatus::Missing);
            run(
                view,
                key,
                CredentialOperation::Test,
                os_failed(CredentialError::Unavailable),
                cx,
            );
            assert_eq!(view.status[&key], AccountStatus::Unavailable);
            assert!(view.errors.contains_key(key.label()));

            // A bad key found while testing the login marks the key, not the login.
            view.status.insert(key, AccountStatus::Saved);
            run(
                view,
                login,
                CredentialOperation::Test,
                os_failed(E::ApiKeyRejected),
                cx,
            );
            assert_eq!(view.status[&key], AccountStatus::Rejected);
            assert_eq!(view.status[&login], AccountStatus::Saved);
        })
        .unwrap();
}

#[gpui::test]
fn opensubtitles_save_remove_and_load_results_follow_stored_parts(cx: &mut TestAppContext) {
    use crate::credentials::{CredentialError, OpenSubtitlesStatus};
    cx.update(theme::init);
    let handle = cx.add_window(|window, cx| {
        let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
        SettingsWindow::fixture(coordinator, Section::Accounts, window, cx)
    });
    handle
        .update(cx, |view, _, cx| {
            let key = Account::OpenSubtitlesKey;
            let login = Account::OpenSubtitlesLogin;
            let stored = |api_key, login| AppOutcome::OpenSubtitles {
                status: OpenSubtitlesStatus { api_key, login },
            };
            for (account, operation, outcome, expected) in [
                (
                    login,
                    CredentialOperation::Save,
                    stored(true, true),
                    AccountStatus::Saved,
                ),
                (
                    key,
                    CredentialOperation::Remove,
                    stored(false, true),
                    AccountStatus::Missing,
                ),
                (
                    login,
                    CredentialOperation::Remove,
                    stored(false, false),
                    AccountStatus::Missing,
                ),
                (
                    key,
                    CredentialOperation::Save,
                    os_failed(CredentialError::AccessDenied),
                    AccountStatus::Unavailable,
                ),
            ] {
                view.pending
                    .insert(7, Pending::Credential(account, operation));
                view.latest_account.insert(account, 7);
                view.outcome(&os_event(7, outcome), cx);
                assert_eq!(view.status[&account], expected);
            }
            // Load sets both rows, but never overwrites a row with an operation in flight.
            view.latest_account.insert(login, 9);
            view.pending.insert(8, Pending::OpenSubtitlesLoad);
            view.outcome(&os_event(8, stored(true, true)), cx);
            assert_eq!(view.status[&key], AccountStatus::Saved);
            assert_eq!(view.status[&login], AccountStatus::Missing);
            view.latest_account.clear();
            view.pending.insert(10, Pending::OpenSubtitlesLoad);
            view.outcome(&os_event(10, os_failed(CredentialError::Unavailable)), cx);
            for account in Account::OPENSUBTITLES {
                assert_eq!(view.status[&account], AccountStatus::Unavailable);
            }
        })
        .unwrap();
}

#[gpui::test]
fn opensubtitles_login_entry_needs_both_fields_and_clears_them(cx: &mut TestAppContext) {
    cx.update(theme::init);
    let handle = cx.add_window(|window, cx| {
        let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
        SettingsWindow::fixture(coordinator, Section::Accounts, window, cx)
    });
    handle
        .update(cx, |view, _, cx| {
            let login = Account::OpenSubtitlesLogin;
            view.editing = Some(login);
            view.secret_user
                .update(cx, |input, cx| input.set_text("someone", cx));
            view.submit_secret(cx);
            assert!(view.errors.contains_key(login.label()), "password missing");
            assert_eq!(view.secret_user.read(cx).text(), "");
            assert_eq!(view.status[&login], AccountStatus::Missing);

            view.errors.clear();
            view.editing = Some(login);
            view.secret_user
                .update(cx, |input, cx| input.set_text("someone", cx));
            view.secret
                .update(cx, |input, cx| input.set_text("fixture-password", cx));
            view.submit_secret(cx);
            assert!(view.errors.is_empty());
            assert_eq!(view.secret.read(cx).text(), "");
            assert_eq!(view.secret_user.read(cx).text(), "");
            assert_eq!(view.status[&login], AccountStatus::Saved);
        })
        .unwrap();
}
