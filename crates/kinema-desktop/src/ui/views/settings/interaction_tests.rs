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
