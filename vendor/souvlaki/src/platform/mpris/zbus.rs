use std::collections::HashMap;
use std::convert::From;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use zbus::connection;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zvariant::{ObjectPath, Value};

use crate::{
    ActionCapabilities, MediaControlEvent, MediaMetadata, MediaPlayback, MediaPosition,
    PlatformConfig, SeekDirection,
};

use super::Error;

/// A handle to OS media controls.
pub struct MediaControls {
    thread: Option<ServiceThreadHandle>,
    dbus_name: String,
    friendly_name: String,
    capabilities: ActionCapabilities,
}

struct ServiceThreadHandle {
    event_channel: mpsc::SyncSender<(InternalEvent, mpsc::SyncSender<Result<(), String>>)>,
    thread: JoinHandle<()>,
}

#[derive(Clone, PartialEq, Debug)]
enum InternalEvent {
    ChangeMetadata(OwnedMetadata),
    ChangePlayback(MediaPlayback),
    ChangeVolume(f64),
    ChangeCapabilities(ActionCapabilities),
    Kill,
}

#[derive(Clone, Debug)]
struct ServiceState {
    metadata: OwnedMetadata,
    playback_status: MediaPlayback,
    volume: f64,
    capabilities: ActionCapabilities,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
struct OwnedMetadata {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub cover_url: Option<String>,
    pub duration: Option<i64>,
}

impl From<MediaMetadata<'_>> for OwnedMetadata {
    fn from(other: MediaMetadata) -> Self {
        OwnedMetadata {
            title: other.title.map(|s| s.to_string()),
            artist: other.artist.map(|s| s.to_string()),
            album: other.album.map(|s| s.to_string()),
            cover_url: other.cover_url.map(|s| s.to_string()),
            duration: other.duration.and_then(|d| d.as_micros().try_into().ok()),
        }
    }
}

impl MediaControls {
    /// Create media controls with the specified config.
    pub fn new(config: PlatformConfig) -> Result<Self, Error> {
        let PlatformConfig {
            dbus_name,
            display_name,
            ..
        } = config;

        Ok(Self {
            thread: None,
            capabilities: ActionCapabilities::default(),
            dbus_name: dbus_name.to_string(),
            friendly_name: display_name.to_string(),
        })
    }

    /// Attach the media control events to a handler.
    pub fn attach<F>(&mut self, event_handler: F) -> Result<(), Error>
    where
        F: Fn(MediaControlEvent) + Send + 'static,
    {
        self.detach()?;

        let dbus_name = self.dbus_name.clone();
        let friendly_name = self.friendly_name.clone();
        let event_handler = Arc::new(Mutex::new(event_handler));
        let (event_channel, rx) = mpsc::sync_channel(8);

        let capabilities = self.capabilities;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        self.thread = Some(ServiceThreadHandle {
            event_channel,
            thread: thread::spawn(move || {
                let result = pollster::block_on(run_service(
                    dbus_name,
                    friendly_name,
                    event_handler,
                    rx,
                    capabilities,
                    &ready_tx,
                ));
                if let Err(error) = result {
                    let _ = ready_tx.send(Err(error.to_string()));
                }
            }),
        });
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(()),
            result => {
                let message = match result {
                    Ok(Err(message)) => message,
                    _ => "service thread exited during registration".into(),
                };
                let _ = self.detach();
                Err(Error::RegistrationFailed(message))
            }
        }
    }

    /// Change advertised actions. Next/Previous must remain disabled without a queue.
    pub fn set_capabilities(&mut self, capabilities: ActionCapabilities) -> Result<(), Error> {
        self.capabilities = capabilities;
        if self.thread.is_some() {
            self.send_internal_event(InternalEvent::ChangeCapabilities(capabilities))?;
        }
        Ok(())
    }
    /// Detach the event handler.
    pub fn detach(&mut self) -> Result<(), Error> {
        if let Some(ServiceThreadHandle {
            event_channel,
            thread,
        }) = self.thread.take()
        {
            let (ack, _) = mpsc::sync_channel(1);
            event_channel.send((InternalEvent::Kill, ack)).ok();
            thread.join().map_err(|_| Error::ThreadPanicked)?;
        }
        Ok(())
    }

    /// Set the current playback status.
    pub fn set_playback(&mut self, playback: MediaPlayback) -> Result<(), Error> {
        self.send_internal_event(InternalEvent::ChangePlayback(playback))?;
        Ok(())
    }

    /// Set the metadata of the currently playing media item.
    pub fn set_metadata(&mut self, metadata: MediaMetadata) -> Result<(), Error> {
        self.send_internal_event(InternalEvent::ChangeMetadata(metadata.into()))?;
        Ok(())
    }

    /// Set the volume level (0.0 - 1.0) (Only available on MPRIS)
    pub fn set_volume(&mut self, volume: f64) -> Result<(), Error> {
        self.send_internal_event(InternalEvent::ChangeVolume(volume))?;
        Ok(())
    }

    fn send_internal_event(&mut self, event: InternalEvent) -> Result<(), Error> {
        let channel = &self
            .thread
            .as_ref()
            .ok_or(Error::ThreadNotRunning)?
            .event_channel;
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        channel
            .send((event, ack_tx))
            .map_err(|_| Error::ThreadPanicked)?;
        match ack_rx.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(Error::RegistrationFailed(error)),
            Err(_) => Err(Error::ThreadPanicked),
        }
    }
}

struct AppInterface {
    friendly_name: String,
    event_handler: Arc<Mutex<dyn Fn(MediaControlEvent) + Send + 'static>>,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl AppInterface {
    fn raise(&self) {
        self.send_event(MediaControlEvent::Raise);
    }
    fn quit(&self) {
        self.send_event(MediaControlEvent::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_tracklist(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        &self.friendly_name
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> &[&str] {
        &[]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> &[&str] {
        &[]
    }
}

impl AppInterface {
    fn send_event(&self, event: MediaControlEvent) {
        (self.event_handler.lock().unwrap())(event);
    }
}

struct PlayerInterface {
    state: ServiceState,
    event_handler: Arc<Mutex<dyn Fn(MediaControlEvent) + Send + 'static>>,
}

impl PlayerInterface {
    fn send_event(&self, event: MediaControlEvent) {
        (self.event_handler.lock().unwrap())(event);
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerInterface {
    fn next(&self) {
        self.send_event(MediaControlEvent::Next);
    }
    fn previous(&self) {
        self.send_event(MediaControlEvent::Previous);
    }
    fn pause(&self) {
        self.send_event(MediaControlEvent::Pause);
    }
    fn play_pause(&self) {
        self.send_event(MediaControlEvent::Toggle);
    }
    fn stop(&self) {
        self.send_event(MediaControlEvent::Stop);
    }
    fn play(&self) {
        self.send_event(MediaControlEvent::Play);
    }

    fn seek(&self, offset: i64) {
        let abs_offset = offset.unsigned_abs();
        let direction = if offset > 0 {
            SeekDirection::Forward
        } else {
            SeekDirection::Backward
        };

        self.send_event(MediaControlEvent::SeekBy(
            direction,
            Duration::from_micros(abs_offset),
        ));

        // NOTE: Should the `Seeked` signal be called when calling this method?
    }

    fn set_position(&self, _track_id: zvariant::ObjectPath, position: i64) {
        if let Ok(micros) = position.try_into() {
            if let Some(duration) = self.state.metadata.duration {
                // If the Position argument is greater than the track length, do nothing.
                if position > duration {
                    return;
                }
            }

            let position = Duration::from_micros(micros);
            self.send_event(MediaControlEvent::SetPosition(MediaPosition(position)));
        }
    }

    fn open_uri(&self, uri: String) {
        // NOTE: we should check if the URI is in the `SupportedUriSchemes` list.
        self.send_event(MediaControlEvent::OpenUri(uri));
    }

    #[zbus(property)]
    fn playback_status(&self) -> &'static str {
        match self.state.playback_status {
            MediaPlayback::Playing { .. } => "Playing",
            MediaPlayback::Paused { .. } => "Paused",
            MediaPlayback::Stopped => "Stopped",
        }
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<&str, Value<'_>> {
        // TODO: this should be stored in a cache inside the state.
        let mut dict = HashMap::<&str, Value>::new();

        let OwnedMetadata {
            ref title,
            ref album,
            ref artist,
            ref cover_url,
            ref duration,
        } = self.state.metadata;

        // MPRIS
        dict.insert(
            "mpris:trackid",
            // TODO: this is just a workaround to enable SetPosition.
            Value::new(ObjectPath::try_from("/").unwrap()),
        );

        if let Some(length) = duration {
            dict.insert("mpris:length", Value::new(*length));
        }

        if let Some(cover_url) = cover_url {
            dict.insert("mpris:artUrl", Value::new(cover_url.clone()));
        }

        // Xesam
        if let Some(title) = title {
            dict.insert("xesam:title", Value::new(title.clone()));
        }
        if let Some(artist) = artist {
            dict.insert("xesam:artist", Value::new(vec![artist.clone()]));
        }
        if let Some(album) = album {
            dict.insert("xesam:album", Value::new(album.clone()));
        }
        dict
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.state.volume
    }

    #[zbus(property)]
    fn set_volume(&mut self, volume: f64) {
        self.state.volume = volume;
        self.send_event(MediaControlEvent::SetVolume(volume));
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        let position = match self.state.playback_status {
            MediaPlayback::Playing {
                progress: Some(pos),
            }
            | MediaPlayback::Paused {
                progress: Some(pos),
            } => pos.0.as_micros(),
            _ => 0,
        };

        position.try_into().unwrap_or(0)
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.state.capabilities.next
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.state.capabilities.previous
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.state.capabilities.play
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.state.capabilities.pause
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.state.capabilities.seek
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        let c = self.state.capabilities;
        c.play || c.pause || c.stop || c.next || c.previous || c.seek
    }
}

async fn run_service(
    dbus_name: String,
    friendly_name: String,
    event_handler: Arc<Mutex<dyn Fn(MediaControlEvent) + Send + 'static>>,
    event_channel: mpsc::Receiver<(InternalEvent, mpsc::SyncSender<Result<(), String>>)>,
    capabilities: ActionCapabilities,
    ready: &mpsc::SyncSender<Result<(), String>>,
) -> zbus::Result<()> {
    let app = AppInterface {
        friendly_name,
        event_handler: event_handler.clone(),
    };

    let player = PlayerInterface {
        state: ServiceState {
            metadata: OwnedMetadata::default(),
            playback_status: MediaPlayback::Stopped,
            volume: 1.0,
            capabilities,
        },
        event_handler,
    };

    let name = format!("org.mpris.MediaPlayer2.{dbus_name}");
    let path = ObjectPath::try_from("/org/mpris/MediaPlayer2")?;
    let connection = connection::Builder::session()?
        .serve_at(&path, app)?
        .serve_at(&path, player)?
        .name(name.as_str())?
        .build()
        .await?;
    let _ = ready.send(Ok(()));

    loop {
        if let Ok((event, ack)) = event_channel.recv_timeout(Duration::from_millis(10)) {
            if event == InternalEvent::Kill {
                break;
            }

            let interface_ref = connection
                .object_server()
                .interface::<_, PlayerInterface>(&path)
                .await?;
            let mut interface = interface_ref.get_mut().await;
            let ctxt = SignalEmitter::new(&connection, &path)?;

            match event {
                InternalEvent::ChangeMetadata(metadata) => {
                    interface.state.metadata = metadata;
                    interface.metadata_changed(&ctxt).await?;
                }
                InternalEvent::ChangePlayback(playback) => {
                    interface.state.playback_status = playback;
                    interface.playback_status_changed(&ctxt).await?;
                }
                InternalEvent::ChangeVolume(volume) => {
                    interface.state.volume = volume;
                    interface.volume_changed(&ctxt).await?;
                }
                InternalEvent::ChangeCapabilities(capabilities) => {
                    interface.state.capabilities = capabilities;
                    interface.can_go_next_changed(&ctxt).await?;
                    interface.can_go_previous_changed(&ctxt).await?;
                    interface.can_play_changed(&ctxt).await?;
                    interface.can_pause_changed(&ctxt).await?;
                    interface.can_seek_changed(&ctxt).await?;
                    interface.can_control_changed(&ctxt).await?;
                }
                InternalEvent::Kill => (),
            }
            let _ = ack.send(Ok(()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> PlayerInterface {
        PlayerInterface {
            state: ServiceState {
                metadata: OwnedMetadata::default(),
                playback_status: MediaPlayback::Stopped,
                volume: 1.0,
                capabilities: ActionCapabilities::default(),
            },
            event_handler: Arc::new(Mutex::new(|_| {})),
        }
    }

    #[test]
    fn properties_follow_selected_player_capabilities() {
        let mut player = player();
        assert!(!player.can_go_next());
        assert!(!player.can_go_previous());
        assert!(!player.can_seek());
        assert!(!player.can_play());
        player.state.capabilities = ActionCapabilities {
            play: true,
            pause: true,
            stop: true,
            seek: true,
            ..ActionCapabilities::default()
        };
        assert!(player.can_play());
        assert!(player.can_pause());
        assert!(player.can_seek());
        assert!(player.can_control());
        assert!(!player.can_go_next());
        assert!(!player.can_go_previous());
        player.state.capabilities.seek = false;
        assert!(!player.can_seek());
    }

    #[test]
    fn capabilities_can_be_configured_before_registration_without_a_bus() {
        let mut controls = MediaControls::new(PlatformConfig {
            display_name: "Test",
            dbus_name: "kinema_test",
            hwnd: None,
        })
        .unwrap();
        controls
            .set_capabilities(ActionCapabilities {
                seek: true,
                ..ActionCapabilities::default()
            })
            .unwrap();
        assert!(controls.capabilities.seek);
        assert!(!controls.capabilities.next);
        assert!(controls.thread.is_none());
    }

    #[test]
    fn overflowing_duration_is_not_a_panic_or_invalid_metadata() {
        let metadata: OwnedMetadata = MediaMetadata {
            duration: Some(Duration::MAX),
            ..MediaMetadata::default()
        }
        .into();
        assert_eq!(metadata.duration, None);
    }

    #[test]
    fn registration_failure_is_returned_not_reported_as_success() {
        const CHILD: &str = "KINEMA_TEST_MPRIS_FAILURE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let mut controls = MediaControls::new(PlatformConfig {
                display_name: "Test",
                dbus_name: "kinema_test",
                hwnd: None,
            })
            .unwrap();
            assert!(controls.attach(|_| {}).is_err());
            assert!(controls.thread.is_none());
        } else {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "platform::mpris::zbus::tests::registration_failure_is_returned_not_reported_as_success"])
                .env(CHILD, "1")
                .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/kinema-test-bus")
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
