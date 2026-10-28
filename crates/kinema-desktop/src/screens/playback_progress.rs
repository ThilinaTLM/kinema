//! Player-side protocol only. The app coordinator owns persistence and retries.
use kinema_core::watch_progress::{PlaybackObservation, PlaybackProgressTarget};
use kinema_mpv::PlaybackState;
use std::time::{Duration, Instant};

pub(crate) use super::super::coordinator::PlaybackIntent;

/// Prepared off-thread by the application before loading a source.
#[derive(Clone, Debug)]
pub struct PlaybackLaunch {
    pub subject: PlaybackProgressTarget,
    pub epoch: i64,
    pub run: i64,
    pub checkpoint: Option<PlaybackObservation>,
    pub watched: bool,
    pub intent: PlaybackIntent,
}
#[derive(Clone, Debug)]
pub struct PlayerCheckpoint {
    pub generation: u64,
    pub subject: PlaybackProgressTarget,
    pub epoch: i64,
    pub run: i64,
    pub sequence: u64,
    pub observation: PlaybackObservation,
    pub force: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Phase {
    Loading,
    AwaitSeekability,
    Choice,
    Mismatch,
    Unavailable,
    Seeking {
        target: f64,
        position_revision: u64,
        restart_revision: u64,
    },
    Starting {
        position_revision: u64,
        restart_revision: u64,
    },
    Reset,
    Ready,
    Active,
}
pub(super) struct Progress {
    pub launch: PlaybackLaunch,
    pub phase: Phase,
    pub authorized: bool,
    pub notice: bool,
    pub duration_accepted: bool,
    pub generation: Option<u64>,
    pub play: bool,
    pub seek_started: Option<Instant>,
    wait_started: Instant,
    pub last: Option<PlaybackObservation>,
    sequence: u64,
    last_sent: Option<Instant>,
    last_position_revision: u64,
    paused: bool,
    restart_revision: u64,
    ended: bool,
}
impl Progress {
    pub fn new(launch: PlaybackLaunch) -> Self {
        Self {
            launch,
            phase: Phase::Loading,
            authorized: false,
            notice: false,
            duration_accepted: false,
            generation: None,
            play: true,
            seek_started: None,
            wait_started: Instant::now(),
            last: None,
            sequence: 0,
            last_sent: None,
            last_position_revision: 0,
            paused: true,
            restart_revision: 0,
            ended: false,
        }
    }
    /// Includes pre-file-loaded and missing-property waits. A user prompt never
    /// consumes this budget; an explicit retry starts a new bounded attempt.
    pub fn check_timeout(&mut self, now: Instant) -> bool {
        let since = match self.phase {
            Phase::Loading | Phase::AwaitSeekability => Some(self.wait_started),
            Phase::Seeking { .. } | Phase::Starting { .. } => self.seek_started,
            _ => None,
        };
        if since.is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_secs(10)) {
            self.phase = Phase::Unavailable;
            self.authorized = false;
            self.seek_started = None;
            return true;
        }
        false
    }
    pub fn retry_startup(&mut self, now: Instant) {
        self.phase = Phase::Loading;
        self.wait_started = now;
        self.seek_started = None;
        self.authorized = false;
        self.last = None;
        self.duration_accepted = false;
    }
    pub fn await_seekability(&mut self, now: Instant) {
        if !matches!(self.phase, Phase::Loading | Phase::AwaitSeekability) {
            self.wait_started = now;
        }
        self.phase = Phase::AwaitSeekability;
    }
    pub fn can_start_over(&self, state: &PlaybackState) -> bool {
        self.generation == Some(state.media_generation)
            && state.file_loaded
            && !state.loading
            && !state.media_stopped
            && !state.seeking
            && !state.stopped
            && state.error.is_none()
            && state.failure_reason.is_none()
            && state.position_observed
            && state.position.is_finite()
            && state.position >= 0.0
            && (state.position <= 1.0 || state.seekable == Some(true))
    }
    pub fn usable_checkpoint(&self) -> Option<PlaybackObservation> {
        self.launch.checkpoint.filter(|v| v.position_ms() > 0)
    }
    pub fn loaded(&mut self, state: &PlaybackState) -> bool {
        if !state.file_loaded
            || state.stopped
            || state.loading
            || state.media_stopped
            || state.error.is_some()
            || state.failure_reason.is_some()
        {
            return false;
        }
        match self.generation {
            None => {
                self.generation = Some(state.media_generation);
                true
            }
            Some(generation) => generation == state.media_generation,
        }
    }
    pub fn sample(
        &mut self,
        state: &PlaybackState,
        now: Instant,
        force: bool,
    ) -> Option<PlayerCheckpoint> {
        // An EOF snapshot may retain the final observed position. Never fabricate duration/EOF position.
        let valid = self.phase == Phase::Active
            && self.authorized
            && self.generation == Some(state.media_generation)
            && state.position_observed
            && state.file_loaded
            && !state.stopped
            && !state.loading
            && !state.seeking
            && (!state.media_stopped || state.ended)
            && state.error.is_none()
            && state.failure_reason.is_none();
        if valid && state.position_revision > self.last_position_revision {
            // A finite sliding/live duration is not sufficient completion evidence.
            // Without an explicit live-source classifier, require observed
            // seekability; finite unseekable movies conservatively cannot auto-mark.
            let duration = state.duration.filter(|_| state.seekable == Some(true));
            self.last = PlaybackObservation::from_seconds(state.position, duration);
            self.last_position_revision = state.position_revision;
        }
        if state.seekable != Some(true) {
            self.last = self
                .last
                .and_then(|last| PlaybackObservation::new(last.position_ms(), None));
        }
        let transition = (state.paused && !self.paused)
            || state.restart_revision != self.restart_revision
            || (state.ended && !self.ended);
        self.paused = state.paused;
        self.restart_revision = state.restart_revision;
        self.ended = state.ended;
        if !valid
            || !(force
                || transition
                || self
                    .last_sent
                    .is_none_or(|t| now.duration_since(t) >= Duration::from_secs(5)))
        {
            return None;
        }
        let observation = self.last?;
        self.sequence += 1;
        self.last_sent = Some(now);
        Some(PlayerCheckpoint {
            generation: 0, // Filled by the lease-owning window at delivery.
            subject: self.launch.subject,
            epoch: self.launch.epoch,
            run: self.launch.run,
            sequence: self.sequence,
            observation,
            force: force || transition,
        })
    }
}

pub(super) fn seek_confirmed(
    state: &PlaybackState,
    target: f64,
    position_revision: u64,
    restart_revision: u64,
) -> bool {
    state.file_loaded
        && state.position_observed
        && !state.loading
        && !state.media_stopped
        && !state.seeking
        && state.error.is_none()
        && state.failure_reason.is_none()
        && state.position_revision > position_revision
        && state.restart_revision > restart_revision
        && (state.position - target).abs() <= 1.0
}

pub(super) fn duration_mismatch(saved: PlaybackObservation, duration: Option<f64>) -> bool {
    saved.duration_ms().zip(duration).is_some_and(|(old, new)| {
        new.is_finite()
            && new > 0.0
            && (new - old as f64 / 1000.0).abs() > 30.0_f64.max(old as f64 / 1000.0 * 0.02)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinema_core::library::TitleId;
    fn progress() -> Result<Progress, Box<dyn std::error::Error>> {
        let mut p = Progress::new(PlaybackLaunch {
            subject: PlaybackProgressTarget::Movie(
                TitleId::new(1).ok_or("expected valid fixture or checkpoint")?,
            ),
            epoch: 3,
            run: 2,
            checkpoint: None,
            watched: false,
            intent: PlaybackIntent::Normal,
        });
        p.authorized = true;
        p.phase = Phase::Active;
        p.generation = Some(1);
        Ok(p)
    }
    fn state(position: f64, revision: u64) -> PlaybackState {
        PlaybackState {
            position,
            position_revision: revision,
            media_generation: 1,
            file_loaded: true,
            position_observed: true,
            media_stopped: false,
            ..Default::default()
        }
    }
    #[test]
    fn observed_backward_position_and_sequence() -> Result<(), Box<dyn std::error::Error>> {
        let mut p = progress()?;
        let now = Instant::now();
        assert_eq!(
            p.sample(&state(100.0, 1), now, false)
                .ok_or("expected valid fixture or checkpoint")?
                .sequence,
            1
        );
        assert!(p.sample(&state(50.0, 2), now, false).is_none());
        let sample = p
            .sample(&state(50.0, 2), now, true)
            .ok_or("expected valid fixture or checkpoint")?;
        assert_eq!(sample.observation.position_ms(), 50_000);
        assert_eq!(sample.sequence, 2);
        Ok(())
    }
    #[test]
    fn loading_stale_and_revoked_never_save() -> Result<(), Box<dyn std::error::Error>> {
        let mut p = progress()?;
        let now = Instant::now();
        let mut s = state(90.0, 1);
        s.loading = true;
        assert!(p.sample(&s, now, true).is_none());
        s.loading = false;
        s.media_generation = 2;
        assert!(p.sample(&s, now, true).is_none());
        s.media_generation = 1;
        p.authorized = false;
        assert!(p.sample(&s, now, true).is_none());
        Ok(())
    }
    #[test]
    fn pause_seek_and_end_flush_inside_interval() -> Result<(), Box<dyn std::error::Error>> {
        let mut p = progress()?;
        let now = Instant::now();
        let mut s = state(10.0, 1);
        p.sample(&s, now, false)
            .ok_or("expected valid fixture or checkpoint")?;
        s.paused = true;
        assert!(
            p.sample(&s, now, false)
                .ok_or("expected valid fixture or checkpoint")?
                .force
        );
        s.position = 5.0;
        s.position_revision += 1;
        s.restart_revision += 1;
        let seek = p
            .sample(&s, now, false)
            .ok_or("expected valid fixture or checkpoint")?;
        assert_eq!(seek.observation.position_ms(), 5000);
        assert!(seek.force);
        s.ended = true;
        s.media_stopped = true;
        assert_eq!(
            p.sample(&s, now, false)
                .ok_or("expected valid fixture or checkpoint")?
                .observation
                .position_ms(),
            5000
        );
        Ok(())
    }
    #[test]
    fn no_default_or_speculative_position_and_unknown_duration_is_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut p = progress()?;
        let now = Instant::now();
        let mut s = state(0.0, 0);
        s.position_observed = false;
        assert!(p.sample(&s, now, true).is_none());
        s.position = 900.0;
        s.position_revision = 1;
        s.position_observed = true;
        s.seeking = true;
        assert!(p.sample(&s, now, true).is_none());
        s.seeking = false;
        let checkpoint = p
            .sample(&s, now, true)
            .ok_or("expected valid fixture or checkpoint")?;
        assert_eq!(checkpoint.observation.duration_ms(), None);
        assert_eq!(checkpoint.observation.position_ms(), 900_000);
        Ok(())
    }
    #[test]
    fn read_only_launch_needs_explicit_authorization() -> Result<(), Box<dyn std::error::Error>> {
        let mut p = progress()?;
        p.authorized = false;
        assert!(p.sample(&state(0.0, 1), Instant::now(), true).is_none());
        p.authorized = true;
        assert_eq!(
            p.sample(&state(0.0, 1), Instant::now(), true)
                .ok_or("expected valid fixture or checkpoint")?
                .observation
                .position_ms(),
            0
        );
        Ok(())
    }
    #[test]
    fn seek_requires_fresh_position_and_restart_not_command_acceptance()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut s = state(20.0, 3);
        assert!(!seek_confirmed(&s, 20.0, 3, 0));
        s.position_revision += 1;
        assert!(!seek_confirmed(&s, 20.0, 3, 0));
        s.restart_revision += 1;
        assert!(seek_confirmed(&s, 20.0, 3, 0));
        assert!(!seek_confirmed(&s, 50.0, 3, 0));
        s.loading = true;
        assert!(!seek_confirmed(&s, 20.0, 3, 0));
        Ok(())
    }
    #[test]
    fn startup_deadline_covers_loading_and_missing_position()
    -> Result<(), Box<dyn std::error::Error>> {
        for file_loaded in [false, true] {
            let mut p = progress()?;
            let now = Instant::now();
            p.retry_startup(now);
            let mut s = state(0.0, 0);
            s.file_loaded = file_loaded;
            s.position_observed = false;
            p.loaded(&s);
            assert!(!p.check_timeout(now + Duration::from_secs(9)));
            assert!(p.check_timeout(now + Duration::from_secs(10)));
            assert_eq!(p.phase, Phase::Unavailable);
            assert!(!p.can_start_over(&s));
            assert!(p.sample(&s, now + Duration::from_secs(10), true).is_none());
        }
        Ok(())
    }
    #[test]
    fn seek_deadlines_do_not_depend_on_remaining_loaded() -> Result<(), Box<dyn std::error::Error>>
    {
        let now = Instant::now();
        for phase in [
            Phase::Seeking {
                target: 20.0,
                position_revision: 1,
                restart_revision: 1,
            },
            Phase::Starting {
                position_revision: 1,
                restart_revision: 1,
            },
        ] {
            let mut p = progress()?;
            p.phase = phase;
            p.seek_started = Some(now);
            let mut s = state(0.0, 1);
            s.loading = true;
            assert!(!p.loaded(&s));
            assert!(p.check_timeout(now + Duration::from_secs(10)));
            assert_eq!(p.phase, Phase::Unavailable);
            assert!(!p.authorized);
        }
        Ok(())
    }
    #[test]
    fn unknown_seekability_has_a_deadline_and_retry_gets_a_fresh_budget()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut p = progress()?;
        let now = Instant::now();
        p.retry_startup(now);
        p.await_seekability(now + Duration::from_secs(9));
        assert!(p.check_timeout(now + Duration::from_secs(10)));
        p.retry_startup(now + Duration::from_secs(11));
        assert!(!p.check_timeout(now + Duration::from_secs(20)));
        assert!(p.check_timeout(now + Duration::from_secs(21)));
        // Waiting for a user choice is not a startup timeout.
        p.phase = Phase::Choice;
        assert!(!p.check_timeout(now + Duration::from_secs(200)));
        p.await_seekability(now + Duration::from_secs(200));
        assert!(!p.check_timeout(now + Duration::from_secs(209)));
        assert!(p.check_timeout(now + Duration::from_secs(210)));
        Ok(())
    }
    #[test]
    fn start_over_requires_real_position_and_seekability_when_repositioning()
    -> Result<(), Box<dyn std::error::Error>> {
        let p = progress()?;
        let mut s = state(100.0, 1);
        assert!(!p.can_start_over(&s));
        s.seekable = Some(true);
        assert!(p.can_start_over(&s));
        s.position_observed = false;
        assert!(!p.can_start_over(&s));
        s.position_observed = true;
        s.position = 0.0;
        s.seekable = None;
        assert!(p.can_start_over(&s));
        s.media_generation = 2;
        assert!(!p.can_start_over(&s));
        Ok(())
    }
    #[test]
    fn unknown_live_durations_and_error_snapshots_cannot_auto_mark()
    -> Result<(), Box<dyn std::error::Error>> {
        let now = Instant::now();
        for duration in [
            None,
            Some(0.0),
            Some(-1.0),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            let mut p = progress()?;
            let mut s = state(10_000.0, 1);
            s.duration = duration;
            let observation = p
                .sample(&s, now, true)
                .ok_or("expected valid fixture or checkpoint")?
                .observation;
            assert_eq!(observation.duration_ms(), None);
            assert!(!observation.qualifies_watched(false));
        }
        let mut p = progress()?;
        let mut s = state(99.0, 1);
        s.duration = Some(100.0);
        s.error = Some("private backend error".into());
        assert!(p.sample(&s, now, true).is_none());
        s.error = None;
        s.stopped = true;
        assert!(p.sample(&s, now, true).is_none());
        s.stopped = false;
        p.authorized = false; // Explicit Play without saving never regains authority from observations.
        assert!(p.sample(&s, now, true).is_none());
        Ok(())
    }
    #[test]
    fn finite_duration_needs_observed_seekability_for_completion()
    -> Result<(), Box<dyn std::error::Error>> {
        let now = Instant::now();
        for seekable in [None, Some(false), Some(true)] {
            let mut p = progress()?;
            let mut s = state(95.0, 1);
            s.duration = Some(100.0);
            s.seekable = seekable;
            let observation = p
                .sample(&s, now, true)
                .ok_or("expected valid fixture or checkpoint")?
                .observation;
            assert_eq!(observation.position_ms(), 95_000);
            assert_eq!(observation.qualifies_watched(false), seekable == Some(true));
        }
        // A forced pause/close may use a cached confirmed position. Revoking
        // seekability must also revoke its cached completion evidence.
        let mut p = progress()?;
        let mut s = state(95.0, 1);
        s.duration = Some(100.0);
        s.seekable = Some(true);
        assert!(
            p.sample(&s, now, true)
                .ok_or("expected valid fixture or checkpoint")?
                .observation
                .qualifies_watched(false)
        );
        s.seekable = Some(false);
        let observation = p
            .sample(&s, now, true)
            .ok_or("expected valid fixture or checkpoint")?
            .observation;
        assert_eq!(observation.duration_ms(), None);
        assert!(!observation.qualifies_watched(false));
        Ok(())
    }
    #[test]
    fn mismatch_uses_absolute_or_relative_tolerance() -> Result<(), Box<dyn std::error::Error>> {
        let saved = PlaybackObservation::new(1000, Some(100_000))
            .ok_or("expected valid fixture or checkpoint")?;
        assert!(!duration_mismatch(saved, Some(130.0)));
        assert!(duration_mismatch(saved, Some(131.0)));
        assert!(!duration_mismatch(saved, None));
        Ok(())
    }
}
