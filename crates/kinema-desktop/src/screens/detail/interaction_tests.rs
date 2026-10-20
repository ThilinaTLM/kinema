//! Offline production-screen interaction coverage.

use super::{BrowseTitle, Detail, Page};
use crate::{screens::coordinator::AppCoordinator, tmdb::TmdbTitle, widgets::tokens};
use ::core::prelude::v1::test;
use gpui::{prelude::*, *};
use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, ImdbId},
    sources::PlaybackTarget,
};
fn title(tmdb: bool) -> BrowseTitle {
    if tmdb {
        BrowseTitle::Tmdb(TmdbTitle {
            id: 91,
            kind: MediaKind::Series,
            name: "Series".into(),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: ImdbId::new("tt1234567").ok(),
        })
    } else {
        BrowseTitle::Cinemeta(
            CatalogTitle::new(
                ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id:{e}")),
                MediaKind::Series,
                "Series",
                None,
            )
            .unwrap_or_else(|e| panic!("title:{e}")),
        )
    }
}
fn tmdb_movie() -> BrowseTitle {
    BrowseTitle::Tmdb(TmdbTitle {
        id: 7,
        kind: MediaKind::Movie,
        name: "Movie".into(),
        overview: None,
        release_info: None,
        poster_url: None,
        display: Default::default(),
        imdb: ImdbId::new("tt1234567").ok(),
    })
}
fn draw(window: WindowHandle<Detail>, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
    })
    .unwrap_or_else(|e| panic!("draw:{e}"));
}
#[gpui::test]
fn episode_click_opens_streams_for_that_episode_and_back_restores_the_detail(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        tokens::init(cx);
        tokens::set_reduce_motion(true, cx);
    });
    for tmdb in [false, true] {
        let title = title(tmdb);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
        let window =
            cx.add_window(|_, cx| Detail::fixture(coordinator.clone(), title, false, None, cx));
        draw(window, cx);
        let season = if tmdb { 2 } else { 1 };
        window
            .update(cx, |view, _, cx| {
                assert_eq!(view.page(), Page::Overview);
                assert_eq!(view.season, Some(season));
                assert_eq!(view.episodes(cx).len(), 8);
                let episodes = view.episodes(cx);
                view.open_episode(episodes[1].clone(), cx);
                assert_eq!(view.page(), Page::Streams);
                assert!(matches!(
                    view.target,
                    Some(PlaybackTarget::Episode { season: s, episode: 2, .. }) if s == season
                ));
                assert!(view.coordinator.read(cx).sources.error().is_none());
            })
            .unwrap_or_else(|e| panic!("window:{e}"));
        draw(window, cx);
        window
            .update(cx, |view, _, cx| {
                // Back pops the page and keeps season and episode focus memory.
                assert!(view.go_back(cx));
                assert_eq!(view.page(), Page::Overview);
                assert!(view.episode.is_none() && view.target.is_none());
                assert_eq!(view.season, Some(season));
                assert_eq!(view.episode_return_focus, Some((season, 2)));
                assert!(!view.go_back(cx), "Overview is the root of the route");
                view.select_season(season + 1, cx);
                assert_eq!(view.episodes(cx).len(), 3);
                assert!(view.episodes(cx)[0].still_url.is_some());
            })
            .unwrap_or_else(|e| panic!("window:{e}"));
        draw(window, cx);
    }
}
#[gpui::test]
fn play_opens_the_streams_page_and_enter_prepares_a_stream(cx: &mut TestAppContext) {
    cx.update(|cx| {
        tokens::init(cx);
        tokens::set_reduce_motion(true, cx);
        crate::widgets::menu::bind_keys(cx);
    });
    let title = BrowseTitle::Cinemeta(
        CatalogTitle::new(
            ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id:{e}")),
            MediaKind::Movie,
            "Movie",
            None,
        )
        .unwrap_or_else(|e| panic!("title:{e}")),
    );
    let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
    let window = cx.add_window(|_, cx| Detail::fixture(coordinator, title, false, None, cx));
    cx.simulate_window_resize(window.into(), size(px(1280.), px(800.)));
    draw(window, cx);
    window
        .update(cx, |view, _, cx| {
            assert_eq!(view.page(), Page::Overview);
            assert!(view.coordinator.read(cx).sources.value().is_none());
            view.run_primary(cx);
            assert_eq!(view.page(), Page::Streams);
        })
        .unwrap_or_else(|e| panic!("play:{e}"));
    draw(window, cx);
    window
        .update(cx, |view, window, cx| {
            assert!(view.coordinator.read(cx).sources.value().is_some());
            view.stream_focus[0].focus(window, cx)
        })
        .unwrap_or_else(|e| panic!("focus:{e}"));
    cx.simulate_keystrokes(window.into(), "shift-f10");
    cx.run_until_parked();
    cx.simulate_keystrokes(window.into(), "down");
    cx.simulate_keystrokes(window.into(), "enter");
    cx.run_until_parked();
    window
        .update(cx, |view, window, cx| {
            assert_eq!(view.inline, Some(0));
            assert!(view.play.is_none());
            view.stream_focus[0].focus(window, cx);
        })
        .unwrap_or_else(|e| panic!("details:{e}"));
    cx.simulate_keystrokes(window.into(), "enter");
    cx.run_until_parked();
    window
        .update(cx, |view, _, cx| {
            assert!(view.play.is_some());
            assert_eq!(view.selected, Some(0));
            // Back leaves the page and drops the in-flight sheet with it.
            assert!(view.go_back(cx));
            assert!(view.play.is_none());
            assert_eq!(view.page(), Page::Overview);
        })
        .unwrap_or_else(|e| panic!("read:{e}"));
}
#[gpui::test]
fn similar_grid_shows_two_rows_and_show_more_pages_in_titles(cx: &mut TestAppContext) {
    cx.update(|cx| {
        tokens::init(cx);
        tokens::set_reduce_motion(true, cx);
    });
    let title = tmdb_movie();
    let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
    let window = cx.add_window(|_, cx| Detail::fixture(coordinator, title, false, None, cx));
    // Narrow enough that two rows (3 columns) hide some of the first page.
    cx.simulate_window_resize(window.into(), size(px(520.), px(1400.)));
    draw(window, cx);
    window
        .update(cx, |view, _, _| {
            let cols = view.similar_cols;
            assert_eq!(cols, 3, "140-200 px posters in a 496 px frame");
            assert_eq!(view.similar.loaded(), 20);
            assert_eq!(view.similar.visible(cols).len(), 2 * cols, "two rows");
        })
        .unwrap_or_else(|e| panic!("window:{e}"));
    window
        .update(cx, |view, _, cx| {
            // Rows 4 and 6 still fit the 20 loaded titles; the third click
            // (8 rows = 24 slots) outgrows them and loads page 2.
            view.show_more_similar(cx);
            view.show_more_similar(cx);
            assert_eq!(view.similar.loaded(), 20);
            view.show_more_similar(cx);
        })
        .unwrap_or_else(|e| panic!("window:{e}"));
    draw(window, cx);
    window
        .read_with(cx, |view, _| {
            assert_eq!(view.similar.loaded(), 40, "page 2 was merged");
            assert_eq!(view.similar.visible(3).len(), 24);
        })
        .unwrap_or_else(|e| panic!("window:{e}"));
}
#[gpui::test]
fn facts_block_shows_ratings_votes_and_provider_facts(cx: &mut TestAppContext) {
    cx.update(|cx| {
        tokens::init(cx);
        tokens::set_reduce_motion(true, cx);
    });
    for title in [tmdb_movie(), title(true)] {
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
        let window = cx.add_window(|_, cx| Detail::fixture(coordinator, title, false, None, cx));
        cx.simulate_window_resize(window.into(), size(px(1280.), px(800.)));
        draw(window, cx);
        window
            .read_with(cx, |view, cx| {
                let facts = view.facts_view(&view.display_title(cx), cx);
                let ratings: Vec<_> = facts
                    .ratings
                    .iter()
                    .map(|r| (r.label, r.value.as_str(), r.detail.as_deref()))
                    .collect();
                assert_eq!(
                    ratings,
                    [("IMDb", "8.1", None), ("TMDB", "7.8", Some("12,345 votes"))]
                );
                assert_eq!(
                    facts.tagline.as_deref(),
                    Some("The signal was never meant to be answered.")
                );
                let labels: Vec<_> = facts.rows.iter().map(|r| r.0).collect();
                for want in [
                    "Status",
                    "Certification",
                    "Language",
                    "Writers",
                    "Studios",
                    "Countries",
                ] {
                    assert!(labels.contains(&want), "{want} in {labels:?}");
                }
                assert_eq!(facts.cast.len(), 10);
                assert_eq!(facts.cast[0].role.as_deref(), Some("Role 1"));
            })
            .unwrap_or_else(|e| panic!("facts:{e}"));
    }
}
#[gpui::test]
fn column_counts_follow_the_window_width_on_every_resize(cx: &mut TestAppContext) {
    cx.update(|cx| {
        tokens::init(cx);
        tokens::set_reduce_motion(true, cx);
    });
    let title = tmdb_movie();
    let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
    let window = cx.add_window(|_, cx| Detail::fixture(coordinator, title, false, None, cx));
    // The count is derived from the viewport inside the same render pass, so a
    // single draw after each resize already shows the new count (no stale frame).
    for (width, cols) in [(520., 3), (1280., 6), (1920., 9), (2560., 9), (960., 5)] {
        cx.simulate_window_resize(window.into(), size(px(width), px(900.)));
        draw(window, cx);
        window
            .read_with(cx, |view, _| assert_eq!(view.similar_cols, cols, "{width}"))
            .unwrap_or_else(|e| panic!("window:{e}"));
    }
}
