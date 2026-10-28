//! Souvlaki adapter using the repository-owned action-capability patch.
use super::{
    Command, CommandSender, Playback, PlayerSnapshot, PlayerTarget, Transport, normalized_volume,
};
use souvlaki::{
    ActionCapabilities, MediaControlEvent as Event, MediaControls, MediaMetadata, MediaPlayback,
    MediaPosition, PlatformConfig, SeekDirection,
};
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationError {
    MissingRetainedWindowsHost,
    Backend(String),
}

/// Owner must keep this native HWND alive until this guard is dropped. Runtime
/// retains the guard and detaches the OS media registration before releasing it.
#[cfg(target_os = "windows")]
pub trait WindowsMediaHost {
    fn hwnd(&self) -> std::num::NonZeroIsize;
}

/// Retain on the creation thread. Windows registration is blocked until a retained
/// HWND owner is supplied by the application; never borrow the closable Main HWND.
/// macOS creation/publication must run on the application main thread.
pub struct MediaSession {
    controls: MediaControls,
    selected: Arc<Mutex<Option<(PlayerTarget, bool)>>>,
    _thread_bound: PhantomData<Rc<()>>,
    #[cfg(target_os = "windows")]
    _host: Option<Rc<dyn WindowsMediaHost>>,
}

impl MediaSession {
    pub fn register(sender: CommandSender) -> Result<Self, RegistrationError> {
        #[cfg(target_os = "windows")]
        {
            let _ = sender;
            Err(RegistrationError::MissingRetainedWindowsHost)
        }
        #[cfg(not(target_os = "windows"))]
        Self::register_config(sender, None)
    }

    #[cfg(target_os = "windows")]
    pub fn register_with_host(
        sender: CommandSender,
        host: Rc<dyn WindowsMediaHost>,
    ) -> Result<Self, RegistrationError> {
        let mut session =
            Self::register_config(sender, Some(host.hwnd().get() as *mut std::ffi::c_void))?;
        session._host = Some(host);
        Ok(session)
    }

    fn register_config(
        sender: CommandSender,
        hwnd: Option<*mut std::ffi::c_void>,
    ) -> Result<Self, RegistrationError> {
        let mut controls = MediaControls::new(PlatformConfig {
            display_name: "Kinema",
            dbus_name: "kinema",
            hwnd,
        })
        .map_err(|e| RegistrationError::Backend(format!("{e:?}")))?;
        controls
            .set_capabilities(ActionCapabilities::default())
            .map_err(|e| RegistrationError::Backend(format!("{e:?}")))?;
        let selected = Arc::new(Mutex::new(None));
        let callback_selection = selected.clone();
        controls
            .attach(move |event| {
                // Never block a native callback on UI-held state or a full channel.
                let Ok(selection) = callback_selection.try_lock() else {
                    return;
                };
                if let Some(command) = translate_event(event, *selection) {
                    sender.try_send(command);
                }
            })
            .map_err(|e| RegistrationError::Backend(format!("{e:?}")))?;
        Ok(Self {
            controls,
            selected,
            _thread_bound: PhantomData,
            #[cfg(target_os = "windows")]
            _host: None,
        })
    }

    /// Call on an owned native adapter task, never in render. Metadata and target
    /// are changed under the same gate; errors revoke routing instead of leaving
    /// callbacks pointing at a different player than the published metadata.
    pub fn publish(&mut self, snapshot: Option<&PlayerSnapshot>) -> Result<(), String> {
        let mut selected = self
            .selected
            .lock()
            .map_err(|_| "media selection poisoned")?;
        *selected = None;
        self.controls
            .set_capabilities(capabilities_for(snapshot))
            .map_err(|e| format!("{e:?}"))?;
        let Some(snapshot) = snapshot else {
            self.controls
                .set_playback(MediaPlayback::Stopped)
                .map_err(|e| format!("{e:?}"))?;
            self.controls
                .set_metadata(MediaMetadata::default())
                .map_err(|e| format!("{e:?}"))?;
            return Ok(());
        };
        self.controls
            .set_metadata(MediaMetadata {
                title: Some(&snapshot.title),
                artist: snapshot.description.as_deref(),
                duration: snapshot.duration.filter(|d| !d.is_zero()),
                // No source URL or local path can cross this adapter boundary.
                cover_url: None,
                album: None,
            })
            .map_err(|e| format!("{e:?}"))?;
        let progress = Some(MediaPosition(snapshot.position));
        let playback = match snapshot.playback {
            Playback::Playing => MediaPlayback::Playing { progress },
            Playback::Paused => MediaPlayback::Paused { progress },
            Playback::Stopped => MediaPlayback::Stopped,
        };
        self.controls
            .set_playback(playback)
            .map_err(|e| format!("{e:?}"))?;
        #[cfg(target_os = "linux")]
        if let Some(volume) = normalized_volume(snapshot.volume) {
            self.controls
                .set_volume(volume)
                .map_err(|e| format!("{e:?}"))?;
        }
        *selected = Some((snapshot.target, snapshot.seekable));
        Ok(())
    }
}

pub fn capabilities_for(snapshot: Option<&PlayerSnapshot>) -> ActionCapabilities {
    ActionCapabilities {
        play: snapshot.is_some(),
        pause: snapshot.is_some(),
        stop: snapshot.is_some(),
        next: false,
        previous: false,
        seek: snapshot.is_some_and(|snapshot| snapshot.seekable),
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        if let Ok(mut selected) = self.selected.lock() {
            *selected = None;
        }
        let _ = self.controls.detach();
    }
}

/// Unsupported queue/shuffle/OpenUri events are ignored, never arbitrary launches.
pub fn translate_event(event: Event, selected: Option<(PlayerTarget, bool)>) -> Option<Command> {
    match event {
        Event::Raise => return Some(Command::ShowMain),
        Event::Quit => return Some(Command::Quit),
        _ => {}
    }
    let (target, seekable) = selected?;
    let action = match event {
        Event::Play => Transport::Play,
        Event::Pause => Transport::Pause,
        Event::Toggle => Transport::Toggle,
        Event::Stop => Transport::Stop,
        Event::SetVolume(value) => Transport::SetVolume(normalized_volume(value)?),
        Event::SetPosition(MediaPosition(position)) if seekable => {
            Transport::SeekAbsolute(position)
        }
        Event::Seek(direction) if seekable => Transport::SeekRelative {
            forward: direction == SeekDirection::Forward,
            amount: Duration::from_secs(10),
        },
        Event::SeekBy(direction, amount) if seekable => Transport::SeekRelative {
            forward: direction == SeekDirection::Forward,
            amount,
        },
        _ => return None,
    };
    Some(Command::Transport { target, action })
}

#[cfg(test)]
mod tests {
    use super::*;
    const TARGET: PlayerTarget = PlayerTarget {
        session_id: 7,
        generation: 3,
    };

    #[test]
    fn safe_event_translation() {
        let selected = Some((TARGET, true));
        for event in [
            Event::Next,
            Event::Previous,
            Event::OpenUri("file:///secret".into()),
            Event::SetVolume(f64::NAN),
        ] {
            assert_eq!(translate_event(event, selected), None);
        }
        assert_eq!(translate_event(Event::Play, None), None);
        assert_eq!(
            translate_event(Event::Pause, selected),
            Some(Command::Transport {
                target: TARGET,
                action: Transport::Pause
            })
        );
        assert_eq!(
            translate_event(Event::SetVolume(25.0), selected),
            Some(Command::Transport {
                target: TARGET,
                action: Transport::SetVolume(1.0)
            })
        );
    }

    #[test]
    fn seeks_require_observed_seekability() {
        let event = Event::SetPosition(MediaPosition(Duration::from_secs(90)));
        assert_eq!(translate_event(event.clone(), Some((TARGET, false))), None);
        assert_eq!(
            translate_event(event, Some((TARGET, true))),
            Some(Command::Transport {
                target: TARGET,
                action: Transport::SeekAbsolute(Duration::from_secs(90))
            })
        );
    }

    #[test]
    fn selected_player_capabilities_never_advertise_a_queue() {
        let mut snapshot = PlayerSnapshot {
            target: TARGET,
            title: "Catalog title".into(),
            description: None,
            playback: Playback::Paused,
            position: Duration::ZERO,
            duration: None,
            volume: 0.5,
            seekable: true,
        };
        let capabilities = capabilities_for(Some(&snapshot));
        assert!(capabilities.play && capabilities.pause && capabilities.stop && capabilities.seek);
        assert!(!capabilities.next && !capabilities.previous);
        snapshot.seekable = false;
        assert!(!capabilities_for(Some(&snapshot)).seek);
    }

    #[test]
    fn no_player_advertises_no_transport() {
        assert_eq!(capabilities_for(None), ActionCapabilities::default());
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "media_session/linux_smoke.rs"]
mod linux_smoke;
