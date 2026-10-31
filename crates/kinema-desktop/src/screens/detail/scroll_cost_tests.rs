//! Measured cost of repeated overview builds while scrolling a series page.
//!
//! The counters come from `work`; timings are wall-clock microseconds of the
//! test build, so they are only meaningful as before/after comparisons on one
//! machine. Run with `--nocapture` to print the report table.
use super::{BrowseTitle, Detail, work};
use crate::{
    catalog::{
        library::{PersonalEpisode, PersonalTitle},
        playback_progress::{PlaybackCheckpoint, PlaybackProgressSnapshot},
    },
    screens::coordinator::{AppCoordinator, EpisodeRef},
    widgets::tokens,
};
use ::core::prelude::v1::test;
use gpui::{prelude::*, *};
use kinema_core::{
    MediaKind,
    catalog::{CatalogEpisode, CatalogTitle, ImdbId},
    library::{EpisodeId, TitleId, WatchState},
    watch_progress::{PlaybackObservation, PlaybackProgressTarget},
};

/// One scenario: `(seasons, episodes per season, season 1 episodes)`.
#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub label: &'static str,
    pub seasons: u32,
    pub per_season: u32,
    pub first_season: u32,
}
pub(super) const SHAPES: [Shape; 4] = [
    Shape {
        label: "typical 3x8",
        seasons: 3,
        per_season: 8,
        first_season: 8,
    },
    Shape {
        label: "24 eps x 3 seasons",
        seasons: 3,
        per_season: 24,
        first_season: 24,
    },
    Shape {
        label: "20 seasons x 24",
        seasons: 20,
        per_season: 24,
        first_season: 24,
    },
    Shape {
        label: "20 seasons, S1 = 150",
        seasons: 20,
        per_season: 24,
        first_season: 150,
    },
];

fn series_title() -> BrowseTitle {
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
fn count_for(shape: Shape, season: u32) -> u32 {
    if season == 1 {
        shape.first_season
    } else {
        shape.per_season
    }
}
fn episodes(shape: Shape) -> Vec<CatalogEpisode> {
    let mut out = Vec::new();
    for season in 1..=shape.seasons {
        for number in 1..=count_for(shape, season) {
            out.push(CatalogEpisode {
                still_url: (number % 2 == 1)
                    .then(|| format!("https://images.example.test/e-{season}-{number}.jpg")),
                runtime_minutes: Some(44),
                season,
                number,
                name: Some(format!("Episode {season}x{number}")),
                overview: Some("A signal reaches the harbor and nobody answers.".into()),
                released: Some(format!("{}-03-02", 2000 + season)),
            });
        }
    }
    out
}
fn progress(
    target: PlaybackProgressTarget,
    watch: Option<WatchState>,
    checkpoint: Option<i64>,
    updated_at: i64,
) -> PlaybackProgressSnapshot {
    PlaybackProgressSnapshot {
        target,
        watch,
        epoch: 0,
        active_run: 0,
        auto_mark_suppressed: false,
        checkpoint: checkpoint.and_then(|position| {
            Some(PlaybackCheckpoint {
                observation: PlaybackObservation::new(position, Some(2_700_000))?,
                updated_at,
                revision: 1,
            })
        }),
    }
}
/// Seed the shared personal snapshot: the first third of season 1 is watched
/// and the episode after it holds a resumable checkpoint.
pub(super) fn seed_personal(
    coordinator: &Entity<AppCoordinator>,
    title: &BrowseTitle,
    shape: Shape,
    cx: &mut TestAppContext,
) {
    let id = TitleId::new(41).unwrap_or_else(|| panic!("title id"));
    let personal = PersonalTitle {
        title_id: id,
        kind: MediaKind::Series,
        added_at: Some(1),
        movie_watch: None,
        playback_progress: None,
        show_summary: None,
        has_resume: true,
    };
    let mut rows = Vec::new();
    for season in 1..=shape.seasons {
        for number in 1..=count_for(shape, season) {
            let episode_id = EpisodeId::new(i64::from(season) * 1000 + i64::from(number))
                .unwrap_or_else(|| panic!("episode id"));
            let watched = season == 1 && number <= shape.first_season / 3;
            let checkpoint =
                (season == 1 && number == shape.first_season / 3 + 1).then_some(725_000);
            let watch = Some(WatchState {
                watched,
                updated_at: 1,
            });
            rows.push((
                EpisodeRef::Coordinates {
                    title: title.personal_ref(),
                    season,
                    episode: number,
                },
                PersonalEpisode {
                    episode_id,
                    watch,
                    playback_progress: progress(
                        PlaybackProgressTarget::Episode(episode_id),
                        watch,
                        checkpoint,
                        5,
                    ),
                },
            ));
        }
    }
    coordinator.update(cx, |c, cx| {
        c.fixture_personal(title, personal, rows);
        cx.notify();
    });
}
fn draw(window: WindowHandle<Detail>, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
    })
    .unwrap_or_else(|e| panic!("draw:{e}"));
}
/// A series window with `shape` loaded and personal state seeded and settled.
pub(super) fn window_for(
    shape: Shape,
    personal: bool,
    cx: &mut TestAppContext,
) -> (
    WindowHandle<Detail>,
    Entity<AppCoordinator>,
    VisualTestContext,
) {
    cx.update(|cx| {
        tokens::init(cx);
        tokens::set_reduce_motion(true, cx);
    });
    let title = series_title();
    let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
    let window =
        cx.add_window(|_, cx| Detail::fixture(coordinator.clone(), title.clone(), false, None, cx));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(800.)));
    coordinator.update(&mut visual, |c, cx| {
        let mut details = c
            .details
            .value()
            .cloned()
            .unwrap_or_else(|| panic!("fixture details"));
        details.episodes = episodes(shape);
        let generation = c.details.begin(true);
        c.details.fetched(generation, details, 1);
        cx.notify();
    });
    if personal {
        // Reads are seeded as already requested, like `seed_personal_fixture`.
        let id = TitleId::new(41).unwrap_or_else(|| panic!("title id"));
        window
            .update(&mut visual, |d, _, _| {
                for season in 1..=shape.seasons {
                    for number in 1..=count_for(shape, season) {
                        d.episode_personal_requested.insert((id, season, number));
                    }
                }
            })
            .unwrap_or_else(|e| panic!("requested:{e}"));
        seed_personal(&coordinator, &title, shape, &mut visual);
    }
    draw(window, &mut visual);
    draw(window, &mut visual);
    (window, coordinator, visual)
}

#[derive(Debug)]
pub(super) struct Cost {
    pub cards: usize,
    pub status: usize,
    pub known: usize,
    pub coordinates: usize,
    pub all: usize,
    pub saved: usize,
    pub art: usize,
    pub snapshot: usize,
    /// Microseconds per wheel step. A step is one wheel event (which itself
    /// triggers a draw) plus one explicit draw, so each step builds the
    /// overview twice. `step_us` covers both; `explicit_draw_us` only the
    /// explicit draw (the only part the first baseline timed). The section
    /// timers below also accumulate over both builds, so compare them with
    /// `step_us`, never with `explicit_draw_us`.
    pub step_us: u128,
    pub explicit_draw_us: u128,
    /// Episodes section, card loop, season selector and hero, per step.
    pub section_us: u128,
    pub cards_us: u128,
    pub selector_us: u128,
    pub hero_us: u128,
}
/// Ten wheel steps; counts and times are totals divided by ten (per step).
pub(super) fn measure_scroll(window: WindowHandle<Detail>, visual: &mut VisualTestContext) -> Cost {
    use work::{Section as S, Timed as T};
    const STEPS: u128 = 10;
    work::reset();
    let mut step_ns = 0u128;
    let mut draw_ns = 0u128;
    for _ in 0..STEPS {
        let step = std::time::Instant::now();
        visual.simulate_event(ScrollWheelEvent {
            position: point(px(640.), px(500.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
            ..Default::default()
        });
        let start = std::time::Instant::now();
        draw(window, visual);
        draw_ns += start.elapsed().as_nanos();
        step_ns += step.elapsed().as_nanos();
    }
    let per = |n: usize| n / STEPS as usize;
    Cost {
        cards: per(work::get(S::EpisodeCards)),
        status: per(work::get(S::EpisodeStatus)),
        known: per(work::get(S::KnownWatched)),
        coordinates: per(work::get(S::EpisodeCoordinates)),
        all: per(work::get(S::AllEpisodes)),
        saved: per(work::get(S::SavedProgress)),
        art: per(work::get(S::ArtLookup)),
        snapshot: per(work::get(S::SnapshotBuild)),
        step_us: step_ns / 1_000 / STEPS,
        explicit_draw_us: draw_ns / 1_000 / STEPS,
        section_us: work::micros(T::EpisodesSection) / STEPS,
        cards_us: work::micros(T::CardLoop) / STEPS,
        selector_us: work::micros(T::SeasonSelector) / STEPS,
        hero_us: work::micros(T::Hero) / STEPS,
    }
}

/// Prints the table the design document quotes. Reproduce with
/// `cargo test -p kinema-desktop --features ui-probe-test --locked --lib
/// scroll_cost_report -- --nocapture --test-threads=1` (use `--release` for
/// stable timings).
#[gpui::test]
fn scroll_cost_report(cx: &mut TestAppContext) {
    eprintln!(
        "per wheel step (event draw + explicit draw): shape | cards | status | known | coords | all_eps | saved | art | snap | step/explicit-draw/section/cards/selector/hero (us)"
    );
    for shape in SHAPES {
        let (window, _, mut visual) = window_for(shape, true, cx);
        let c = measure_scroll(window, &mut visual);
        eprintln!(
            "COST {:<22} | {:>4} | {:>6} | {:>5} | {:>6} | {:>7} | {:>5} | {:>4} | {:>4} | {}/{}/{}/{}/{}/{}",
            shape.label,
            c.cards,
            c.status,
            c.known,
            c.coordinates,
            c.all,
            c.saved,
            c.art,
            c.snapshot,
            c.step_us,
            c.explicit_draw_us,
            c.section_us,
            c.cards_us,
            c.selector_us,
            c.hero_us
        );
    }
}

// ---------------------------------------------------------------------------
// Regression: scrolling reuses the snapshot.

#[gpui::test]
fn wheel_scroll_reuses_personal_state_and_season_summaries(cx: &mut TestAppContext) {
    for shape in SHAPES {
        let (window, _, mut visual) = window_for(shape, true, cx);
        let c = measure_scroll(window, &mut visual);
        let name = shape.label;
        assert!(
            c.cards >= count_for(shape, 1) as usize,
            "{name}: cards still built"
        );
        // Check raw totals: integer per-step averages could round a single
        // unwanted rebuild down to zero and hide an invalidation regression.
        use work::Section as S;
        assert_eq!(
            (
                work::get(S::EpisodeStatus),
                work::get(S::KnownWatched),
                work::get(S::EpisodeCoordinates),
                work::get(S::AllEpisodes),
                work::get(S::SavedProgress),
                work::get(S::SnapshotBuild),
            ),
            (0, 0, 0, 0, 0, 0),
            "{name}: per-episode and whole-show personal work recomputed while scrolling: {c:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Snapshot values are exactly the direct reads.

type Reads = (
    Vec<(u32, u32, super::snapshot::EpisodeView)>,
    Vec<kinema_core::watch_progress::EpisodeMark>,
    Option<(u32, u32)>,
);
/// `(cached views, cached marks, cached next-up)` beside the same three
/// computed directly from the coordinator.
fn reads(d: &Detail, cx: &App) -> (Reads, Reads) {
    let coordinates = d.episode_coordinates(&d.season_options, cx);
    let snapshot = d.show_snapshot(cx);
    let cached: Vec<_> = coordinates
        .iter()
        .map(|&(s, n)| (s, n, d.episode_view(s, n, cx)))
        .collect();
    let direct: Vec<_> = coordinates
        .iter()
        .map(|&(s, n)| {
            let view = {
                let known = d.episode_known_watched(s, n, cx);
                let saved = d.saved_progress_direct(Some((s, n)), cx);
                super::snapshot::EpisodeView {
                    status: d.episode_status(s, n, cx),
                    known_watched: known,
                    saved,
                    visible: super::progress::visible_resume(saved, known),
                }
            };
            (s, n, view)
        })
        .collect();
    let marks = d.episode_marks(&d.season_options, cx);
    let resumed = kinema_core::watch_progress::latest_resume(
        direct
            .iter()
            .filter_map(|(s, n, v)| Some(((*s, *n), v.visible?.updated_at))),
    );
    let next = resumed.or_else(|| kinema_core::watch_progress::next_up(&marks, &d.season_options));
    (
        (cached, snapshot.marks.clone(), snapshot.next_up),
        (direct, marks, next),
    )
}
fn assert_snapshot_matches_direct(
    window: WindowHandle<Detail>,
    visual: &mut VisualTestContext,
    what: &str,
) -> Reads {
    window
        .read_with(visual, |d, cx| {
            let (cached, direct) = reads(d, cx);
            assert_eq!(cached.0, direct.0, "{what}: per-episode views");
            assert_eq!(cached.1, direct.1, "{what}: marks");
            assert_eq!(cached.2, direct.2, "{what}: next up");
            cached
        })
        .unwrap_or_else(|e| panic!("reads:{e}"))
}

#[gpui::test]
fn snapshot_equals_direct_reads_seeded_unseeded_and_local(cx: &mut TestAppContext) {
    for personal in [true, false] {
        let (window, _, mut visual) = window_for(SHAPES[2], personal, cx);
        let (views, marks, next) =
            assert_snapshot_matches_direct(window, &mut visual, &format!("cinemeta {personal}"));
        if personal {
            assert_eq!(next, Some((1, 9)), "the unfinished resume wins");
            assert!(!marks.is_empty());
        } else {
            // Nothing is known: unknown is neither watched nor a mark.
            assert!(marks.is_empty() && views.iter().all(|(_, _, v)| v.known_watched.is_none()));
        }
    }
    // Local title with no seeded personal rows: the persisted detail rows are
    // the fallback for watched state while the shared read is unknown.
    cx.update(|cx| {
        tokens::init(cx);
        tokens::set_reduce_motion(true, cx);
    });
    let title = super::fixtures::local_fixture_title(true);
    let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
    let window = cx.add_window(|_, cx| Detail::fixture(coordinator, title, false, None, cx));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(800.)));
    draw(window, &mut visual);
    let (views, marks, _) = assert_snapshot_matches_direct(window, &mut visual, "local fallback");
    assert!(!marks.is_empty(), "local rows still supply known state");
    assert!(
        views
            .iter()
            .any(|(_, _, v)| !matches!(v.status, ToggleStatus::Known { .. })
                && v.known_watched.is_some()),
        "the fallback applies where the shared read is not Known"
    );
}

// ---------------------------------------------------------------------------
// Warmed snapshot follows changes.

use crate::widgets::state_toggle::ToggleStatus;

fn episode_row(
    title: &BrowseTitle,
    season: u32,
    number: u32,
    watched: bool,
    checkpoint: Option<(i64, i64)>,
) -> (EpisodeRef, PersonalEpisode) {
    let episode_id = EpisodeId::new(i64::from(season) * 1000 + i64::from(number))
        .unwrap_or_else(|| panic!("episode id"));
    let watch = Some(WatchState {
        watched,
        updated_at: 1,
    });
    (
        EpisodeRef::Coordinates {
            title: title.personal_ref(),
            season,
            episode: number,
        },
        PersonalEpisode {
            episode_id,
            watch,
            playback_progress: progress(
                PlaybackProgressTarget::Episode(episode_id),
                watch,
                checkpoint.map(|c| c.0),
                checkpoint.map_or(0, |c| c.1),
            ),
        },
    )
}
fn set_episode(
    coordinator: &Entity<AppCoordinator>,
    row: (EpisodeRef, PersonalEpisode),
    state: crate::screens::coordinator::PersonalLoad<PersonalEpisode>,
    pending: bool,
    unmapped: bool,
    cx: &mut VisualTestContext,
) {
    coordinator.update(cx, |c, cx| {
        c.fixture_episode_state(&row.0, state, pending, unmapped, cx)
    });
    cx.run_until_parked();
}

#[gpui::test]
fn watched_and_progress_changes_refresh_cards_and_next_up(cx: &mut TestAppContext) {
    let (window, coordinator, mut visual) = window_for(SHAPES[0], true, cx);
    let title = series_title();
    let view = |visual: &mut VisualTestContext, n: u32| {
        window
            .read_with(visual, |d, cx| d.episode_view(1, n, cx))
            .unwrap_or_else(|e| panic!("view:{e}"))
    };
    let next = |visual: &mut VisualTestContext| {
        window
            .read_with(visual, |d, cx| d.next_up(cx))
            .unwrap_or_else(|e| panic!("next:{e}"))
    };
    let bar = |visual: &mut VisualTestContext, n: u32| {
        window
            .read_with(visual, |d, cx| d.episode_progress_bar(1, n, cx).is_some())
            .unwrap_or_else(|e| panic!("bar:{e}"))
    };
    use crate::screens::coordinator::PersonalLoad::Ready;
    // Warmed: E1-E2 watched, E3 holds the only checkpoint.
    assert_eq!(next(&mut visual), Some((1, 3)));
    assert!(view(&mut visual, 3).visible.is_some() && bar(&mut visual, 3));
    // Watching E3 hides its checkpoint; it stops being the continue target.
    let row = episode_row(&title, 1, 3, true, Some((725_000, 5)));
    set_episode(
        &coordinator,
        row.clone(),
        Ready(row.1.clone()),
        false,
        false,
        &mut visual,
    );
    let v = view(&mut visual, 3);
    assert_eq!(v.known_watched, Some(true));
    assert!(v.saved.is_some(), "the checkpoint is kept, only hidden");
    assert!(v.visible.is_none() && !bar(&mut visual, 3));
    assert_eq!(next(&mut visual), Some((1, 4)), "next unwatched episode");
    // A later unfinished resume elsewhere wins over the earlier one.
    for (n, at) in [(6, 9), (5, 7)] {
        let row = episode_row(&title, 1, n, false, Some((300_000, at)));
        set_episode(
            &coordinator,
            row.clone(),
            Ready(row.1.clone()),
            false,
            false,
            &mut visual,
        );
    }
    assert_eq!(next(&mut visual), Some((1, 6)), "most recently resumed");
    assert!(bar(&mut visual, 6) && bar(&mut visual, 5));
    assert_snapshot_matches_direct(window, &mut visual, "after changes");
    // Settings: an external player keeps the checkpoint visible but offers no resume.
    let resume = |visual: &mut VisualTestContext| {
        window
            .update(visual, |d, _, cx| {
                (
                    d.visible_progress(Some((1, 6)), cx).is_some(),
                    d.resume_offered(Some((1, 6)), cx).is_some(),
                    d.progress_menu(Some((1, 6)), cx).len(),
                )
            })
            .unwrap_or_else(|e| panic!("resume:{e}"))
    };
    assert_eq!(resume(&mut visual), (true, true, 2));
    coordinator.update(&mut visual, |c, cx| {
        if let Some(s) = c.settings.as_mut() {
            s.player = crate::player::PlayerChoice::Mpv;
        }
        cx.notify();
    });
    visual.run_until_parked();
    assert_eq!(
        resume(&mut visual),
        (true, false, 1),
        "saved stays shown, resume and Play from beginning are withdrawn"
    );
    coordinator.update(&mut visual, |c, cx| {
        if let Some(s) = c.settings.as_mut() {
            s.player = crate::player::PlayerChoice::Embedded;
        }
        cx.notify();
    });
    visual.run_until_parked();
    assert_eq!(resume(&mut visual), (true, true, 2));
}

#[gpui::test]
fn loading_failed_and_pending_states_stay_distinct(cx: &mut TestAppContext) {
    use crate::screens::coordinator::PersonalLoad::{Failed, Loading, Ready};
    let (window, coordinator, mut visual) = window_for(SHAPES[0], true, cx);
    let title = series_title();
    let read = |visual: &mut VisualTestContext| {
        window
            .read_with(visual, |d, cx| {
                (
                    d.episode_view(1, 5, cx),
                    d.show_snapshot(cx)
                        .marks
                        .iter()
                        .any(|m| (m.season, m.number) == (1, 5)),
                )
            })
            .unwrap_or_else(|e| panic!("read:{e}"))
    };
    let (view, marked) = read(&mut visual);
    assert_eq!(
        view.status,
        ToggleStatus::Known {
            on: false,
            pending: false
        }
    );
    assert!(marked);
    let row = episode_row(&title, 1, 5, false, None);
    // Still loading: unknown, not writable, not a mark.
    set_episode(
        &coordinator,
        row.clone(),
        Loading,
        false,
        false,
        &mut visual,
    );
    let (view, marked) = read(&mut visual);
    assert_eq!(
        (view.status, view.known_watched, marked),
        (ToggleStatus::Loading, None, false)
    );
    // Failed read: unknown, retryable, not a mark.
    set_episode(
        &coordinator,
        row.clone(),
        Failed("offline".into()),
        false,
        false,
        &mut visual,
    );
    let (view, marked) = read(&mut visual);
    assert_eq!(
        (view.status, view.known_watched, marked),
        (ToggleStatus::Failed, None, false)
    );
    // Failed read of an episode with no stored row counts unwatched but stays Failed.
    set_episode(
        &coordinator,
        row.clone(),
        Failed("no row".into()),
        false,
        true,
        &mut visual,
    );
    let (view, marked) = read(&mut visual);
    assert_eq!(
        (view.status, view.known_watched, marked),
        (ToggleStatus::Failed, Some(false), true)
    );
    // A write in flight shows the value but disables the control.
    let watched = episode_row(&title, 1, 5, true, None);
    set_episode(
        &coordinator,
        watched.clone(),
        Ready(watched.1.clone()),
        true,
        false,
        &mut visual,
    );
    let (view, _) = read(&mut visual);
    assert_eq!(
        view.status,
        ToggleStatus::Known {
            on: true,
            pending: true
        }
    );
    set_episode(
        &coordinator,
        watched.clone(),
        Ready(watched.1),
        false,
        false,
        &mut visual,
    );
    let (view, _) = read(&mut visual);
    assert_eq!(
        view.status,
        ToggleStatus::Known {
            on: true,
            pending: false
        }
    );
    assert_snapshot_matches_direct(window, &mut visual, "after state changes");
}
