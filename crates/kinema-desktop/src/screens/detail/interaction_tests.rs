//! Offline production-screen interaction coverage.

use super::{BrowseTitle, Detail};
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
#[gpui::test]
fn cinemeta_and_tmdb_decode_real_shapes_and_switch_seasons(cx: &mut TestAppContext) {
    cx.update(tokens::init);
    for tmdb in [false, true] {
        let title = title(tmdb);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
        let window =
            cx.add_window(|_, cx| Detail::fixture(coordinator.clone(), title, false, None, cx));
        cx.run_until_parked();
        window.update(cx,|view,_,cx|{
                assert_eq!(view.season,Some(if tmdb{2}else{1}));assert_eq!(view.episodes(cx).len(),8);
                let episodes=view.episodes(cx);view.choose_episode(episodes[1].clone(),cx);
                assert!(matches!(view.target,Some(PlaybackTarget::Episode{season,episode:2,..}) if season==if tmdb{2}else{1}));
                assert!(view.coordinator.read(cx).sources.error().is_none());
                view.back_to_episodes(cx);assert!(view.episode.is_none());
                view.select_season(if tmdb{3}else{2},cx);assert_eq!(view.episodes(cx).len(),3);
                assert!(view.episodes(cx)[0].still_url.is_some());
            }).unwrap_or_else(|e|panic!("window:{e}"));
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .unwrap_or_else(|e| panic!("draw:{e}"));
    }
}
#[gpui::test]
fn stream_keyboard_context_details_and_enter_use_same_row(cx: &mut TestAppContext) {
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
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
    })
    .unwrap_or_else(|e| panic!("draw:{e}"));
    window
        .update(cx, |view, window, cx| {
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
        .read_with(cx, |view, _| {
            assert!(view.play.is_some());
            assert_eq!(view.selected, Some(0));
        })
        .unwrap_or_else(|e| panic!("read:{e}"));
}
