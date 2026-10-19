use super::{auto_choice, can_play, split_path};
use kinema_core::sources::RemoteFile;
#[test]
fn play_needs_a_valid_choice_and_no_pending_work() {
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
#[test]
fn byte_labels_are_bounded_and_provider_names_are_human_readable() {
    assert_eq!(super::format_bytes(0), "0 B");
    assert_eq!(super::format_bytes(1024), "1.0 KiB");
    assert_eq!(super::format_bytes(1_048_576), "1.0 MiB");
    assert_eq!(super::format_bytes(1_073_741_824), "1.0 GiB");
    assert_eq!(
        super::provider_name(kinema_core::sources::DebridKind::RealDebrid),
        "Real-Debrid"
    );
}
#[cfg(feature = "ui-probe-test")]
#[test]
fn account_invalidation_clears_obsolete_file_choices() {
    use gpui::{AppContext, TestAppContext};
    let mut cx = TestAppContext::single();
    let coordinator = cx.new(|cx| super::AppCoordinator::fixture(vec![], None, cx));
    let sheet = cx.new(|cx| super::PlaySheet::fixture(coordinator.clone(), "play-live-ready", cx));
    coordinator.update(&mut cx, |_, cx| {
        cx.emit(super::CoordinatorEvent::App(crate::app::AppEvent {
            generation: 0,
            outcome: super::AppOutcome::Invalidated { released: vec![] },
        }))
    });
    cx.run_until_parked();
    sheet.update(&mut cx, |sheet, _| {
        assert!(sheet.files.is_empty());
        assert!(sheet.chosen.is_none());
        assert!(sheet.preparation.is_none());
        assert!(sheet.pending.is_none());
        assert!(sheet.missing);
    });
}
#[test]
fn a_single_file_is_chosen_automatically_but_several_are_not() {
    let file = |id: &str| RemoteFile {
        id: id.into(),
        path: format!("{id}.mkv"),
        size: 1,
    };
    assert_eq!(auto_choice(&[]), None);
    assert_eq!(auto_choice(&[file("a")]).as_deref(), Some("a"));
    assert_eq!(auto_choice(&[file("a"), file("b")]), None);
}
#[test]
fn file_paths_split_into_name_and_folder() {
    assert_eq!(split_path("movie.mkv"), ("movie.mkv", None));
    assert_eq!(
        split_path("Extras/Interview.mkv"),
        ("Interview.mkv", Some("Extras"))
    );
    assert_eq!(split_path("a\\b\\c.mkv"), ("c.mkv", Some("a\\b")));
    assert_eq!(split_path("dir/"), ("dir/", None));
}
