#[test]
fn deferred_close_checkpoint_is_an_owned_pre_teardown_observation()
-> Result<(), Box<dyn std::error::Error>> {
    use super::{PlaybackIntent, PlaybackLaunch, Progress, ProgressPhase};
    use kinema_core::{library::TitleId, watch_progress::PlaybackProgressTarget};
    let captured = {
        let mut progress = Progress::new(PlaybackLaunch {
            subject: PlaybackProgressTarget::Movie(
                TitleId::new(1).ok_or("expected valid checkpoint fixture")?,
            ),
            epoch: 3,
            run: 3,
            checkpoint: None,
            watched: false,
            intent: PlaybackIntent::Normal,
        });
        progress.phase = ProgressPhase::Active;
        progress.authorized = true;
        progress.generation = Some(1);
        let state = kinema_mpv::PlaybackState {
            position: 24.0,
            position_revision: 1,
            position_observed: true,
            file_loaded: true,
            media_stopped: false,
            media_generation: 1,
            ..Default::default()
        };
        progress
            .sample(&state, std::time::Instant::now(), true)
            .ok_or("expected valid checkpoint fixture")?
    }; // Delivery owns values, not a player/view borrow.
    assert_eq!(captured.epoch, 3);
    assert_eq!(captured.run, 3);
    assert_eq!(captured.observation.position_ms(), 24_000);
    assert!(captured.force);
    Ok(())
}

#[test]
fn desktop_metadata_never_publishes_untracked_filenames_or_urls() {
    let title = kinema_core::media::playback_title("/private/SECRET.mkv?token=secret", None);
    let snapshot = super::desktop_snapshot(
        super::desktop_target(7),
        &title,
        false,
        &kinema_mpv::PlaybackState::default(),
    );
    assert_eq!(snapshot.title, "Local video");
    assert_eq!(snapshot.description, None);
    let public = kinema_core::media::playback_title("Public movie", None);
    assert_eq!(
        super::desktop_snapshot(
            super::desktop_target(7),
            &public,
            true,
            &kinema_mpv::PlaybackState::default()
        )
        .title,
        "Public movie"
    );
}
#[test]
fn desktop_snapshot_uses_observed_state_and_valid_units() {
    use crate::desktop_integration::Playback;
    let mut state = kinema_mpv::PlaybackState {
        file_loaded: true,
        position_observed: true,
        media_stopped: false,
        position: 12.5,
        duration: Some(90.0),
        volume: 75.0,
        seekable: Some(true),
        ..Default::default()
    };
    let title = kinema_core::media::playback_title("Public movie", None);
    let snapshot = super::desktop_snapshot(super::desktop_target(1), &title, true, &state);
    assert_eq!(snapshot.playback, Playback::Playing);
    assert_eq!(snapshot.position, std::time::Duration::from_millis(12_500));
    assert_eq!(snapshot.duration, Some(std::time::Duration::from_secs(90)));
    assert_eq!(snapshot.volume, 0.75);
    assert!(snapshot.seekable);
    state.paused = true;
    assert_eq!(
        super::desktop_snapshot(super::desktop_target(1), &title, true, &state).playback,
        Playback::Paused
    );
    state.loading = true;
    state.position = f64::NAN;
    state.duration = Some(f64::INFINITY);
    let snapshot = super::desktop_snapshot(super::desktop_target(1), &title, true, &state);
    assert_eq!(snapshot.playback, Playback::Stopped);
    assert!(!snapshot.seekable);
    assert_eq!(snapshot.position, std::time::Duration::ZERO);
    assert_eq!(snapshot.duration, None);
}
#[test]
fn desktop_seek_capability_and_bounds_are_not_speculative() {
    let mut state = kinema_mpv::PlaybackState {
        duration: Some(90.0),
        ..Default::default()
    };
    assert!(!super::desktop_can_seek(&state));
    state.file_loaded = true;
    state.position_observed = true;
    state.media_stopped = false;
    state.seekable = Some(false);
    assert!(!super::desktop_can_seek(&state));
    state.seekable = Some(true);
    assert!(super::desktop_can_seek(&state));
    assert_eq!(super::bounded_seek(-10.0, Some(90.0)), 0.0);
    assert_eq!(super::bounded_seek(120.0, Some(90.0)), 90.0);
    assert_eq!(super::bounded_seek(120.0, None), 120.0);
    assert_ne!(super::desktop_target(1), super::desktop_target(2));
}

#[test]
fn player_title_drops_episode_name_before_episode_identity() {
    let title =
        kinema_core::media::playback_title("The Outsider", Some((1, 4, Some("Que Viene el Coco"))));
    assert_eq!(
        super::title_for_width(&title, gpui::px(500.), gpui::px(500.)),
        title.full
    );
    assert_eq!(
        super::title_for_width(&title, gpui::px(500.), gpui::px(499.)),
        title.compact
    );
    assert_eq!(
        super::title_for_width(&title, gpui::px(500.), gpui::px(0.)),
        title.compact
    );
}

#[test]
fn embedded_player_icon_decodes_and_differs_from_application_icon()
-> Result<(), Box<dyn std::error::Error>> {
    let player = super::player_window_icon().ok_or("invalid embedded player PNG")?;
    let application = image::load_from_memory(include_bytes!(
        "../../../../assets/brand/png/256/dev.tlmtech.kinema.png"
    ))?
    .into_rgba8();
    assert_eq!(player.dimensions(), (256, 256));
    assert_ne!(player.as_ref(), &application);
    Ok(())
}

#[cfg(feature = "ui-probe")]
#[test]
fn probe_controls_require_observed_mpv_state_for_every_phase() {
    use super::{ProbeCommand, ProbeControls};
    let mut controls = ProbeControls::default();
    let mut state = kinema_mpv::PlaybackState {
        duration: Some(12.),
        ..Default::default()
    };
    assert_eq!(controls.observe(&state, 2), None);
    assert_eq!(controls.observe(&state, 3), Some(ProbeCommand::Pause));
    assert_eq!(controls.observe(&state, 3), None);
    state.paused = true;
    assert_eq!(controls.observe(&state, 3), Some(ProbeCommand::Adjust));
    assert_eq!(controls.observe(&state, 4), None);
    state.position = controls.target;
    state.speed = 1.25;
    assert_eq!(controls.observe(&state, 4), None);
    state.volume = 37.;
    assert_eq!(controls.observe(&state, 4), Some(ProbeCommand::Resume));
    assert!(!controls.verified());
    state.paused = false;
    assert_eq!(controls.observe(&state, 4), None);
    state.position += 0.1;
    assert_eq!(controls.observe(&state, 5), Some(ProbeCommand::Restore));
    assert!(!controls.verified());
    state.speed = 1.;
    state.volume = 100.;
    assert_eq!(controls.observe(&state, 5), None);
    assert!(controls.verified());
}

#[cfg(feature = "ui-probe")]
#[test]
fn probe_resize_requires_changes_not_requests_or_repeated_samples() {
    let mut resize = super::ProbeResize {
        requested: 2,
        ..Default::default()
    };
    resize.observe([1100, 680]);
    resize.observe([1100, 680]);
    assert!(!resize.verified());
    resize.observe([940, 620]);
    resize.observe([940, 620]);
    assert_eq!(resize.observed, 1);
    assert!(!resize.verified());
    resize.observe([1260, 760]);
    assert!(resize.verified());
}

#[test]
fn failure_labels_never_use_private_error_text() {
    let secret =
        "https://secret.invalid/media?credential=SECRET_MARKER /private/media raw-driver-error";
    let state = PlaybackState {
        error: Some(secret.into()),
        fatal_error: Some(secret.into()),
        ..Default::default()
    };
    for reason in [
        PlaybackFailure::Unknown,
        PlaybackFailure::BackendNotCompiled,
        PlaybackFailure::GpuContextUnavailable,
        PlaybackFailure::HdrUnsupported,
        PlaybackFailure::ColorMetadataUnavailable,
        PlaybackFailure::ColorTransferUnqualified,
        PlaybackFailure::DecodeMetadataUnavailable,
        PlaybackFailure::DecoderNotDirect,
        PlaybackFailure::GpuDeviceUnsupported,
        PlaybackFailure::GpuInteropUnavailable,
        PlaybackFailure::GpuCompletionFailed,
        PlaybackFailure::LibmpvUnavailable,
        PlaybackFailure::LibmpvIncompatible,
        PlaybackFailure::RenderInitFailed,
        PlaybackFailure::MediaLoadFailed,
        PlaybackFailure::RenderSizeLimit,
    ] {
        let label = failure_label(failure_reason(&state, Some(reason)));
        assert_eq!(label, format!("{}: {}", reason.code(), reason.message()));
        for forbidden in [
            "https://",
            "SECRET_MARKER",
            "/private/media",
            "raw-driver-error",
        ] {
            assert!(!label.contains(forbidden));
        }
    }
    assert_eq!(failure_reason(&state, None), PlaybackFailure::Unknown);
    let state = PlaybackState {
        failure_reason: Some(PlaybackFailure::GpuContextUnavailable),
        ..state
    };
    assert_eq!(
        failure_reason(&state, Some(PlaybackFailure::Unknown)),
        PlaybackFailure::GpuContextUnavailable
    );
}

#[test]
fn startup_failure_preserves_typed_reason_and_safe_phase() {
    for phase in [FailurePhase::Initialize, FailurePhase::Load] {
        let raw = "https://SECRET.invalid/private raw-driver-error";
        let error = kinema_mpv::Error::Library {
            attempts: raw.into(),
        };
        let expected = error.playback_failure();
        let (_, reason, actual_phase) = startup_error(error, phase);
        assert_eq!(reason, expected);
        assert_eq!(actual_phase, phase);
        assert!(!actual_phase.code().contains("SECRET"));
    }
    let state = PlaybackState::default();
    assert_eq!(failure_phase(&state, None), FailurePhase::Runtime);
    assert_eq!(
        failure_phase(&state, Some(FailurePhase::Load)),
        FailurePhase::Load
    );
    let state = PlaybackState {
        failure_reason: Some(PlaybackFailure::Unknown),
        ..state
    };
    assert_eq!(
        failure_phase(&state, Some(FailurePhase::Load)),
        FailurePhase::Runtime
    );
}

#[test]
fn embedded_failure_reports_once_not_each_sample() {
    let mut reported = false;
    assert!(!failure_transition(false, &mut reported));
    assert!(failure_transition(true, &mut reported));
    for _ in 0..120 {
        assert!(!failure_transition(true, &mut reported));
    }
    assert!(!failure_transition(false, &mut reported));
    assert!(!failure_transition(true, &mut reported));
}

#[cfg(feature = "gpu-surface")]
#[test]
fn absent_renderer_context_has_distinct_startup_reason() {
    let Err((_, reason, phase)) = required_gpu_context::<()>(None) else {
        panic!("missing renderer context must fail closed");
    };
    assert_eq!(reason, PlaybackFailure::GpuContextUnavailable);
    assert_eq!(phase, FailurePhase::Config);
    assert_eq!(required_gpu_context(Some(42)), Ok(42));
    // Unit tests register no native window renderer. This only reads its
    // context registry; it must not create a device or silently fall back.
    let Err((_, reason, phase)) = player_config() else {
        panic!("embedded startup requires the actual window renderer context");
    };
    assert_eq!(reason, PlaybackFailure::GpuContextUnavailable);
    assert_eq!(phase, FailurePhase::Config);
}

#[cfg(not(feature = "gpu-surface"))]
#[test]
fn embedded_backend_is_explicitly_unavailable_without_feature() {
    let result = super::player_config();
    let Err((error, reason, phase)) = result else {
        panic!("feature-off backend must fail closed");
    };
    assert_eq!(reason, PlaybackFailure::BackendNotCompiled);
    assert_eq!(phase, FailurePhase::Config);
    assert!(error.contains("not compiled"));
    assert!(error.contains("gpu-surface"));
}

use super::*;
use ::core::prelude::v1::test;
#[test]
fn performance_opt_in_is_exact_and_categories_are_closed() {
    for value in [
        None,
        Some(""),
        Some("0"),
        Some("true"),
        Some(" 1"),
        Some("1 "),
    ] {
        assert!(!diagnostics_enabled(value));
    }
    assert!(diagnostics_enabled(Some("1")));
    assert_eq!(decoder_category(Some("vaapi-copy")), "vaapi");
    assert_eq!(
        decoder_category(Some("https://secret.invalid/token")),
        "other"
    );
    assert_eq!(renderer_category(Some("llvmpipe (LLVM)")), "software");
    assert_eq!(renderer_category(Some("private driver string")), "other");
}
#[test]
fn performance_rates_use_window_time_and_reject_invalid_time() {
    assert_eq!(window_rate(150, 5.), 30.);
    assert_eq!(window_rate(0, 5.), 0.);
    for seconds in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(window_rate(150, seconds), 0.);
    }
}
#[test]
fn buffered_fractions_drop_invalid_and_clamp_without_widening() {
    use kinema_mpv::BufferedRange as R;
    let r = |start, end| R { start, end };
    let ranges = [
        r(10., 20.),
        r(20., 20.),
        r(30., 10.),
        r(f64::NAN, 5.),
        r(5., f64::INFINITY),
        r(-5., 10.),
        r(90., 200.),
        r(150., 200.),
    ];
    assert_eq!(
        buffered_fractions(Some(&ranges), 100.),
        vec![(0.1, 0.2), (0., 0.1), (0.9, 1.)]
    );
    assert!(buffered_fractions(None, 100.).is_empty());
    assert!(buffered_fractions(Some(&[]), 100.).is_empty());
    assert!(buffered_fractions(Some(&ranges), 0.).is_empty());
    assert!(buffered_fractions(Some(&ranges), f64::NAN).is_empty());
}
#[test]
fn transfer_rate_is_unknown_until_observed_and_ignores_baseline() {
    let t0 = Instant::now();
    let secs = |n| t0 + Duration::from_secs(n);
    let mut rate = TransferRate::default();
    assert_eq!(rate.bytes_per_second(), None);
    rate.observe("a", Some((500_000_000, 1_000_000_000)), t0);
    assert_eq!(rate.bytes_per_second(), None);
    rate.observe("a", Some((500_000_000, 1_000_000_000)), secs(2));
    assert_eq!(rate.bytes_per_second(), Some(0.));
    rate.observe("a", Some((502_000_000, 1_000_000_000)), secs(4));
    assert_eq!(rate.bytes_per_second(), Some(500_000.));
    // Samples older than the window are pruned.
    rate.observe("a", Some((502_000_000, 1_000_000_000)), secs(10));
    assert_eq!(rate.bytes_per_second(), Some(0.));
}
#[test]
fn transfer_rate_resets_on_session_regression_and_missing_progress() {
    let t0 = Instant::now();
    let mut rate = TransferRate::default();
    rate.observe("a", Some((100, 1000)), t0);
    rate.observe(
        "a",
        Some((3_000_100, 5_000_000)),
        t0 + Duration::from_secs(2),
    );
    assert!(rate.bytes_per_second().is_some());
    rate.observe("a", Some((50, 5_000_000)), t0 + Duration::from_secs(3));
    assert_eq!(rate.bytes_per_second(), None);
    rate.observe("a", Some((60, 5_000_000)), t0 + timing::METRICS_REPORT);
    assert!(rate.bytes_per_second().is_some());
    rate.observe("b", Some((60, 5_000_000)), t0 + Duration::from_secs(6));
    assert_eq!(rate.bytes_per_second(), None);
    rate.observe("b", None, t0 + Duration::from_secs(8));
    assert_eq!(rate.latest, None);
    assert_eq!(download_label(&rate), None);
}
#[test]
fn cadence_counts_playing_rate_and_gap_and_resets_per_interval() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    let mut meter = CadenceMeter::new(t0);
    // Initial loading: no rate, and the first accepted image is only a baseline.
    for n in (16..=300).step_by(16) {
        meter.sample(ms(n), Activity::Waiting, false);
    }
    meter.sample(ms(304), Activity::Playing, true);
    for n in [350, 400, 450, 500] {
        meter.sample(ms(n), Activity::Playing, true);
    }
    meter.sample(ms(900), Activity::Playing, false);
    let Some(report) = meter.finish_if_due(ms(1000)) else {
        unreachable!("interval due")
    };
    assert_eq!(report.accepted, 5);
    assert!(
        report
            .fps
            .is_some_and(|fps| (fps - 5. / 0.696).abs() < 0.01)
    );
    // The open gap since the last image (500 -> 1000) is included.
    assert_eq!(report.max_accept_gap_ms, Some(500.));
    assert_eq!(report.sample_max_gap_ms, 400.);
    assert!(meter.finish_if_due(ms(1500)).is_none());
    let Some(next) = meter.finish_if_due(ms(2000)) else {
        unreachable!("interval due")
    };
    assert_eq!(next.accepted, 0);
    assert_eq!(next.fps, Some(0.));
}
#[test]
fn cadence_pause_buffering_and_reload_are_states_not_regressions() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    let mut meter = CadenceMeter::new(t0);
    meter.sample(ms(100), Activity::Playing, true);
    meter.sample(ms(200), Activity::Paused, false);
    let Some(paused) = meter.finish_if_due(ms(1000)) else {
        unreachable!("interval due")
    };
    assert_eq!(paused.label(), "Video updates: Paused");
    assert_eq!(paused.max_accept_gap_ms, None);
    meter.sample(ms(1100), Activity::Buffering, false);
    meter.sample(ms(1200), Activity::Buffering, false);
    let Some(buffering) = meter.finish_if_due(ms(2000)) else {
        unreachable!("interval due")
    };
    assert_eq!(buffering.fps, None);
    assert_eq!(buffering.cache_paused_samples, 2);
    assert_eq!(buffering.label(), "Video updates: Buffering");
    // Resume: the first image after the idle period must not span the gap.
    meter.sample(ms(2500), Activity::Playing, true);
    meter.sample(ms(2540), Activity::Playing, true);
    let Some(resumed) = meter.finish_if_due(ms(3000)) else {
        unreachable!("interval due")
    };
    assert_eq!(resumed.accepted, 2);
    assert_eq!(resumed.max_accept_gap_ms, Some(460.));
    // A replay passes through Stopped/Waiting and re-baselines.
    meter.sample(ms(3100), Activity::Stopped, false);
    meter.sample(ms(3200), Activity::Waiting, false);
    meter.sample(ms(3300), Activity::Playing, true);
    let Some(replay) = meter.finish_if_due(ms(4000)) else {
        unreachable!("interval due")
    };
    assert!(replay.max_accept_gap_ms.is_some_and(|gap| gap <= 700.));
    assert!(replay.label().starts_with("Video updates: "));
}
#[test]
fn cadence_long_stall_yields_one_honest_report() {
    let t0 = Instant::now();
    let mut meter = CadenceMeter::new(t0);
    meter.sample(t0 + timing::FRAME_TICK, Activity::Playing, true);
    meter.sample(t0 + Duration::from_millis(3016), Activity::Playing, true);
    let Some(report) = meter.finish_if_due(t0 + Duration::from_millis(3020)) else {
        unreachable!("interval due")
    };
    assert_eq!(report.accepted, 2);
    assert_eq!(report.max_accept_gap_ms, Some(3000.));
    assert!(report.interval_seconds > 3.);
    assert!(report.fps.is_some_and(|fps| fps < 1.));
    assert!(
        meter
            .finish_if_due(t0 + Duration::from_millis(3100))
            .is_none()
    );
}
#[test]
fn activity_distinguishes_states() {
    let mut state = PlaybackState {
        loading: false,
        paused: false,
        paused_for_cache: false,
        ended: false,
        media_stopped: false,
        ..Default::default()
    };
    assert_eq!(activity_of(&state, true), Activity::Playing);
    state.ended = true;
    assert_eq!(activity_of(&state, true), Activity::Stopped);
    state.ended = false;
    assert_eq!(activity_of(&state, false), Activity::Waiting);
    state.paused = true;
    assert_eq!(activity_of(&state, true), Activity::Paused);
    state.paused_for_cache = true;
    assert_eq!(activity_of(&state, true), Activity::Buffering);
    state.loading = true;
    assert_eq!(activity_of(&state, true), Activity::Waiting);
}

#[test]
fn cadence_counts_view_renders_and_gaps_only_while_playing() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    let mut meter = CadenceMeter::new(t0);
    meter.sample(ms(10), Activity::Playing, false);
    meter.render_called(ms(20));
    meter.render_called(ms(60));
    meter.sample(ms(70), Activity::Playing, false);
    let Some(report) = meter.finish_if_due(ms(1000)) else {
        unreachable!("interval due")
    };
    assert_eq!(report.view_render_calls, 2);
    // The open gap since the last render (60 -> 1000) is included.
    assert_eq!(report.max_view_render_gap_ms, Some(940.));
    meter.sample(ms(1100), Activity::Paused, false);
    meter.render_called(ms(1200));
    let Some(next) = meter.finish_if_due(ms(2000)) else {
        unreachable!("interval due")
    };
    assert_eq!(next.view_render_calls, 1);
    assert_eq!(next.max_view_render_gap_ms, None);
}
#[test]
fn transfer_poll_is_bounded_and_display_key_tracks_changes() {
    let t0 = Instant::now();
    assert!(transfer_poll_due(None, t0));
    assert!(!transfer_poll_due(
        Some(t0),
        t0 + Duration::from_millis(900)
    ));
    assert!(transfer_poll_due(Some(t0), t0 + Duration::from_secs(1)));
    let mut rate = TransferRate::default();
    let empty = rate.display_key();
    rate.observe("a", Some((10, 100)), t0);
    assert_ne!(rate.display_key(), empty);
    rate.clear();
    assert_eq!(rate.display_key(), empty);
}
#[test]
fn buffer_and_download_labels_are_distinct_and_honest() {
    let mut state = PlaybackState {
        position: 10.,
        ..Default::default()
    };
    assert_eq!(buffer_ahead_label(&state), "Buffer —");
    state.buffered_ranges = Some(vec![]);
    assert_eq!(buffer_ahead_label(&state), "Buffer 0s");
    state.buffered_ranges = Some(vec![kinema_mpv::BufferedRange {
        start: 0.,
        end: 145.,
    }]);
    assert_eq!(buffer_ahead_label(&state), "Buffer 2m 15s");
    let mut transfer = TransferRate::default();
    let t0 = Instant::now();
    transfer.observe("a", Some((1_500_000, 4_500_000)), t0);
    assert_eq!(
        download_label(&transfer).as_deref(),
        Some("Downloaded 1.5 MB of 4.5 MB (33%) · —")
    );
    transfer.observe(
        "a",
        Some((2_500_000, 4_500_000)),
        t0 + Duration::from_secs(2),
    );
    assert_eq!(
        download_label(&transfer).as_deref(),
        Some("Downloaded 2.5 MB of 4.5 MB (55%) · 500.0 KB/s")
    );
    assert_eq!(format_bytes(999), "999 B");
    // Position 10: ahead = end - 10, rounded once before splitting.
    for (end, label) in [
        (129.6, "Buffer 2m 00s"),
        (189.9, "Buffer 3m 00s"),
        (129.4, "Buffer 119s"),
        (10. + 180.4, "Buffer 3m 00s"),
    ] {
        state.buffered_ranges = Some(vec![kinema_mpv::BufferedRange { start: 0., end }]);
        assert_eq!(buffer_ahead_label(&state), label);
    }
    let mut huge = TransferRate::default();
    huge.observe("a", Some((u64::MAX, u64::MAX)), t0);
    assert!(download_label(&huge).is_some_and(|label| label.contains("100%")));
}
#[test]
fn compact_status_omits_unknown_buffer_and_finished_downloads() {
    let mut state = PlaybackState {
        position: 10.,
        ..Default::default()
    };
    let mut transfer = TransferRate::default();
    assert!(transfer_items(&state, &transfer).is_empty());
    state.buffered_ranges = Some(vec![kinema_mpv::BufferedRange {
        start: 0.,
        end: 403.,
    }]);
    let t0 = Instant::now();
    transfer.observe("a", Some((1_500_000, 4_500_000)), t0);
    transfer.observe(
        "a",
        Some((2_500_000, 4_500_000)),
        t0 + Duration::from_secs(2),
    );
    assert_eq!(
        transfer_items(&state, &transfer),
        vec![
            StatusItem::icon(IconName::Timer, "6m 33s"),
            StatusItem::icon(IconName::Download, "55% · 500.0 KB/s"),
        ]
    );
    // Complete download: no "100% · 0 B/s" noise.
    transfer.observe(
        "a",
        Some((4_500_000, 4_500_000)),
        t0 + Duration::from_secs(4),
    );
    assert_eq!(
        transfer_items(&state, &transfer),
        vec![StatusItem::icon(IconName::Timer, "6m 33s")]
    );
    // Unknown total keeps byte count; no fabricated percentage.
    let mut unknown = TransferRate::default();
    unknown.observe("b", Some((1_500_000, 0)), t0);
    state.buffered_ranges = None;
    assert_eq!(
        transfer_items(&state, &unknown),
        vec![StatusItem::icon(IconName::Download, "1.5 MB")]
    );
    state.ended = true;
    assert!(transfer_items(&state, &unknown).is_empty());
}
#[test]
fn performance_cache_preserves_unknown_empty_and_containing_ranges() {
    use kinema_mpv::BufferedRange;
    let mut state = PlaybackState {
        position: 10.,
        ..Default::default()
    };
    assert_eq!(cache_ahead(&state), None);
    state.buffered_ranges = Some(vec![]);
    assert_eq!(cache_ahead(&state), Some(0.));
    state.buffered_ranges = Some(vec![
        BufferedRange {
            start: 15.,
            end: 20.,
        },
        BufferedRange {
            start: 25.,
            end: 30.,
        },
        BufferedRange {
            start: 8.,
            end: 15.,
        },
        BufferedRange {
            start: f64::NAN,
            end: 99.,
        },
        BufferedRange {
            start: 9.,
            end: f64::INFINITY,
        },
    ]);
    // The adjacent 15..20 range is intentionally not merged.
    assert_eq!(cache_ahead(&state), Some(5.));
    state.position = 22.;
    assert_eq!(cache_ahead(&state), Some(0.));
    state.position = f64::NAN;
    assert_eq!(cache_ahead(&state), None);
}
#[test]
fn time_is_safe_and_handles_hours() {
    assert_eq!(format_clock(f64::NAN), "0:00");
    assert_eq!(format_clock(-2.), "0:00");
    assert_eq!(format_clock(3661.9), "1:01:01");
}
#[test]
fn timeline_rejects_unknown_and_invalid_duration() {
    for duration in [
        None,
        Some(0.),
        Some(-1.),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ] {
        assert_eq!(progress(1., duration), None);
    }
    assert_eq!(progress(200., Some(100.)), Some(1.));
    assert_eq!(progress(-1., Some(100.)), Some(0.));
}
#[test]
fn disabled_controls_explain_themselves() {
    let a = Availability {
        backend: true,
        seekable: true,
        chapters: false,
        subtitles: false,
        audio: false,
        chapters_known: true,
        tracks_known: true,
    };
    assert_eq!(disabled_reason(PLAY, a), None);
    assert_eq!(
        disabled_reason(SUBTITLES, a),
        Some("No subtitle tracks in this media")
    );
    assert_eq!(
        disabled_reason(CHAPTERS, a),
        Some("No chapters in this media")
    );
    let unknown = Availability {
        chapters_known: false,
        tracks_known: false,
        ..a
    };
    assert_eq!(
        disabled_reason(SUBTITLES, unknown),
        Some("Subtitle tracks unavailable")
    );
    assert_eq!(
        disabled_reason(AUDIO, unknown),
        Some("Audio tracks unavailable")
    );
    assert_eq!(
        disabled_reason(CHAPTERS, unknown),
        Some("Chapter information unavailable")
    );
    let off = Availability {
        backend: false,
        ..a
    };
    assert_eq!(disabled_reason(AUDIO, off), Some("Player is not ready"));
    assert_eq!(disabled_reason(CLOSE, off), None);
}
#[test]
fn hides_after_approved_idle_delay() {
    let idle = |ms| controls_visible(false, false, false, false, Duration::from_millis(ms));
    assert!(idle(2400));
    assert!(!idle(2500));
}
#[test]
fn hides_only_while_playing_and_idle() {
    let idle = Duration::from_secs(4);
    assert!(!controls_visible(false, false, false, false, idle));
    assert!(controls_visible(true, false, false, false, idle));
    assert!(controls_visible(false, true, false, false, idle));
    assert!(controls_visible(false, false, true, false, idle));
    assert!(controls_visible(false, false, false, true, idle));
}
#[test]
fn idle_and_paused_ticks_do_not_redraw() {
    assert!(!redraw_needed(false, false, true, true));
    assert!(!redraw_needed(false, false, false, false));
    assert!(redraw_needed(false, false, true, false));
    assert!(redraw_needed(true, false, true, true));
    assert!(redraw_needed(false, true, false, false));
}
#[test]
fn disabled_controls_are_skipped() {
    let enabled = |availability| {
        (0..CONTROL_COUNT)
            .filter(|i| control_enabled(*i, availability))
            .collect::<Vec<_>>()
    };
    let none = Availability {
        backend: false,
        seekable: false,
        chapters: true,
        subtitles: true,
        audio: true,
        chapters_known: true,
        tracks_known: true,
    };
    assert_eq!(enabled(none), vec![CLOSE, FULLSCREEN, HELP]);
    let live = Availability {
        backend: true,
        ..none
    };
    assert_eq!(
        enabled(live),
        vec![
            CLOSE, PLAY, SUBTITLES, AUDIO, SPEED, FULLSCREEN, MUTE, VOLUME, HELP
        ]
    );
    let seekable = Availability {
        seekable: true,
        ..live
    };
    assert_eq!(enabled(seekable), (0..CONTROL_COUNT).collect::<Vec<_>>());
    let bare = Availability {
        chapters: false,
        subtitles: false,
        audio: false,
        chapters_known: true,
        tracks_known: true,
        ..seekable
    };
    assert!(
        !control_enabled(CHAPTERS, bare)
            && !control_enabled(SUBTITLES, bare)
            && !control_enabled(AUDIO, bare)
    );
}
#[test]
fn recoverable_control_error_keeps_player_and_lease() {
    let mut state = PlaybackState {
        media_stopped: false,
        error: Some("queue error".into()),
        ..Default::default()
    };
    assert!(!terminal_failure(false, &state));
    state.media_stopped = true;
    assert!(terminal_failure(false, &state));
    state.error = None;
    state.ended = true;
    assert!(!terminal_failure(false, &state));
    assert!(replay_needed(&state));
    state.stopped = true;
    assert!(terminal_failure(false, &state));
    assert!(!replay_needed(&state));
}
