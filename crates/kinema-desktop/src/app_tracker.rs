//! Pure bookkeeping for application-worker commands: which command a result
//! answers, whether it is still current, and which playback sessions (and so the
//! local media server) are active. No I/O; the shell owns the worker.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Pending {
    /// Replaceable read; a failure only matters while no newer load is pending.
    Load,
    /// Settings save: a successful snapshot is always applied and failures shown.
    Settings,
    Credential,
    Prepare,
    Play {
        remote: bool,
    },
    Release,
    Replay,
    Reveal,
    Cleanup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Tracked {
    pub id: String,
    pub remote: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Tracker {
    next: u64,
    pending: HashMap<u64, Pending>,
    /// Session id for each pending release, so a failure can keep that lease.
    releases: HashMap<u64, String>,
    latest_prepare: u64,
    latest_play: u64,
    sessions: Vec<Tracked>,
}

/// Which command an event answers and whether the UI should still show it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Resolved {
    pub pending: Option<Pending>,
    /// For a release command: the session it targeted.
    pub release: Option<String>,
    /// False when a newer Prepare/Play superseded the command.
    pub current: bool,
}

impl Tracker {
    /// Registers a command and returns its generation (never 0, which the
    /// worker uses for unsolicited events).
    pub(crate) fn issue(&mut self, pending: Pending) -> u64 {
        self.next += 1;
        let generation = self.next;
        match pending {
            Pending::Prepare => self.latest_prepare = generation,
            Pending::Play { .. } => self.latest_play = generation,
            _ => {}
        }
        self.pending.insert(generation, pending);
        generation
    }

    pub(crate) fn issue_release(&mut self, id: String) -> u64 {
        self.issue_session(Pending::Release, id)
    }

    pub(crate) fn issue_session(&mut self, pending: Pending, id: String) -> u64 {
        let generation = self.issue(pending);
        self.releases.insert(generation, id);
        generation
    }

    pub(crate) fn cancel(&mut self, generation: u64) {
        if self.latest_prepare == generation {
            self.latest_prepare = 0;
        }
        if self.latest_play == generation {
            self.latest_play = 0;
        }
    }

    pub(crate) fn is_pending(&self, generation: u64) -> bool {
        self.pending.contains_key(&generation)
    }

    /// The worker refused the command (backpressure); it will never answer.
    pub(crate) fn withdraw(&mut self, generation: u64) {
        self.pending.remove(&generation);
        self.releases.remove(&generation);
    }

    /// Consumes the pending entry for a terminal event.
    pub(crate) fn resolve(&mut self, generation: u64) -> Resolved {
        let pending = self.pending.remove(&generation);
        let release = self.releases.remove(&generation);
        let current = match pending {
            Some(Pending::Prepare) => generation == self.latest_prepare,
            Some(Pending::Play { .. }) => generation == self.latest_play,
            Some(Pending::Load) => !self
                .pending
                .iter()
                .any(|(g, p)| *p == Pending::Load && *g > generation),
            _ => true,
        };
        Resolved {
            pending,
            release,
            current,
        }
    }

    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Sessions are tracked even for superseded results: the server is running.
    pub(crate) fn session_started(&mut self, id: String, remote: bool) {
        if !self.sessions.iter().any(|s| s.id == id) {
            self.sessions.push(Tracked { id, remote });
        }
    }

    pub(crate) fn session_released(&mut self, id: &str) {
        self.sessions.retain(|s| s.id != id);
    }

    pub(crate) fn active_sessions(&self) -> usize {
        self.sessions.len()
    }

    /// A Play command may already have started the media server and launched a
    /// player while its event waits to be drained, so a pending Play counts.
    pub(crate) fn pending_plays(&self) -> usize {
        self.pending
            .values()
            .filter(|p| matches!(p, Pending::Play { .. } | Pending::Replay))
            .count()
    }

    /// True when quitting could stop a live or just-launching stream.
    pub(crate) fn needs_quit_confirmation(&self) -> bool {
        self.active_sessions() > 0 || self.pending_plays() > 0
    }
}

#[cfg(test)]
mod tests {
    use super::{Pending, Tracker};

    #[test]
    fn obsolete_prepare_is_not_current_but_settings_always_apply() {
        let mut t = Tracker::default();
        let first = t.issue(Pending::Prepare);
        let settings = t.issue(Pending::Settings);
        let second = t.issue(Pending::Prepare);
        assert!(!t.resolve(first).current);
        assert!(t.resolve(second).current);
        let resolved = t.resolve(settings);
        assert_eq!(resolved.pending, Some(Pending::Settings));
        assert!(resolved.current);
        assert_eq!(t.pending_count(), 0);
    }

    #[test]
    fn play_supersedes_play_and_unknown_generations_are_unsolicited() {
        let mut t = Tracker::default();
        let a = t.issue(Pending::Play { remote: true });
        let b = t.issue(Pending::Play { remote: false });
        assert!(!t.resolve(a).current);
        assert!(t.resolve(b).current);
        let unknown = t.resolve(0);
        assert_eq!(unknown.pending, None);
    }

    #[test]
    fn sessions_stay_tracked_until_released_even_when_result_was_stale() {
        let mut t = Tracker::default();
        t.session_started("a".into(), true);
        t.session_started("a".into(), true);
        t.session_started("b".into(), false);
        assert_eq!(t.active_sessions(), 2);
        t.session_released("a");
        assert_eq!(t.active_sessions(), 1);
        t.session_released("missing");
        assert_eq!(t.active_sessions(), 1);
    }

    #[test]
    fn superseded_load_is_not_current_while_a_newer_load_is_pending() {
        let mut t = Tracker::default();
        let old = t.issue(Pending::Load);
        let new = t.issue(Pending::Load);
        assert!(!t.resolve(old).current);
        assert!(t.resolve(new).current);
    }

    #[test]
    fn release_commands_remember_their_session() {
        let mut t = Tracker::default();
        let g = t.issue_release("lease-1".into());
        let resolved = t.resolve(g);
        assert_eq!(resolved.pending, Some(Pending::Release));
        assert_eq!(resolved.release.as_deref(), Some("lease-1"));
        let h = t.issue_release("lease-2".into());
        t.withdraw(h);
        assert_eq!(t.resolve(h).release, None);
    }

    #[test]
    fn pending_play_requires_quit_confirmation_until_resolved_or_withdrawn() {
        let mut t = Tracker::default();
        assert!(!t.needs_quit_confirmation());
        let rejected = t.issue(Pending::Play { remote: true });
        assert!(t.needs_quit_confirmation());
        t.withdraw(rejected);
        assert!(!t.needs_quit_confirmation());
        let g = t.issue(Pending::Play { remote: false });
        t.resolve(g);
        t.session_started("a".into(), false);
        assert!(t.needs_quit_confirmation());
        let r = t.issue_release("a".into());
        t.resolve(r);
        t.session_released("a");
        assert!(!t.needs_quit_confirmation());
        t.issue(Pending::Prepare);
        assert!(!t.needs_quit_confirmation());
    }

    #[test]
    fn withdrawn_command_leaves_no_pending_entry() {
        let mut t = Tracker::default();
        let g = t.issue(Pending::Cleanup);
        t.withdraw(g);
        assert_eq!(t.pending_count(), 0);
    }
    #[test]
    fn dismissed_launch_rejects_late_ui_result_without_forgetting_lease() {
        let mut tracker = Tracker::default();
        let prepare = tracker.issue(Pending::Prepare);
        let play = tracker.issue(Pending::Play { remote: true });
        tracker.cancel(prepare);
        tracker.cancel(play);
        assert!(tracker.needs_quit_confirmation());
        assert!(!tracker.resolve(prepare).current);
        assert!(!tracker.resolve(play).current);
        tracker.session_started("late-session".into(), true);
        assert!(tracker.needs_quit_confirmation());
        tracker.session_released("late-session");
        assert!(!tracker.needs_quit_confirmation());
    }
    #[test]
    fn replay_is_pending_launch_work_and_failure_keeps_existing_lease() {
        let mut tracker = Tracker::default();
        tracker.session_started("existing".into(), false);
        let replay = tracker.issue_session(Pending::Replay, "existing".into());
        assert_eq!(tracker.pending_plays(), 1);
        let result = tracker.resolve(replay);
        assert_eq!(result.release.as_deref(), Some("existing"));
        assert_eq!(tracker.active_sessions(), 1);
        assert_eq!(tracker.pending_plays(), 0);
    }
}
