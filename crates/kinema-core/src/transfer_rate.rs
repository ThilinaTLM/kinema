//! Pure transfer-speed and summary policy for the toolbar status button.
//!
//! Time is supplied by the caller as monotonic milliseconds so the math is
//! deterministic; sampling happens off the render path.

use std::collections::HashMap;

/// Samples closer together than this are folded into the next window.
const MIN_WINDOW_MS: u64 = 900;
/// A session that has not reported for this long no longer contributes speed.
const STALE_AFTER_MS: u64 = 3_500;
/// Weight of the newest window in the smoothed rate.
const SMOOTHING: f64 = 0.4;

#[derive(Clone, Copy, Debug)]
struct Track {
    at_ms: u64,
    bytes: u64,
    /// `None` until one full window has been measured.
    rate: Option<f64>,
}

/// Smoothed bytes-per-second per session, summed for the toolbar.
#[derive(Clone, Debug, Default)]
pub struct RateTracker {
    tracks: HashMap<String, Track>,
}

impl RateTracker {
    /// Record the cumulative downloaded bytes for a session at `now_ms`.
    pub fn observe(&mut self, id: &str, downloaded_bytes: u64, now_ms: u64) {
        let Some(track) = self.tracks.get_mut(id) else {
            self.tracks.insert(
                id.to_owned(),
                Track {
                    at_ms: now_ms,
                    bytes: downloaded_bytes,
                    rate: None,
                },
            );
            return;
        };
        if downloaded_bytes < track.bytes || now_ms < track.at_ms {
            // Counter reset (cache eviction, relaunch): restart the measurement.
            *track = Track {
                at_ms: now_ms,
                bytes: downloaded_bytes,
                rate: None,
            };
            return;
        }
        let elapsed = now_ms - track.at_ms;
        if elapsed < MIN_WINDOW_MS {
            return;
        }
        let instant = (downloaded_bytes - track.bytes) as f64 * 1000. / elapsed as f64;
        track.rate = Some(match track.rate {
            Some(previous) => previous + SMOOTHING * (instant - previous),
            None => instant,
        });
        track.at_ms = now_ms;
        track.bytes = downloaded_bytes;
    }

    /// Keep only sessions for which `keep` returns true.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.tracks.retain(|id, _| keep(id));
    }

    /// Combined smoothed speed in bytes per second at `now_ms`.
    pub fn total_bytes_per_second(&self, now_ms: u64) -> u64 {
        let total: f64 = self
            .tracks
            .values()
            .filter(|t| now_ms.saturating_sub(t.at_ms) <= STALE_AFTER_MS)
            .filter_map(|t| t.rate)
            .sum();
        // Below 1 B/s is noise from decay; present it as idle.
        if total < 1. { 0 } else { total as u64 }
    }
}

/// What the Transfers button shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferSummary {
    Idle,
    Serving {
        count: usize,
    },
    Downloading {
        active: usize,
        downloaded: u64,
        total: u64,
    },
}

/// One session as seen by the summary: only incomplete online downloads count
/// as "active"; everything else that is held open is "serving".
#[derive(Clone, Copy, Debug)]
pub struct SessionTally {
    pub remote: bool,
    pub progress: Option<(u64, u64)>,
}

impl TransferSummary {
    pub fn from_sessions(sessions: impl IntoIterator<Item = SessionTally>) -> Self {
        let (mut serving, mut active, mut downloaded, mut total) = (0usize, 0usize, 0u64, 0u64);
        for session in sessions {
            match session.progress {
                Some((done, size)) if session.remote && size > 0 && done < size => {
                    active += 1;
                    downloaded = downloaded.saturating_add(done);
                    total = total.saturating_add(size);
                }
                _ => serving += 1,
            }
        }
        if active > 0 {
            Self::Downloading {
                active,
                downloaded,
                total,
            }
        } else if serving > 0 {
            Self::Serving { count: serving }
        } else {
            Self::Idle
        }
    }

    /// Combined 0..=1 progress for the bottom bar.
    pub fn fraction(&self) -> Option<f32> {
        match *self {
            Self::Downloading {
                downloaded, total, ..
            } if total > 0 => Some((downloaded as f64 / total as f64).clamp(0., 1.) as f32),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_observation_has_no_rate_then_measures_a_window() {
        let mut t = RateTracker::default();
        t.observe("a", 0, 0);
        assert_eq!(t.total_bytes_per_second(500), 0);
        t.observe("a", 1_000_000, 1_000);
        assert_eq!(t.total_bytes_per_second(1_000), 1_000_000);
    }

    #[test]
    fn smoothing_blends_and_sub_window_samples_accumulate() {
        let mut t = RateTracker::default();
        t.observe("a", 0, 0);
        t.observe("a", 1_000, 1_000);
        t.observe("a", 1_500, 1_300); // too soon: folded into next window
        assert_eq!(t.total_bytes_per_second(1_300), 1_000);
        t.observe("a", 3_000, 2_000); // 2000 B over 1 s
        let rate = t.total_bytes_per_second(2_000);
        assert_eq!(rate, 1_400); // 1000 + 0.4 * (2000 - 1000)
    }

    #[test]
    fn sums_sessions_and_drops_stale_or_forgotten_ones() {
        let mut t = RateTracker::default();
        for (id, per_s) in [("a", 100u64), ("b", 300)] {
            t.observe(id, 0, 0);
            t.observe(id, per_s, 1_000);
        }
        assert_eq!(t.total_bytes_per_second(1_000), 400);
        assert_eq!(t.total_bytes_per_second(1_000 + STALE_AFTER_MS + 1), 0);
        t.retain(|id| id == "a");
        assert_eq!(t.total_bytes_per_second(1_000), 100);
    }

    #[test]
    fn counter_reset_restarts_without_negative_speed() {
        let mut t = RateTracker::default();
        t.observe("a", 5_000, 0);
        t.observe("a", 6_000, 1_000);
        t.observe("a", 10, 2_000);
        assert_eq!(t.total_bytes_per_second(2_000), 0);
        t.observe("a", 2_010, 3_000);
        assert_eq!(t.total_bytes_per_second(3_000), 2_000);
    }

    fn tally(remote: bool, progress: Option<(u64, u64)>) -> SessionTally {
        SessionTally { remote, progress }
    }

    #[test]
    fn summary_distinguishes_idle_serving_and_downloading() {
        assert_eq!(TransferSummary::from_sessions([]), TransferSummary::Idle);
        let serving = TransferSummary::from_sessions([
            tally(false, None),
            tally(true, Some((10, 10))),
            tally(true, None),
        ]);
        assert_eq!(serving, TransferSummary::Serving { count: 3 });
        let busy = TransferSummary::from_sessions([
            tally(true, Some((10, 40))),
            tally(true, Some((30, 60))),
            tally(false, None),
        ]);
        assert_eq!(
            busy,
            TransferSummary::Downloading {
                active: 2,
                downloaded: 40,
                total: 100
            }
        );
        assert_eq!(busy.fraction(), Some(0.4));
        assert_eq!(serving.fraction(), None);
    }
}
