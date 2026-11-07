use super::*;
use ::core::prelude::v1::test;
#[test]
fn section_roving_navigation() {
    assert_eq!(Section::General.navigate("up"), Some(Section::About));
    assert_eq!(Section::About.navigate("down"), Some(Section::General));
    assert_eq!(Section::Accounts.navigate("home"), Some(Section::General));
    assert_eq!(Section::Playback.navigate("end"), Some(Section::About));
    assert_eq!(
        Section::Playback.navigate("down"),
        Some(Section::KeyboardMouse)
    );
    assert_eq!(
        Section::Storage.navigate("up"),
        Some(Section::KeyboardMouse)
    );
    assert_eq!(Section::KeyboardMouse.label(), "Keyboard & Mouse");
    assert_eq!(Section::ALL.len(), 7);
    let groups = crate::player_shortcuts::reference_groups();
    assert!(!groups.is_empty());
    assert!(
        groups
            .iter()
            .all(|g| !g.title.is_empty() && !g.entries.is_empty())
    );
    assert_eq!(Section::Storage.navigate("a"), None);
}
#[test]
fn single_instance_reopens_after_close() {
    assert_eq!(open_decision(true), OpenDecision::Activate);
    assert_eq!(open_decision(false), OpenDecision::Create);
}
#[test]
fn immediate_changes_preserve_unrelated_fields() {
    let mut settings = Settings::default();
    apply_change(&mut settings, SettingChange::Theme(ThemeChoice::Light));
    apply_change(&mut settings, SettingChange::Poster(PosterSize::Large));
    apply_change(&mut settings, SettingChange::Motion(MotionChoice::Always));
    assert_eq!(settings.theme, ThemeChoice::Light);
    assert_eq!(settings.poster_size.preferred_width(), 212.);
    assert_eq!(settings.reduce_motion, MotionChoice::Always);
    assert_eq!(settings.metadata_priority, ProviderPriority::default());
    assert_eq!(
        settings.media_limit_bytes,
        Settings::default().media_limit_bytes
    );
}
#[test]
fn provider_roles_follow_priority_and_token_state() {
    use ProviderId::{Cinemeta, Tmdb};
    let mut priority = ProviderPriority::default();
    assert_eq!(
        provider_roles(&priority, true),
        [
            (Cinemeta, ProviderRole::Primary),
            (Tmdb, ProviderRole::Enrichment)
        ]
    );
    assert_eq!(
        provider_roles(&priority, false),
        [
            (Cinemeta, ProviderRole::Primary),
            (Tmdb, ProviderRole::Inactive)
        ]
    );
    priority.move_up(Tmdb);
    assert_eq!(
        provider_roles(&priority, true),
        [
            (Tmdb, ProviderRole::Primary),
            (Cinemeta, ProviderRole::Enrichment)
        ]
    );
    assert_eq!(
        provider_roles(&priority, false),
        [
            (Tmdb, ProviderRole::Inactive),
            (Cinemeta, ProviderRole::Primary)
        ]
    );
    assert!(!tmdb_token_active(None));
    assert!(!tmdb_token_active(Some(AccountStatus::Missing)));
    assert!(!tmdb_token_active(Some(AccountStatus::Loading)));
    assert!(tmdb_token_active(Some(AccountStatus::Saved)));
}
#[test]
fn provider_moves_apply_and_clamp() {
    let mut settings = Settings::default();
    apply_change(
        &mut settings,
        SettingChange::ProviderUp(ProviderId::Cinemeta),
    );
    assert_eq!(settings.metadata_priority, ProviderPriority::default());
    apply_change(
        &mut settings,
        SettingChange::ProviderDown(ProviderId::Cinemeta),
    );
    assert_eq!(
        settings.metadata_priority.order(),
        &[ProviderId::Tmdb, ProviderId::Cinemeta]
    );
}
#[test]
fn credential_failure_is_not_empty_or_missing() {
    assert_eq!(
        AccountStatus::failure("unknown failure"),
        AccountStatus::ServiceFailure
    );
    assert_eq!(
        AccountStatus::failure(&crate::providers::DebridError::CredentialRejected.to_string()),
        AccountStatus::Rejected
    );
    assert_eq!(
        AccountStatus::snapshot(Some(&CredentialState::Missing)),
        AccountStatus::Missing
    );
    assert_eq!(
        AccountStatus::snapshot(Some(&CredentialState::Unavailable("unavailable".into()))),
        AccountStatus::Unavailable
    );
}

#[test]
fn subtitle_changes_edit_the_ordered_list_and_keep_it_valid() {
    let mut settings = Settings::default();
    apply_change(&mut settings, SettingChange::SubtitleAdd("pt"));
    apply_change(&mut settings, SettingChange::SubtitleAdd("pt"));
    apply_change(&mut settings, SettingChange::SubtitleAdd("fr"));
    assert_eq!(settings.subtitle_languages.codes(), ["en", "pt", "fr"]);
    apply_change(&mut settings, SettingChange::SubtitleUp("fr"));
    apply_change(&mut settings, SettingChange::SubtitleDown("en"));
    assert_eq!(settings.subtitle_languages.codes(), ["fr", "en", "pt"]);
    for code in ["fr", "en", "pt"] {
        apply_change(&mut settings, SettingChange::SubtitleRemove(code));
    }
    assert_eq!(
        settings.subtitle_languages.codes(),
        ["pt"],
        "last one stays"
    );
}
#[test]
fn language_add_menu_offers_only_unchosen_languages() -> Result<(), Box<dyn std::error::Error>> {
    let chosen = kinema_core::subtitle_languages::SubtitleLanguages::new(["en", "de"])?;
    let options = subtitles::add_options(&chosen);
    assert_eq!(options[0].0, None);
    assert!(
        options
            .iter()
            .skip(1)
            .all(|(code, name)| code.is_some_and(|c| !chosen.contains(c)) && !name.is_empty())
    );
    assert!(options.iter().any(|(code, _)| *code == Some("fr")));
    Ok(())
}

#[test]
fn opensubtitles_rows_map_to_the_right_actions() {
    use crate::app::{OpenSubtitlesEdit as Edit, OpenSubtitlesPart as Part};
    let secret = |s: &str| Some(SecretString::new(s.into()));
    assert!(matches!(
        opensubtitles::action(Part::ApiKey, CredentialOperation::Save, secret("k"), None),
        Some(AppAction::EditOpenSubtitles(Edit::ApiKey(_)))
    ));
    assert!(matches!(
        opensubtitles::action(
            Part::Login,
            CredentialOperation::Save,
            secret("p"),
            secret("u")
        ),
        Some(AppAction::EditOpenSubtitles(Edit::Login { .. }))
    ));
    // A login save without a username, or any save without a value, is not sent.
    assert!(
        opensubtitles::action(Part::Login, CredentialOperation::Save, secret("p"), None).is_none()
    );
    assert!(opensubtitles::action(Part::ApiKey, CredentialOperation::Save, None, None).is_none());
    assert!(matches!(
        opensubtitles::action(Part::Login, CredentialOperation::Remove, None, None),
        Some(AppAction::EditOpenSubtitles(Edit::RemoveLogin))
    ));
    assert!(matches!(
        opensubtitles::action(Part::ApiKey, CredentialOperation::Test, None, None),
        Some(AppAction::CheckOpenSubtitles(Part::ApiKey))
    ));
    assert!(matches!(
        opensubtitles::action(Part::Login, CredentialOperation::Test, None, None),
        Some(AppAction::CheckOpenSubtitles(Part::Login))
    ));
}
