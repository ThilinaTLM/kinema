//! Pure Cinema Dock policies: pointer-to-value mapping, scrub display state,
//! chapter lookup and label formatting. No GPUI types, so they are unit-tested.
use std::time::{Duration, Instant};

use kinema_mpv::{Chapter, MAX_VOLUME, Track};

/// Volume change for the Up/Down keys.
pub const VOLUME_STEP: f64 = 5.;
/// A committed seek keeps the thumb at its target until mpv reports it or this
/// long passes, so the thumb does not snap back to the stale position.
const PENDING_SEEK_TTL: Duration = Duration::from_millis(1200);
const PENDING_SEEK_TOLERANCE: f64 = 2.;

/// Horizontal position within `[left, left + width]` as 0..=1; NaN/zero width → 0.
pub fn ratio_in_span(pos: f32, start: f32, len: f32) -> f32 {
    if !pos.is_finite() || !start.is_finite() || !len.is_finite() || len <= 0. {
        return 0.;
    }
    ((pos - start) / len).clamp(0., 1.)
}

/// Whole-percent volume for a vertical slider whose top is `MAX_VOLUME`.
pub fn volume_at(y: f32, top: f32, height: f32) -> f64 {
    (f64::from(1. - ratio_in_span(y, top, height)) * MAX_VOLUME).round()
}

/// Slider fill fraction for a volume in `0..=MAX_VOLUME`.
pub fn volume_fraction(volume: f64) -> f32 {
    if volume.is_finite() {
        (volume / MAX_VOLUME).clamp(0., 1.) as f32
    } else {
        0.
    }
}

pub fn step_volume(volume: f64, delta: f64) -> f64 {
    if volume.is_finite() {
        (volume + delta).clamp(0., MAX_VOLUME).round()
    } else {
        0.
    }
}

/// An in-progress or just-committed timeline gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scrub {
    /// Pointer is down; value is the ratio under the pointer.
    Dragging(f64),
    /// Released; waiting for the backend to reach `target` seconds.
    Pending { target: f64, since: Instant },
}

/// Thumb/fill seconds to display for the given gesture and backend position.
pub fn displayed_position(scrub: Option<Scrub>, position: f64, duration: f64, now: Instant) -> f64 {
    match scrub {
        Some(Scrub::Dragging(ratio)) => ratio.clamp(0., 1.) * duration,
        Some(Scrub::Pending { target, since })
            if now.duration_since(since) < PENDING_SEEK_TTL
                && (position - target).abs() > PENDING_SEEK_TOLERANCE =>
        {
            target
        }
        _ => position,
    }
}

/// A pending seek is finished once it times out or the backend arrives.
pub fn scrub_settled(scrub: Option<Scrub>, position: f64, now: Instant) -> bool {
    match scrub {
        Some(Scrub::Pending { target, since }) => {
            now.duration_since(since) >= PENDING_SEEK_TTL
                || (position - target).abs() <= PENDING_SEEK_TOLERANCE
        }
        _ => false,
    }
}

/// Index of the chapter containing `time`; chapters must be sorted by start.
pub fn chapter_index(chapters: &[Chapter], time: f64) -> Option<usize> {
    chapters.iter().rposition(|chapter| time >= chapter.time)
}

/// Interior chapter boundaries as ratios, skipping the start and out-of-range marks.
pub fn chapter_ticks(chapters: &[Chapter], duration: f64) -> Vec<f32> {
    if !(duration.is_finite() && duration > 0.) {
        return Vec::new();
    }
    chapters
        .iter()
        .filter(|c| c.time.is_finite() && c.time > 0. && c.time < duration)
        .map(|c| (c.time / duration) as f32)
        .collect()
}

/// Left edge for a floating label of `width` centred on `ratio`, kept inside the track.
pub fn bubble_left(ratio: f32, track: f32, width: f32) -> f32 {
    let center = ratio.clamp(0., 1.) * track;
    center.clamp(width / 2., (track - width / 2.).max(width / 2.)) - width / 2.
}

/// Lowest `sub-pos` the dock may request; keeps subtitles on screen on short windows.
const SUBTITLE_POSITION_FLOOR: u8 = 40;

/// mpv `sub-pos` (100 = default bottom) that keeps subtitles above the chrome.
/// `occupied` is the bottom band covered by the dock including its margin and a
/// gap, `viewport` the full render height; both in the same unit. mpv positions
/// subtitles against the whole render surface, so video letterboxing does not
/// change the result. Hidden chrome (or unusable geometry) restores 100.
pub fn subtitle_position(chrome_visible: bool, occupied: f32, viewport: f32) -> u8 {
    if !chrome_visible || !occupied.is_finite() || !viewport.is_finite() || viewport <= 0. {
        return 100;
    }
    let covered = (occupied.max(0.) / viewport * 100.).ceil().min(100.) as u8;
    (100 - covered).max(SUBTITLE_POSITION_FLOOR)
}

/// Volume to show while a requested change has not yet been reported back.
pub fn displayed_volume(requested: Option<(f64, Instant)>, actual: f64, now: Instant) -> f64 {
    match requested {
        Some((volume, _)) if !volume_settled(requested, actual, now) => volume,
        _ => actual,
    }
}

pub fn volume_settled(requested: Option<(f64, Instant)>, actual: f64, now: Instant) -> bool {
    requested.is_some_and(|(volume, since)| {
        now.duration_since(since) >= PENDING_SEEK_TTL || (actual - volume).abs() < 0.5
    })
}

pub fn chapter_label(index: usize, chapter: &Chapter) -> String {
    match chapter.title.as_deref().map(str::trim) {
        Some(title) if !title.is_empty() => title.to_owned(),
        _ => format!("Chapter {}", index + 1),
    }
}

pub fn track_label(track: &Track) -> String {
    let title = track
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let lang = track
        .lang
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty());
    match (title, lang) {
        (Some(title), Some(lang)) => format!("{title} · {}", lang.to_uppercase()),
        (Some(label), None) => label.to_owned(),
        (None, Some(lang)) => lang.to_uppercase(),
        (None, None) => format!("Track {}", track.id),
    }
}

pub fn speed_label(speed: f64) -> String {
    format!("{}×", (speed * 100.).round() / 100.)
}

pub fn format_clock(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.).floor() as u64
    } else {
        0
    };
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

pub fn format_remaining(position: f64, duration: f64) -> String {
    format!("−{}", format_clock((duration - position).max(0.)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    fn chapters() -> Vec<Chapter> {
        [0., 100., 300.]
            .into_iter()
            .map(|time| Chapter { time, title: None })
            .collect()
    }

    #[test]
    fn pointer_mapping_clamps_and_rejects_bad_geometry() {
        assert_eq!(ratio_in_span(-5., 0., 100.), 0.);
        assert_eq!(ratio_in_span(500., 0., 100.), 1.);
        assert_eq!(ratio_in_span(25., 0., 100.), 0.25);
        assert_eq!(ratio_in_span(25., 0., 0.), 0.);
        assert_eq!(ratio_in_span(f32::NAN, 0., 10.), 0.);
    }
    #[test]
    fn vertical_volume_is_top_down_and_whole_percent() {
        assert_eq!(volume_at(0., 0., 150.), 150.);
        assert_eq!(volume_at(150., 0., 150.), 0.);
        assert_eq!(volume_at(50., 0., 150.), 100.);
        assert_eq!(volume_at(-40., 0., 150.), 150.);
        assert_eq!(volume_fraction(150.), 1.);
        assert_eq!(volume_fraction(75.), 0.5);
        assert_eq!(volume_fraction(f64::NAN), 0.);
    }
    #[test]
    fn volume_steps_stay_in_range() {
        assert_eq!(step_volume(98., VOLUME_STEP), 103.);
        assert_eq!(step_volume(148., VOLUME_STEP), 150.);
        assert_eq!(step_volume(3., -VOLUME_STEP), 0.);
        assert_eq!(step_volume(f64::NAN, VOLUME_STEP), 0.);
    }
    #[test]
    fn scrub_display_and_settling() {
        let t = Instant::now();
        assert_eq!(displayed_position(None, 10., 100., t), 10.);
        assert_eq!(
            displayed_position(Some(Scrub::Dragging(0.5)), 10., 100., t),
            50.
        );
        let pending = Some(Scrub::Pending {
            target: 50.,
            since: t,
        });
        assert_eq!(displayed_position(pending, 10., 100., t), 50.);
        assert_eq!(displayed_position(pending, 49., 100., t), 49.);
        assert!(scrub_settled(pending, 49., t));
        assert!(!scrub_settled(pending, 10., t));
        assert!(scrub_settled(pending, 10., t + Duration::from_secs(2)));
        assert_eq!(
            displayed_position(pending, 10., 100., t + Duration::from_secs(2)),
            10.
        );
    }
    #[test]
    fn chapter_lookup_and_ticks() {
        let c = chapters();
        assert_eq!(chapter_index(&c, 0.), Some(0));
        assert_eq!(chapter_index(&c, 299.9), Some(1));
        assert_eq!(chapter_index(&c, 300.), Some(2));
        assert_eq!(chapter_index(&[], 5.), None);
        assert_eq!(chapter_ticks(&c, 600.), vec![1. / 6., 0.5]);
        assert!(chapter_ticks(&c, 0.).is_empty());
        assert_eq!(chapter_ticks(&c, 200.), vec![0.5]);
    }
    #[test]
    fn bubble_stays_inside_track() {
        assert_eq!(bubble_left(0., 400., 100.), 0.);
        assert_eq!(bubble_left(1., 400., 100.), 300.);
        assert_eq!(bubble_left(0.5, 400., 100.), 150.);
        assert_eq!(bubble_left(0.5, 50., 100.), 0.);
    }
    #[test]
    fn clock_formats() {
        assert_eq!(format_clock(3661.), "1:01:01");
        assert_eq!(format_remaining(90., 100.), "−0:10");
        assert_eq!(format_remaining(120., 100.), "−0:00");
    }

    #[test]
    fn labels_fall_back_to_real_metadata() {
        use kinema_mpv::TrackKind;
        let track = |title: Option<&str>, lang: Option<&str>| Track {
            id: 7,
            kind: TrackKind::Audio,
            title: title.map(Into::into),
            lang: lang.map(Into::into),
            selected: false,
        };
        assert_eq!(
            track_label(&track(Some("Stereo"), Some("en"))),
            "Stereo · EN"
        );
        assert_eq!(track_label(&track(None, Some("fr"))), "FR");
        assert_eq!(track_label(&track(Some(" "), None)), "Track 7");
        let chapter = Chapter {
            time: 0.,
            title: Some("".into()),
        };
        assert_eq!(chapter_label(2, &chapter), "Chapter 3");
        assert_eq!(speed_label(1.), "1×");
        assert_eq!(speed_label(1.25), "1.25×");
    }
    #[test]
    fn requested_volume_shows_until_reported_or_expired() {
        let t = Instant::now();
        let req = Some((30., t));
        assert_eq!(displayed_volume(req, 80., t), 30.);
        assert_eq!(displayed_volume(req, 30., t), 30.);
        assert_eq!(displayed_volume(req, 80., t + Duration::from_secs(2)), 80.);
        assert_eq!(displayed_volume(None, 80., t), 80.);
    }

    #[test]
    fn subtitles_rise_above_visible_chrome_only() {
        assert_eq!(subtitle_position(false, 150., 600.), 100);
        assert_eq!(subtitle_position(true, 150., 600.), 75);
        assert_eq!(subtitle_position(true, 176., 440.), 60);
        assert_eq!(subtitle_position(true, 150., 1080.), 86);
        // Short windows never push subtitles past the floor; bad geometry restores.
        assert_eq!(subtitle_position(true, 400., 440.), 40);
        assert_eq!(subtitle_position(true, 150., 0.), 100);
        assert_eq!(subtitle_position(true, f32::NAN, 600.), 100);
    }
}

/// Next strictly adjacent speed preset, including intermediate observations.
pub fn adjacent_speed(speed: f64, presets: &[f64], forward: bool) -> Option<f64> {
    if !speed.is_finite() {
        return None;
    }
    if forward {
        presets.iter().copied().find(|v| *v > speed)
    } else {
        presets.iter().rev().copied().find(|v| *v < speed)
    }
}

pub fn adjacent_chapter(chapters: &[Chapter], position: f64, forward: bool) -> Option<f64> {
    if !position.is_finite() {
        return None;
    }
    let current = chapter_index(chapters, position);
    let next = if forward {
        current.map_or(Some(0), |i| i.checked_add(1))
    } else {
        current.and_then(|i| i.checked_sub(1))
    }?;
    chapters
        .get(next)
        .map(|c| c.time)
        .filter(|time| time.is_finite() && *time >= 0.)
}

/// Outer None means unavailable, inner None is the explicit subtitle Off slot.
pub fn cycle_track(
    tracks: &[Track],
    kind: kinema_mpv::TrackKind,
    forward: bool,
) -> Option<Option<i64>> {
    let filtered: Vec<_> = tracks.iter().filter(|t| t.kind == kind).collect();
    if filtered.is_empty() {
        return None;
    }
    let off = kind == kinema_mpv::TrackKind::Subtitle;
    let count = filtered.len() + usize::from(off);
    let selected = filtered.iter().position(|t| t.selected);
    let current = selected.or(if off { Some(filtered.len()) } else { None });
    let index = match (current, forward) {
        (Some(i), true) => (i + 1) % count,
        (Some(i), false) => (i + count - 1) % count,
        (None, true) => 0,
        (None, false) => count - 1,
    };
    Some(filtered.get(index).map(|t| t.id))
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;
    use kinema_mpv::TrackKind;
    #[test]
    fn speed_and_chapter_edges_do_not_wrap() {
        let speeds = [0.5, 0.75, 1., 1.25, 1.5, 1.75, 2.];
        assert_eq!(adjacent_speed(1.1, &speeds, true), Some(1.25));
        assert_eq!(adjacent_speed(1.1, &speeds, false), Some(1.));
        assert_eq!(adjacent_speed(2., &speeds, true), None);
        assert_eq!(adjacent_speed(0.5, &speeds, false), None);
        let chapters = [10., 50., 90.].map(|time| Chapter { time, title: None });
        assert_eq!(adjacent_chapter(&chapters, 0., true), Some(10.));
        assert_eq!(adjacent_chapter(&chapters, 0., false), None);
        assert_eq!(adjacent_chapter(&chapters, 51., false), Some(10.));
        assert_eq!(adjacent_chapter(&chapters, 90., true), None);
        assert_eq!(adjacent_chapter(&[], 0., true), None);
    }
    #[test]
    fn cycling_uses_ids_and_distinct_off_slot() {
        let track = |id, kind, selected| Track {
            id,
            kind,
            selected,
            title: None,
            lang: None,
        };
        let tracks = [
            track(42, TrackKind::Subtitle, true),
            track(3, TrackKind::Audio, true),
            track(91, TrackKind::Subtitle, false),
        ];
        assert_eq!(
            cycle_track(&tracks, TrackKind::Subtitle, true),
            Some(Some(91))
        );
        assert_eq!(cycle_track(&tracks, TrackKind::Subtitle, false), Some(None));
        assert_eq!(cycle_track(&tracks, TrackKind::Audio, true), Some(Some(3)));
        assert_eq!(cycle_track(&[], TrackKind::Audio, true), None);
        let off = [track(91, TrackKind::Subtitle, false)];
        assert_eq!(cycle_track(&off, TrackKind::Subtitle, true), Some(Some(91)));
    }
}
