//! Native adapters only: the application owns routing, GPUI tasks and shutdown.
//! See `README.md` for integration prerequisites and unqualified capabilities.

pub mod inhibition;
pub mod media_session;
pub mod runtime;
pub mod tray;
mod tray_asset;
pub use runtime::{IconData, Runtime, RuntimeConfig, RuntimeSnapshot, RuntimeStatus};

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

/// Generation must change whenever a session ID is reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerTarget {
    pub session_id: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Transport {
    Play,
    Pause,
    Toggle,
    Stop,
    SeekRelative {
        forward: bool,
        amount: Duration,
    },
    SeekAbsolute(Duration),
    /// Normalized 0..=1, not mpv's percentage units.
    SetVolume(f64),
    ChangeVolume(f64),
    ToggleMute,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Transport {
        target: PlayerTarget,
        action: Transport,
    },
    ShowPlayer(PlayerTarget),
    SelectPlayer(PlayerTarget),
    ShowMain,
    /// Request lifecycle-guarded Main-only hide, never application-wide hide.
    HideMain,
    Transfers,
    Settings,
    Quit,
    /// Restore Main immediately; registration is not proof of a visible tray.
    TrayHostLost,
}

/// A callback never waits for queue space. Overflow drops the command, not state.
#[derive(Clone)]
pub struct CommandSender {
    sender: mpsc::SyncSender<Command>,
    alive: Arc<AtomicBool>,
}

impl CommandSender {
    pub fn try_send(&self, command: Command) -> bool {
        self.alive.load(Ordering::Acquire) && self.sender.try_send(command).is_ok()
    }
}

/// Owned by a GPUI coordination task, not a foreign callback. Drop revokes clones.
pub struct CommandReceiver {
    receiver: mpsc::Receiver<Command>,
    alive: Arc<AtomicBool>,
}

impl CommandReceiver {
    pub fn try_recv(&self) -> Result<Command, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

impl Drop for CommandReceiver {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}

pub fn command_channel(capacity: usize) -> (CommandSender, CommandReceiver) {
    let (sender, receiver) = mpsc::sync_channel(capacity.max(1));
    let alive = Arc::new(AtomicBool::new(true));
    (
        CommandSender {
            sender,
            alive: alive.clone(),
        },
        CommandReceiver { receiver, alive },
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playback {
    Playing,
    Paused,
    Stopped,
}

/// Observed mpv state only. No paths, provider URLs, credentials or artwork URLs.
#[derive(Clone, Debug)]
pub struct PlayerSnapshot {
    pub target: PlayerTarget,
    /// Public catalog title; callers must never substitute a source/file name.
    pub title: String,
    pub description: Option<String>,
    pub playback: Playback,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub volume: f64,
    pub seekable: bool,
}

/// Desktop routing only, not playback ownership. Progress publication must not
/// call `started`/`focus`; observed samples never steal an explicit selection.
#[derive(Default, Debug)]
pub struct Selection {
    recent: Vec<PlayerTarget>,
    explicit: Option<PlayerTarget>,
}

impl Selection {
    pub fn started(&mut self, target: PlayerTarget) {
        self.recent
            .retain(|existing| existing.session_id != target.session_id);
        if self
            .explicit
            .is_some_and(|existing| existing.session_id == target.session_id && existing != target)
        {
            self.explicit = None;
        }
        self.recent.push(target);
    }

    pub fn focus(&mut self, target: PlayerTarget) -> bool {
        if !self.recent.contains(&target) {
            return false;
        }
        self.started(target);
        self.explicit = Some(target);
        true
    }

    pub fn closed(&mut self, target: PlayerTarget) {
        self.recent.retain(|existing| *existing != target);
        if self.explicit == Some(target) {
            self.explicit = None;
        }
    }

    pub fn selected(&self) -> Option<PlayerTarget> {
        self.explicit.or_else(|| self.recent.last().copied())
    }
}

pub fn normalized_volume(value: f64) -> Option<f64> {
    value.is_finite().then(|| value.clamp(0.0, 1.0))
}

/// Reject queued transport from the old selected generation after selection changes.
pub fn command_matches_selection(command: &Command, selected: Option<PlayerTarget>) -> bool {
    match command {
        Command::Transport { target, .. } => selected == Some(*target),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_paused_selection_survives_other_starts_and_falls_back_on_close() {
        let one = PlayerTarget {
            session_id: 1,
            generation: 1,
        };
        let two = PlayerTarget {
            session_id: 2,
            generation: 1,
        };
        let three = PlayerTarget {
            session_id: 3,
            generation: 1,
        };
        let mut selection = Selection::default();
        assert!(!selection.focus(one));
        selection.started(one);
        selection.started(two);
        assert_eq!(selection.selected(), Some(two));
        assert!(selection.focus(one));
        selection.started(three);
        assert_eq!(selection.selected(), Some(one));
        selection.closed(two);
        assert_eq!(selection.selected(), Some(one));
        selection.closed(one);
        assert_eq!(selection.selected(), Some(three));
        selection.closed(three);
        assert_eq!(selection.selected(), None);
    }

    #[test]
    fn reused_session_never_falls_back_to_a_dead_generation() {
        let old = PlayerTarget {
            session_id: 1,
            generation: 1,
        };
        let new = PlayerTarget {
            generation: 2,
            ..old
        };
        let mut selection = Selection::default();
        selection.started(old);
        assert!(selection.focus(old));
        selection.started(new);
        assert_eq!(selection.selected(), Some(new));
        selection.closed(old);
        assert_eq!(selection.selected(), Some(new));
        selection.closed(new);
        assert_eq!(selection.selected(), None);
    }

    #[test]
    fn bounded_and_revoked() {
        let (tx, rx) = command_channel(1);
        assert!(tx.try_send(Command::ShowMain));
        assert!(!tx.try_send(Command::Quit));
        assert_eq!(rx.try_recv(), Ok(Command::ShowMain));
        drop(rx);
        assert!(!tx.try_send(Command::Quit));
    }

    #[test]
    fn stale_selected_generation_is_rejected() {
        let old = PlayerTarget {
            session_id: 1,
            generation: 1,
        };
        let new = PlayerTarget {
            generation: 2,
            ..old
        };
        let command = Command::Transport {
            target: old,
            action: Transport::Play,
        };
        assert!(command_matches_selection(&command, Some(old)));
        assert!(!command_matches_selection(&command, Some(new)));
        assert!(!command_matches_selection(&command, None));
    }

    #[test]
    fn volume_units_and_nonfinite_values() {
        assert_eq!(normalized_volume(5.0), Some(1.0));
        assert_eq!(normalized_volume(-1.0), Some(0.0));
        assert_eq!(normalized_volume(0.5), Some(0.5));
        assert_eq!(normalized_volume(f64::NAN), None);
        assert_eq!(normalized_volume(f64::INFINITY), None);
    }
}
