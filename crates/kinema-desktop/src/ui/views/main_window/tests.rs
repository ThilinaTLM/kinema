use super::*;
use crate::app::{AppEvent, AppOutcome};
use ::core::prelude::v1::test;
use std::time::Duration;

#[gpui::test]
fn tray_transfers_opens_existing_panel_without_toggling_it_closed(cx: &mut TestAppContext) {
    cx.update(theme::init);
    let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
    let main =
        cx.add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
    cx.run_until_parked();
    let open = |cx: &mut TestAppContext| {
        main.read_with(cx, |view, cx| {
            view.transfers_popover
                .borrow()
                .as_ref()
                .is_some_and(|handle| handle.is_open(cx))
        })
        .unwrap()
    };
    assert!(!open(cx));
    cx.update(crate::controllers::lifecycle::show_transfers);
    cx.run_until_parked();
    assert!(open(cx));
    cx.update(crate::controllers::lifecycle::show_transfers);
    cx.run_until_parked();
    assert!(open(cx));
}

fn movie(id: u64, name: &str) -> crate::tmdb::TmdbTitle {
    crate::tmdb::TmdbTitle {
        id,
        kind: MediaKind::Movie,
        name: name.into(),
        overview: None,
        release_info: None,
        poster_url: None,
        display: Default::default(),
        imdb: None,
    }
}

fn mounted_detail(main: WindowHandle<MainWindow>, cx: &mut TestAppContext) -> Entity<Detail> {
    main.read_with(cx, |view, _| match &view.route {
        Route::Detail(detail) => detail.clone(),
        Route::Browse => panic!("expected Detail route"),
    })
    .unwrap()
}

#[gpui::test]
fn similar_pointer_and_keyboard_replace_detail_and_back_restores_browse(cx: &mut TestAppContext) {
    cx.update(|cx| {
        theme::init(cx);
        theme::set_reduce_motion(true, cx);
        bind_keys(cx);
    });
    let first = movie(1, "Movie one");
    let second = movie(2, "Movie two");
    let third = movie(3, "Movie three");
    let coordinator = cx.new(|cx| {
        let mut c = AppCoordinator::fixture(vec![BrowseTitle::Tmdb(first.clone())], None, cx);
        for (id, titles) in [(1, vec![second.clone()]), (2, vec![third]), (3, vec![])] {
            c.fixture_similar.insert(
                (id, 1),
                crate::tmdb::TmdbPage {
                    titles,
                    page: 1,
                    total_pages: 1,
                },
            );
        }
        c.load(MediaKind::Movie, "Movie".into(), false, cx);
        c
    });
    let main =
        cx.add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
    let browse = main
        .update(cx, |view, _, cx| {
            view.browse.update(cx, |b, cx| {
                b.sort = Sort::Year;
                cx.notify();
            });
            view.browse.clone()
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(main.into(), cx);
    // A tall fixed viewport exposes the production recommendation row below
    // the hero, without scrolling or relying on a test-only rendering path.
    visual.simulate_resize(size(px(1280.), px(1400.)));
    cx.run_until_parked();
    cx.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    visual.simulate_click(point(px(100.), px(150.)), Modifiers::default());
    cx.run_until_parked();
    let initial = mounted_detail(main, cx);
    assert_eq!(
        initial.read_with(cx, |d, _| d.name().to_owned()),
        first.name
    );
    let retired = initial.downgrade();
    drop(initial);
    let obsolete = coordinator.update(cx, |c, cx| {
        // Simulate outstanding title/season/source/recommendation requests.
        // Keep the recommendation value visible while its refresh is pending.
        let generations = [
            c.details.begin(false),
            c.tmdb_details.begin(false),
            c.tmdb_season.begin(false),
            c.sources.begin(false),
            c.similar.begin(false),
        ];
        cx.notify();
        generations
    });
    cx.run_until_parked();
    cx.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    // Locate the Similar poster from its laid-out bounds, so the test does
    // not depend on the hero height or carousel placement. Detail must tag
    // each tile with `.debug_selector(|| format!("similar-title-{id}"))`.
    let bounds = visual.debug_bounds("similar-title-2").unwrap_or_else(|| {
        panic!("Detail's Similar tile needs debug_selector `similar-title-<id>`")
    });
    visual.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
    let current = mounted_detail(main, cx);
    assert_eq!(
        current.read_with(cx, |d, _| d.name().to_owned()),
        second.name
    );
    assert!(retired.upgrade().is_none());
    coordinator.update(cx, |c, _| {
        assert!(!c.details.failed(obsolete[0], "late title"));
        assert!(!c.tmdb_details.failed(obsolete[1], "late TMDB title"));
        assert!(!c.tmdb_season.failed(obsolete[2], "late season"));
        assert!(!c.sources.failed(obsolete[3], "late sources"));
        assert!(!c.similar.failed(obsolete[4], "late recommendations"));
    });
    let retired = current.downgrade();
    drop(current);
    // Keyboard path without assuming the tile is the last tab stop (Detail
    // may keep a Streams Retry stop after it). Mouse-down alone focuses the
    // tile without activating it; shift-tab then tab must come back to the
    // same stop, and only then does Enter open the recommendation.
    cx.run_until_parked();
    cx.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let tile = visual.debug_bounds("similar-title-3").unwrap_or_else(|| {
        panic!("Detail's Similar tile needs debug_selector `similar-title-<id>`")
    });
    visual.simulate_mouse_down(tile.center(), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        mounted_detail(main, cx).read_with(cx, |d, _| d.name().to_owned()),
        "Movie two",
        "mouse-down must focus, not open"
    );
    cx.simulate_keystrokes(main.into(), "shift-tab tab enter");
    cx.run_until_parked();
    let current = mounted_detail(main, cx);
    assert_eq!(
        current.read_with(cx, |d, _| d.name().to_owned()),
        "Movie three"
    );
    assert!(retired.upgrade().is_none());
    drop(current);
    cx.simulate_keystrokes(main.into(), "alt-left");
    cx.run_until_parked();
    main.update(cx, |view, _, cx| {
        assert!(matches!(view.route, Route::Browse));
        assert_eq!(view.browse, browse);
        assert_eq!(view.browse.read(cx).sort, Sort::Year);
        assert_eq!(view.coordinator.read(cx).query, "Movie");
        assert_eq!(view.coordinator.read(cx).kind, MediaKind::Movie);
        assert!(view.detail_subscription.is_none());
    })
    .unwrap();
    // The original Browse poster, not the last Similar result, regains focus.
    cx.simulate_keystrokes(main.into(), "enter");
    assert_eq!(
        mounted_detail(main, cx).read_with(cx, |d, _| d.name().to_owned()),
        "Movie one"
    );
}

#[gpui::test]
fn successful_initial_launch_and_relaunch_pulse_but_failure_does_not(cx: &mut TestAppContext) {
    cx.update(theme::init);
    let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
    let main =
        cx.add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
    for (outcome, expected) in [
        (
            AppOutcome::PlaybackStarted {
                session: crate::media::PlaybackSession {
                    id: "offline-session".into(),
                    url: "http://127.0.0.1:1234/offline".into(),
                },
                file_name: "fixture.mkv".into(),
            },
            1,
        ),
        (
            AppOutcome::SessionRelaunched {
                session_id: "offline-session".into(),
            },
            2,
        ),
        (
            AppOutcome::Failed {
                message: "Launch failed".into(),
            },
            2,
        ),
    ] {
        coordinator.update(cx, |_, cx| {
            cx.emit(CoordinatorEvent::App(AppEvent {
                generation: 1,
                outcome,
            }));
        });
        cx.run_until_parked();
        main.update(cx, |view, _, _| assert_eq!(view.launch_pulse, expected))
            .unwrap();
    }
}

fn draw(main: WindowHandle<MainWindow>, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
}

fn fixture_window(cx: &mut TestAppContext) -> (Entity<AppCoordinator>, WindowHandle<MainWindow>) {
    cx.update(|cx| {
        theme::init(cx);
        theme::set_reduce_motion(true, cx);
        bind_keys(cx);
    });
    let coordinator = cx.new(|cx| {
        let mut c = AppCoordinator::fixture(
            vec![BrowseTitle::Tmdb(movie(1, "Provider movie"))],
            None,
            cx,
        );
        c.load(MediaKind::Movie, "Provider".into(), false, cx);
        c
    });
    let main =
        cx.add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
    draw(main, cx);
    (coordinator, main)
}

/// Replace the Library result list as a completed local read would.
fn set_library(coordinator: &Entity<AppCoordinator>, ids: &[u64], cx: &mut TestAppContext) {
    coordinator.update(cx, |c, cx| {
        let generation = c.library.begin(true);
        c.library_titles = ids
            .iter()
            .map(|id| BrowseTitle::Tmdb(movie(*id, &format!("Saved {id}"))))
            .collect();
        c.library_has_more = false;
        c.library.fetched(generation, c.library_titles.clone(), 0);
        cx.notify();
    });
}

fn type_in_search(main: WindowHandle<MainWindow>, text: &str, cx: &mut TestAppContext) {
    main.update(cx, |view, _, cx| {
        view.search.update(cx, |s, cx| s.set_text(text, cx))
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn library_scope_reads_local_results_and_restores_each_scopes_context(cx: &mut TestAppContext) {
    let (coordinator, main) = fixture_window(cx);
    let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
    assert_eq!(browse.read_with(cx, |b, _| b.card_keys()), ["Movie:tmdb:1"]);
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Library, window, cx)
    })
    .unwrap();
    set_library(&coordinator, &[7, 8], cx);
    draw(main, cx);
    // Library shows its own rows, never the provider grid filtered.
    assert_eq!(
        browse.read_with(cx, |b, _| b.card_keys()),
        ["Movie:tmdb:7", "Movie:tmdb:8"]
    );
    main.read_with(cx, |v, cx| {
        assert_eq!(v.search.read(cx).text(cx), "");
    })
    .unwrap();
    type_in_search(main, "saved", cx);
    cx.background_executor
        .advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    browse.read_with(cx, |b, _| assert_eq!(b.library_search(), "saved"));
    coordinator.read_with(cx, |c, _| assert_eq!(c.query, "Provider"));
    // Back in Discover the provider query is restored, not Library text.
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Discover, window, cx)
    })
    .unwrap();
    draw(main, cx);
    main.read_with(cx, |v, cx| {
        assert_eq!(v.search.read(cx).text(cx), "Provider")
    })
    .unwrap();
    assert_eq!(browse.read_with(cx, |b, _| b.card_keys()), ["Movie:tmdb:1"]);
}

#[gpui::test]
fn stale_discover_search_cannot_leak_into_library_or_provider(cx: &mut TestAppContext) {
    let (coordinator, main) = fixture_window(cx);
    let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Library, window, cx)
    })
    .unwrap();
    type_in_search(main, "dune", cx);
    cx.background_executor
        .advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Discover, window, cx)
    })
    .unwrap();
    // A Discover edit is still inside its debounce window when the user
    // flips back to Library.
    type_in_search(main, "Other", cx);
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Library, window, cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    main.read_with(cx, |v, cx| assert_eq!(v.search.read(cx).text(cx), "dune"))
        .unwrap();
    browse.read_with(cx, |b, _| assert_eq!(b.library_search(), "dune"));
    coordinator.read_with(cx, |c, _| {
        assert_eq!(c.query, "Provider", "no provider fetch")
    });
    // Rapid back-and-forth settles on the provider's own query.
    for scope in [Scope::Discover, Scope::Library, Scope::Discover] {
        main.update(cx, |view, window, cx| view.switch_scope(scope, window, cx))
            .unwrap();
    }
    cx.background_executor
        .advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    main.read_with(cx, |v, cx| {
        assert_eq!(v.search.read(cx).text(cx), "Provider")
    })
    .unwrap();
    browse.read_with(cx, |b, _| assert_eq!(b.library_search(), "dune"));
    coordinator.read_with(cx, |c, _| assert_eq!(c.query, "Provider"));
}

#[gpui::test]
fn membership_toggle_reports_failure_without_opening_detail(cx: &mut TestAppContext) {
    use crate::ui::blocks::poster_card::MembershipRequested;
    let (coordinator, main) = fixture_window(cx);
    let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
    let card = browse.read_with(cx, |b, _| b.card_for_test(0));
    // The fixture has no catalog worker, so the write is rejected up front:
    // the failure is surfaced and nothing navigates.
    card.update(cx, |_, cx| cx.emit(MembershipRequested(true)));
    draw(main, cx);
    coordinator.read_with(cx, |c, _| assert!(c.personal_notice.is_some()));
    main.read_with(cx, |v, _| assert!(matches!(v.route, Route::Browse)))
        .unwrap();
    // An unacknowledged state is never shown as unsaved: still loading/failed.
    assert!(!matches!(
        card.read_with(cx, |card, _| card.membership),
        crate::ui::blocks::state_toggle::ToggleStatus::Known { on: false, .. }
    ));
}

#[gpui::test]
fn filtered_out_focus_moves_to_next_then_previous_card(cx: &mut TestAppContext) {
    let (coordinator, main) = fixture_window(cx);
    let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Library, window, cx)
    })
    .unwrap();
    set_library(&coordinator, &[1, 2, 3], cx);
    draw(main, cx);
    let focused = |cx: &mut TestAppContext| {
        main.update(cx, |_, window, cx| browse.read(cx).focused_key(window, cx))
            .unwrap()
    };
    for (focus, remaining, expected) in [
        (1usize, vec![1u64, 3], "Movie:tmdb:3"),
        (2, vec![1, 2], "Movie:tmdb:2"),
    ] {
        set_library(&coordinator, &[1, 2, 3], cx);
        draw(main, cx);
        let card = browse.read_with(cx, |b, _| b.card_for_test(focus));
        main.update(cx, |_, window, cx| {
            card.update(cx, |c, cx| c.focus(window, cx))
        })
        .unwrap();
        draw(main, cx);
        assert!(focused(cx).is_some());
        set_library(&coordinator, &remaining, cx);
        draw(main, cx);
        assert_eq!(focused(cx).as_deref(), Some(expected));
    }
}

#[gpui::test]
fn one_filters_chip_per_scope_collapses_to_icon_at_960(cx: &mut TestAppContext) {
    let (_, main) = fixture_window(cx);
    let mut visual = VisualTestContext::from_window(main.into(), cx);
    for (scope, selector, other) in [
        (Scope::Library, "library-filters", "discover-filters"),
        (Scope::Discover, "discover-filters", "library-filters"),
    ] {
        main.update(cx, |view, window, cx| view.switch_scope(scope, window, cx))
            .unwrap();
        let mut widths = Vec::new();
        for width in [1280., 960.] {
            visual.simulate_resize(size(px(width), px(800.)));
            draw(main, cx);
            let bounds = visual
                .debug_bounds(selector)
                .expect("each scope shows exactly one Filters control");
            // Single 48px toolbar row, fully inside the window.
            assert!(
                bounds.top() >= Pixels::ZERO && bounds.bottom() <= size::TOOLBAR_HEIGHT,
                "{bounds:?}"
            );
            assert!(bounds.right() <= px(width), "{bounds:?}");
            widths.push(bounds.size.width);
            assert!(visual.debug_bounds(other).is_none());
        }
        assert!(widths[0] > widths[1] + px(20.), "{widths:?}");
    }
}

#[gpui::test]
fn scope_chip_click_toggles_discover_and_library(cx: &mut TestAppContext) {
    let (_, main) = fixture_window(cx);
    let mut visual = VisualTestContext::from_window(main.into(), cx);
    visual.simulate_resize(size(px(1280.), px(800.)));
    let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
    for expected in [Scope::Library, Scope::Discover, Scope::Library] {
        draw(main, cx);
        let chip = visual.debug_bounds("scope-toggle").expect("scope chip");
        visual.simulate_click(chip.center(), Modifiers::default());
        draw(main, cx);
        assert_eq!(browse.read_with(cx, |b, _| b.scope()), expected);
    }
}

#[gpui::test]
fn header_tools_and_global_group_fit_inside_the_row(cx: &mut TestAppContext) {
    let (_, main) = fixture_window(cx);
    let mut visual = VisualTestContext::from_window(main.into(), cx);
    for width in [1280., 960.] {
        visual.simulate_resize(size(px(width), px(800.)));
        draw(main, cx);
        let filters = visual.debug_bounds("discover-filters").unwrap();
        let global = visual.debug_bounds("toolbar-global").unwrap();
        // Transfers, divider, Settings and overflow: three icon targets at least.
        assert!(global.size.width >= size::ICON_TARGET * 3., "{global:?}");
        assert!(filters.right() <= global.left(), "{filters:?} {global:?}");
        assert!(global.right() <= px(width) - s::S20 + px(0.5), "{global:?}");
        assert!(global.bottom() <= size::TOOLBAR_HEIGHT, "{global:?}");
    }
}

#[gpui::test]
fn header_owns_clicks_but_lets_the_wheel_scroll_the_grid(cx: &mut TestAppContext) {
    cx.update(|cx| {
        theme::init(cx);
        theme::set_reduce_motion(true, cx);
        bind_keys(cx);
    });
    let titles = (1..=60)
        .map(|id| BrowseTitle::Tmdb(movie(id, &format!("Movie {id}"))))
        .collect();
    let coordinator = cx.new(|cx| {
        let mut c = AppCoordinator::fixture(titles, None, cx);
        c.load(MediaKind::Movie, String::new(), false, cx);
        c
    });
    let main =
        cx.add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
    let mut visual = VisualTestContext::from_window(main.into(), cx);
    visual.simulate_resize(size(px(1280.), px(800.)));
    draw(main, cx);
    let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
    // Wheel over the header still scrolls the page beneath it.
    let empty_header = point(px(380.), px(24.));
    visual.simulate_event(ScrollWheelEvent {
        position: empty_header,
        delta: ScrollDelta::Pixels(point(Pixels::ZERO, px(-160.))),
        ..Default::default()
    });
    draw(main, cx);
    assert!(browse.read_with(cx, |b, _| b.scrolled_under_header()) > Pixels::ZERO);
    // A card now sits under the header; clicking the header must not open it.
    visual.simulate_click(empty_header, Modifiers::default());
    draw(main, cx);
    main.read_with(cx, |v, _| assert!(matches!(v.route, Route::Browse)))
        .unwrap();
    // The same card below the header still opens.
    visual.simulate_click(point(px(100.), px(300.)), Modifiers::default());
    draw(main, cx);
    main.read_with(cx, |v, _| assert!(matches!(v.route, Route::Detail(_))))
        .unwrap();
}

#[gpui::test]
fn library_requests_the_next_local_page_only_when_more_exist(cx: &mut TestAppContext) {
    let (coordinator, main) = fixture_window(cx);
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Library, window, cx)
    })
    .unwrap();
    // Establish the active query (the fixture has no worker, so the read fails).
    coordinator.update(cx, |c, cx| {
        c.load_library(Default::default(), cx);
    });
    let ids: Vec<u64> = (1..=14).collect();
    set_library(&coordinator, &ids, cx);
    let seeded = coordinator.read_with(cx, |c, _| c.library.generation());
    draw(main, cx);
    coordinator.read_with(cx, |c, _| {
        assert_eq!(c.library.generation(), seeded, "no more pages: no request");
    });
    coordinator.update(cx, |c, cx| {
        c.library_has_more = true;
        cx.notify();
    });
    draw(main, cx);
    cx.run_until_parked();
    coordinator.read_with(cx, |c, _| {
        assert!(
            c.library.generation() > seeded,
            "a visible short list pages"
        );
        assert_eq!(c.library_titles.len(), 14, "loaded cards stay on failure");
    });
}

#[gpui::test]
fn only_confirmations_expire_and_failures_persist(cx: &mut TestAppContext) {
    let (coordinator, _main) = fixture_window(cx);
    for failure in [false, true] {
        coordinator.update(cx, |c, cx| {
            if failure {
                c.report_personal_failure("Could not update “Dune”: busy.".into(), cx);
            } else {
                c.personal_notice = Some("Updated “Dune”.".into());
                cx.notify();
            }
        });
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(Duration::from_secs(60));
        cx.run_until_parked();
        coordinator.read_with(cx, |c, _| {
            assert_eq!(c.personal_notice.is_none(), !failure);
        });
    }
    // Dismiss is the explicit way to clear a failure.
    coordinator.update(cx, |c, cx| c.dismiss_personal_notice(cx));
    coordinator.read_with(cx, |c, _| assert!(c.personal_notice.is_none()));
}

fn local_entry(id: i64, added_at: Option<i64>) -> crate::catalog::library::LibraryEntry {
    use kinema_core::{
        catalog_normalized::{CanonicalTitle, ExternalIdentifier, FieldValue},
        library::TitleId,
    };
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("name".to_owned(), FieldValue::Text(format!("Saved {id}")));
    crate::catalog::library::LibraryEntry {
        metadata: CanonicalTitle {
            id,
            kind: MediaKind::Movie,
            identifiers: vec![ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: format!("tt{id:07}"),
            }],
            fields,
            provenance: vec![],
            ratings: vec![],
            updated_at: 0,
        },
        personal: crate::catalog::library::PersonalTitle {
            title_id: TitleId::new(id).unwrap(),
            kind: MediaKind::Movie,
            added_at,
            movie_watch: None,
            playback_progress: None,
            has_resume: false,
            show_summary: None,
        },
    }
}

/// Apply committed results the way worker events arrive: first the mutation
/// acknowledgement, then the refreshed first Library page.
fn commit(
    coordinator: &Entity<AppCoordinator>,
    revision: u64,
    mutation: Option<(i64, bool, Option<i64>)>,
    page: Vec<crate::catalog::library::LibraryEntry>,
    cx: &mut TestAppContext,
) {
    use crate::{
        catalog::library::{LibraryOperation, LibraryPage, LibraryResult, TitleRef},
        catalog_worker::local::{LocalEvent, LocalRequest},
    };
    use kinema_core::library::{LibraryQuery, TitleId};
    coordinator.update(cx, |c, cx| {
        if let Some((id, saved, previous_added_at)) = mutation {
            let personal = local_entry(id, saved.then_some(20)).personal;
            c.fixture_apply_personal_event(
                LocalEvent {
                    request: LocalRequest {
                        request_id: revision * 10,
                        generation: 0,
                        operation: LibraryOperation::SetMembership {
                            title: TitleRef::Canonical(TitleId::new(id).unwrap()),
                            saved,
                            restore_added_at: None,
                        },
                    },
                    revision,
                    result: Ok(LibraryResult::Membership {
                        personal,
                        previous_added_at,
                    }),
                },
                cx,
            );
        }
        let generation = c.library.begin(false);
        let total = page.len();
        c.fixture_apply_personal_event(
            LocalEvent {
                request: LocalRequest {
                    request_id: revision * 10 + 1,
                    generation,
                    operation: LibraryOperation::List(LibraryQuery {
                        limit: 60,
                        ..Default::default()
                    }),
                },
                revision,
                result: Ok(LibraryResult::Page(LibraryPage {
                    entries: page,
                    total,
                })),
            },
            cx,
        );
    });
}

#[gpui::test]
fn committed_remove_then_undo_restores_card_order_and_keeps_focus_by_title(
    cx: &mut TestAppContext,
) {
    use crate::ui::blocks::state_toggle::ToggleStatus;
    let (coordinator, main) = fixture_window(cx);
    let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Library, window, cx)
    })
    .unwrap();
    let all = || {
        vec![
            local_entry(1, Some(30)),
            local_entry(2, Some(20)),
            local_entry(3, Some(10)),
        ]
    };
    commit(&coordinator, 1, None, all(), cx);
    draw(main, cx);
    let keys = |cx: &mut TestAppContext| browse.read_with(cx, |b, _| b.card_keys());
    assert_eq!(keys(cx), ["canonical:1", "canonical:2", "canonical:3"]);
    let focused = |cx: &mut TestAppContext| {
        main.update(cx, |_, window, cx| browse.read(cx).focused_key(window, cx))
            .unwrap()
    };
    // Focus the middle card, then commit its removal and the refreshed page.
    let card = browse.read_with(cx, |b, _| b.card_for_test(1));
    main.update(cx, |_, window, cx| {
        card.update(cx, |c, cx| c.focus(window, cx))
    })
    .unwrap();
    draw(main, cx);
    assert_eq!(focused(cx).as_deref(), Some("canonical:2"));
    commit(
        &coordinator,
        2,
        Some((2, false, Some(20))),
        vec![local_entry(1, Some(30)), local_entry(3, Some(10))],
        cx,
    );
    draw(main, cx);
    assert_eq!(keys(cx), ["canonical:1", "canonical:3"]);
    assert_eq!(focused(cx).as_deref(), Some("canonical:3"), "next card");
    coordinator.read_with(cx, |c, _| assert!(c.personal_notice.is_some()));
    // Undo: the committed re-add and refreshed page restore the card at its
    // original sort position; focus stays on the title it was already on.
    commit(&coordinator, 3, Some((2, true, None)), all(), cx);
    draw(main, cx);
    assert_eq!(keys(cx), ["canonical:1", "canonical:2", "canonical:3"]);
    assert_eq!(focused(cx).as_deref(), Some("canonical:3"));
    let restored = browse.read_with(cx, |b, _| b.card_for_test(1));
    assert!(matches!(
        restored.read_with(cx, |c, _| c.membership),
        ToggleStatus::Known { on: true, .. }
    ));
}

#[gpui::test]
fn back_to_library_reruns_the_same_local_query_and_keeps_cards(cx: &mut TestAppContext) {
    let (coordinator, main) = fixture_window(cx);
    let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
    main.update(cx, |view, window, cx| {
        view.switch_scope(Scope::Library, window, cx)
    })
    .unwrap();
    coordinator.update(cx, |c, cx| c.load_library(Default::default(), cx));
    set_library(&coordinator, &[1, 2], cx);
    draw(main, cx);
    let before = coordinator.read_with(cx, |c, _| c.library.generation());
    main.update(cx, |view, window, cx| {
        view.open_title(BrowseTitle::Tmdb(movie(1, "Saved 1")), window, cx)
    })
    .unwrap();
    cx.run_until_parked();
    main.update(cx, |view, window, cx| view.leave_detail(window, cx))
        .unwrap();
    draw(main, cx);
    coordinator.read_with(cx, |c, _| {
        assert!(c.library.generation() > before, "same query re-run on Back");
    });
    browse.read_with(cx, |b, _| {
        assert_eq!(b.scope(), Scope::Library);
        assert_eq!(b.library_filters(), LibraryFilters::default());
        assert_eq!(b.card_keys().len(), 2, "loaded cards survive the refresh");
    });
}

#[gpui::test]
fn back_pops_streams_to_the_detail_page_then_to_browse(cx: &mut TestAppContext) {
    use super::super::detail::Page;
    cx.update(|cx| {
        theme::init(cx);
        theme::set_reduce_motion(true, cx);
        bind_keys(cx);
    });
    let mut title = movie(1, "Movie one");
    title.imdb = kinema_core::catalog::ImdbId::new("tt1234567").ok();
    let coordinator = cx.new(|cx| {
        let mut c = AppCoordinator::fixture(vec![BrowseTitle::Tmdb(title.clone())], None, cx);
        c.fixture_similar.insert(
            (1, 1),
            crate::tmdb::TmdbPage {
                titles: vec![],
                page: 1,
                total_pages: 1,
            },
        );
        c
    });
    let main =
        cx.add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
    let mut visual = VisualTestContext::from_window(main.into(), cx);
    main.update(cx, |view, window, cx| {
        view.open_title(BrowseTitle::Tmdb(title.clone()), window, cx)
    })
    .unwrap();
    cx.run_until_parked();
    let detail = mounted_detail(main, cx);
    detail.update(cx, |d, cx| d.run_primary_for_test(cx));
    cx.run_until_parked();
    assert_eq!(detail.read_with(cx, |d, _| d.page()), Page::Streams);
    visual.dispatch_action(V3Back);
    cx.run_until_parked();
    // Same Detail entity, now on its overview page: state intact.
    assert_eq!(detail.read_with(cx, |d, _| d.page()), Page::Overview);
    assert!(matches!(
        main.read_with(
            cx,
            |v, _| matches!(&v.route, Route::Detail(d) if *d == detail)
        ),
        Ok(true)
    ));
    visual.dispatch_action(V3Back);
    cx.run_until_parked();
    main.read_with(cx, |v, _| assert!(matches!(v.route, Route::Browse)))
        .unwrap();
}
