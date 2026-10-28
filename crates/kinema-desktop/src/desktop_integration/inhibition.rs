//! One process-wide owner, independent of the selected OS media player.
use super::PlayerTarget;
use std::{
    collections::HashSet,
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

pub const BUFFERING_GRACE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug)]
pub enum Activity {
    Playing,
    /// Start of the uninterrupted buffering interval; do not reset on each sample.
    Buffering {
        since: Instant,
    },
    Inactive,
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerActivity {
    pub target: PlayerTarget,
    pub activity: Activity,
}

pub fn inhibition_required(players: &[PlayerActivity], now: Instant) -> bool {
    players.iter().any(|player| match player.activity {
        Activity::Playing => true,
        Activity::Buffering { since } => now.saturating_duration_since(since) < BUFFERING_GRACE,
        Activity::Inactive => false,
    })
}

/// Backend ownership stays on one worker thread (Windows power state is thread
/// bound). Creation, reconciliation and Drop must all happen on that worker.
/// This type is deliberately !Send/!Sync, including on Linux.
#[derive(Default)]
pub struct Inhibitor {
    handle: Option<keepawake::KeepAwake>,
    active: HashSet<PlayerTarget>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl Inhibitor {
    /// Blocking OS calls: never call in GPUI render or on the UI thread.
    /// Pass the complete current embedded-player list, including paused players.
    /// Errors are nonfatal capability failures; the caller should explain once.
    pub fn reconcile(&mut self, players: &[PlayerActivity], now: Instant) -> Result<(), String> {
        self.active = players
            .iter()
            .filter(|p| inhibition_required(&[**p], now))
            .map(|p| p.target)
            .collect();
        if self.active.is_empty() {
            self.handle = None;
        } else if self.handle.is_none() {
            self.handle = Some(
                keepawake::Builder::default()
                    .display(true)
                    .idle(true)
                    // sleep(true) may inhibit explicit suspend on Linux/macOS.
                    // Only automatic idle sleep may be inhibited by this feature.
                    .sleep(false)
                    .reason("Video playback")
                    .app_name("Kinema")
                    .app_reverse_domain("io.kinema.desktop")
                    .create()
                    .map_err(|error| error.to_string())?,
            );
        }
        Ok(())
    }

    pub fn is_acquired(&self) -> bool {
        self.handle.is_some()
    }
    pub fn release(&mut self) {
        self.handle = None;
        self.active.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn player(id: u64, activity: Activity) -> PlayerActivity {
        PlayerActivity {
            target: PlayerTarget {
                session_id: id,
                generation: 1,
            },
            activity,
        }
    }

    #[test]
    fn all_players_not_selected_player_control_inhibition() {
        let now = Instant::now();
        assert!(inhibition_required(
            &[player(1, Activity::Inactive), player(2, Activity::Playing)],
            now
        ));
        assert!(!inhibition_required(&[player(1, Activity::Inactive)], now));
        assert!(!inhibition_required(&[], now));
    }

    #[test]
    fn buffering_grace_expires_without_refresh() {
        let now = Instant::now();
        let players = [player(1, Activity::Buffering { since: now })];
        assert!(inhibition_required(&players, now + Duration::from_secs(29)));
        assert!(!inhibition_required(&players, now + BUFFERING_GRACE));
        assert!(!inhibition_required(
            &players,
            now + Duration::from_secs(60)
        ));
    }
}
