//! Detail's episode continuity: play or resume from the stream memory, the
//! ranked stream list, and **Choose stream**. Offline: the coordinator is the
//! fixture one, and its answers are emitted as the events it would send.
use super::*;
use crate::controllers::coordinator::{DirectPlay, PickerSpec, StreamPicker};
use ::core::prelude::v1::test;
use kinema_core::{
    catalog::{CatalogTitle, ImdbId},
    episode_continuity::ReferenceRelease,
    sources::InfoHash,
};

fn series_title() -> BrowseTitle {
    BrowseTitle::Cinemeta(
        CatalogTitle::new(
            ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id: {e}")),
            MediaKind::Series,
            "Series",
            None,
        )
        .unwrap_or_else(|e| panic!("title: {e}")),
    )
}

fn draw(window: WindowHandle<Detail>, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
    })
    .unwrap_or_else(|e| panic!("draw: {e}"));
}

fn open() -> (TestAppContext, WindowHandle<Detail>, Entity<AppCoordinator>) {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        theme::init(cx);
        theme::set_reduce_motion(true, cx);
    });
    let title = series_title();
    let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
    let window =
        cx.add_window(|_, cx| Detail::fixture(coordinator.clone(), title, false, None, cx));
    draw(window, &mut cx);
    (cx, window, coordinator)
}

fn spec() -> PickerSpec {
    PickerSpec {
        reference: ReferenceRelease {
            // The fixture streams share a group; the BluRay x265 one is closest.
            name: "Quiet.Orbit.2024.1080p.BluRay.x265.REMUX.DTS-HD.MA.5.1-GROUP".into(),
            file_name: None,
            size: Some(4_200_000_000),
        },
        exclude: None,
        label: "No close match — pick a stream".into(),
    }
}

fn ticket(window: WindowHandle<Detail>, cx: &mut TestAppContext) -> u64 {
    window
        .update(cx, |view, _, _| view.direct.as_ref().map(|d| d.ticket))
        .unwrap_or_else(|e| panic!("window: {e}"))
        .unwrap_or_else(|| panic!("a direct request"))
}

fn answer(
    coordinator: &Entity<AppCoordinator>,
    cx: &mut TestAppContext,
    ticket: u64,
    play: DirectPlay,
) {
    coordinator.update(cx, |_, cx| {
        cx.emit(CoordinatorEvent::DirectPlay { ticket, play })
    });
    cx.run_until_parked();
}

#[test]
fn play_waits_for_the_memory_without_opening_the_stream_list() {
    let (mut cx, window, _) = open();
    window
        .update(&mut cx, |view, _, cx| {
            view.play_episode(1, 2, None, cx);
            assert!(view.direct.is_some());
            assert_eq!(view.page(), Page::Overview);
        })
        .unwrap_or_else(|e| panic!("play: {e}"));
    draw(window, &mut cx);
}

#[test]
fn nothing_remembered_falls_back_to_the_ordinary_stream_list() {
    let (mut cx, window, coordinator) = open();
    window
        .update(&mut cx, |view, _, cx| view.play_episode(1, 2, None, cx))
        .unwrap_or_else(|e| panic!("play: {e}"));
    let ticket = ticket(window, &mut cx);
    answer(&coordinator, &mut cx, ticket, DirectPlay::Fallback);
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.page(), Page::Streams);
            assert!(view.ranking.is_none() && view.direct.is_none());
            assert!(matches!(
                view.target,
                Some(PlaybackTarget::Episode {
                    season: 1,
                    episode: 2,
                    ..
                })
            ));
        })
        .unwrap_or_else(|e| panic!("page: {e}"));
}

#[test]
fn an_unavailable_indexer_also_ends_on_the_list_which_shows_its_own_state() {
    let (mut cx, window, coordinator) = open();
    window
        .update(&mut cx, |view, _, cx| view.play_episode(1, 2, None, cx))
        .unwrap_or_else(|e| panic!("play: {e}"));
    let ticket = ticket(window, &mut cx);
    answer(
        &coordinator,
        &mut cx,
        ticket,
        DirectPlay::Unavailable("Couldn't find a stream for this episode".into()),
    );
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.page(), Page::Streams);
            assert!(view.ranking.is_none());
        })
        .unwrap_or_else(|e| panic!("page: {e}"));
}

#[test]
fn a_stale_answer_for_an_earlier_request_is_ignored() {
    let (mut cx, window, coordinator) = open();
    window
        .update(&mut cx, |view, _, cx| view.play_episode(1, 2, None, cx))
        .unwrap_or_else(|e| panic!("play: {e}"));
    let first = ticket(window, &mut cx);
    window
        .update(&mut cx, |view, _, cx| view.play_episode(1, 3, None, cx))
        .unwrap_or_else(|e| panic!("play: {e}"));
    answer(&coordinator, &mut cx, first, DirectPlay::Fallback);
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(
                view.page(),
                Page::Overview,
                "the first request was replaced"
            );
            assert!(view.direct.is_some());
        })
        .unwrap_or_else(|e| panic!("page: {e}"));
}

#[test]
fn a_picker_answer_orders_the_list_best_match_first_and_marks_the_top() {
    let (mut cx, window, coordinator) = open();
    window
        .update(&mut cx, |view, _, cx| view.play_episode(1, 2, None, cx))
        .unwrap_or_else(|e| panic!("play: {e}"));
    let ticket = ticket(window, &mut cx);
    answer(&coordinator, &mut cx, ticket, DirectPlay::Picker(spec()));
    draw(window, &mut cx);
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.page(), Page::Streams);
            assert!(view.ranking.is_some());
            let sources = view
                .coordinator
                .read(cx)
                .sources
                .value()
                .cloned()
                .unwrap_or_else(|| panic!("the fixture sources"));
            let rows = view.stream_indices(&sources, cx);
            // "1080p.BluRay.x265" (index 2) shares the most with the reference.
            assert_eq!(rows.first(), Some(&2));
            let generation = view.coordinator.read(cx).sources.generation();
            let ranking = view.ranking.as_ref().unwrap_or_else(|| panic!("ranking"));
            assert_eq!(ranking.mark(generation, &sources, 2), Some("Best match"));
            // The first row is also the selected one: preselected.
            view.reconcile_stream_selection(&rows, cx);
            assert_eq!(view.selected, Some(2));
        })
        .unwrap_or_else(|e| panic!("ranked: {e}"));
}

#[test]
fn the_ranked_list_leaves_out_the_torrent_whose_file_is_unknown() {
    let (mut cx, window, coordinator) = open();
    window
        .update(&mut cx, |view, _, cx| view.play_episode(1, 2, None, cx))
        .unwrap_or_else(|e| panic!("play: {e}"));
    let ticket = ticket(window, &mut cx);
    let mut excluded = spec();
    excluded.exclude = Some(InfoHash::new("a".repeat(40)).unwrap_or_else(|e| panic!("hash: {e}")));
    answer(&coordinator, &mut cx, ticket, DirectPlay::Picker(excluded));
    draw(window, &mut cx);
    window
        .update(&mut cx, |view, _, cx| {
            let sources = view
                .coordinator
                .read(cx)
                .sources
                .value()
                .cloned()
                .unwrap_or_else(|| panic!("the fixture sources"));
            // Every fixture stream is that torrent: none is offered.
            assert!(view.stream_indices(&sources, cx).is_empty());
        })
        .unwrap_or_else(|e| panic!("ranked: {e}"));
}

#[test]
fn a_remembered_stream_starts_inline_and_a_non_unique_file_opens_the_ranked_list() {
    let (mut cx, window, coordinator) = open();
    window
        .update(&mut cx, |view, _, cx| view.play_episode(1, 2, None, cx))
        .unwrap_or_else(|e| panic!("play: {e}"));
    let ticket = ticket(window, &mut cx);
    let candidate = SourceCandidate {
        torrent: kinema_core::sources::TorrentSource {
            hash: InfoHash::new("b".repeat(40)).unwrap_or_else(|e| panic!("hash: {e}")),
            file_index: None,
            filename: None,
        },
        name: "Quiet.Orbit.2024.1080p.BluRay.x265-GROUP".into(),
        quality: Some("1080p".into()),
        size: Some(1),
        seeders: None,
    };
    answer(
        &coordinator,
        &mut cx,
        ticket,
        DirectPlay::Play {
            candidate,
            label: None,
            picker: None,
        },
    );
    let session = window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.page(), Page::Overview, "the list was skipped");
            match view.direct.as_ref().map(|d| &d.stage) {
                Some(continuity::DirectStage::Playing { session, .. }) => session.clone(),
                _ => panic!("a play session"),
            }
        })
        .unwrap_or_else(|e| panic!("session: {e}"));
    draw(window, &mut cx);
    // The torrent's file is not unique: never a guess, the ranked list opens.
    session.update(&mut cx, |_, cx| cx.emit(PlaySessionEvent::NeedsPicker));
    cx.run_until_parked();
    window
        .update(&mut cx, |view, _, _| {
            assert_eq!(view.page(), Page::Streams);
            assert!(view.ranking.is_some() && view.direct.is_none());
        })
        .unwrap_or_else(|e| panic!("page: {e}"));
}

#[test]
fn choose_stream_is_always_the_way_back_to_the_plain_list() {
    let (mut cx, window, _) = open();
    window
        .update(&mut cx, |view, _, cx| {
            view.play_episode(1, 2, None, cx);
            view.choose_stream(1, 2, cx);
            assert!(view.direct.is_none(), "the direct request is dropped");
            assert_eq!(view.page(), Page::Streams);
            assert!(view.ranking.is_none());
            assert!(matches!(
                view.target,
                Some(PlaybackTarget::Episode {
                    season: 1,
                    episode: 2,
                    ..
                })
            ));
        })
        .unwrap_or_else(|e| panic!("choose: {e}"));
}

#[test]
fn the_picker_for_a_player_window_lists_its_episode_and_frees_the_control_when_left() {
    let (mut cx, window, coordinator) = open();
    let target = PlaybackTarget::Episode {
        series: ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id: {e}")),
        season: 1,
        episode: 3,
    };
    coordinator.update(&mut cx, |c, _| {
        c.fixture_stream_picker(StreamPicker {
            window: "window-1".into(),
            direction: crate::ui::views::player::NeighbourDirection::Next,
            target,
            spec: spec(),
            title: "Series".into(),
            episode_title: Some("Arrival 3".into()),
        })
    });
    window
        .update(&mut cx, |view, _, cx| {
            view.open_window_picker(cx);
            assert_eq!(view.page(), Page::Streams);
            assert!(view.ranking.as_ref().is_some_and(|r| r.window_picker));
            assert!(matches!(
                view.target,
                Some(PlaybackTarget::Episode { episode: 3, .. })
            ));
        })
        .unwrap_or_else(|e| panic!("open: {e}"));
    assert!(coordinator.read_with(&cx, |c, _| c.stream_picker().is_some()));
    // Back / Esc: the picker is dismissed and the player's control is released.
    window
        .update(&mut cx, |view, _, cx| assert!(view.go_back(cx)))
        .unwrap_or_else(|e| panic!("back: {e}"));
    assert!(coordinator.read_with(&cx, |c, _| c.stream_picker().is_none()));
}

#[test]
fn closing_the_title_drops_a_waiting_picker_and_a_pending_request() {
    let (mut cx, window, coordinator) = open();
    window
        .update(&mut cx, |view, _, cx| view.play_episode(1, 2, None, cx))
        .unwrap_or_else(|e| panic!("play: {e}"));
    let target = PlaybackTarget::Episode {
        series: ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id: {e}")),
        season: 1,
        episode: 3,
    };
    coordinator.update(&mut cx, |c, _| {
        c.fixture_stream_picker(StreamPicker {
            window: "window-1".into(),
            direction: crate::ui::views::player::NeighbourDirection::Next,
            target,
            spec: spec(),
            title: "Series".into(),
            episode_title: None,
        })
    });
    window
        .update(&mut cx, |view, _, cx| {
            view.ranking = Some(continuity::Ranking::new(spec(), true));
            view.release_continuity(cx);
        })
        .unwrap_or_else(|e| panic!("release: {e}"));
    assert!(coordinator.read_with(&cx, |c, _| c.stream_picker().is_none()));
}

#[test]
fn metadata_arriving_does_not_close_a_ranked_list_for_another_season() {
    let (mut cx, window, coordinator) = open();
    let target = PlaybackTarget::Episode {
        series: ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id: {e}")),
        season: 2,
        episode: 1,
    };
    coordinator.update(&mut cx, |c, _| {
        c.fixture_stream_picker(StreamPicker {
            window: "window-1".into(),
            direction: crate::ui::views::player::NeighbourDirection::Next,
            target,
            spec: spec(),
            title: "Series".into(),
            episode_title: None,
        })
    });
    window
        .update(&mut cx, |view, _, cx| {
            view.open_window_picker(cx);
            // Detail's own season choice (season 1) must not clear the list.
            view.season = None;
            view.reconcile_metadata(cx);
            assert_eq!(view.page(), Page::Streams);
            assert!(matches!(
                view.target,
                Some(PlaybackTarget::Episode {
                    season: 2,
                    episode: 1,
                    ..
                })
            ));
            assert!(view.ranking.is_some());
        })
        .unwrap_or_else(|e| panic!("picker: {e}"));
}
