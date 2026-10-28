//! Opt-in real protocol test on a private session bus, not a desktop-shell test.
use super::*;
use std::collections::HashMap;
use std::{error::Error, process::Command as ProcessCommand, sync::mpsc::TryRecvError};
use zbus::{
    blocking::{Connection, Proxy, fdo::DBusProxy, proxy::Builder},
    proxy::CacheProperties,
    zvariant::OwnedValue,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const NAME: &str = "org.mpris.MediaPlayer2.kinema";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const CHILD: &str = "KINEMA_ISOLATED_MPRIS_SMOKE_CHILD";

#[test]
#[ignore = "opt-in isolated D-Bus smoke; requires dbus-run-session and timeout"]
fn isolated_mpris_session() -> Result<()> {
    if std::env::var_os(CHILD).is_some() {
        return exercise_session();
    }
    // Env changes apply only to the subprocess. dbus-run-session supplies its own
    // DBUS_SESSION_BUS_ADDRESS; the user's running session is never touched.
    // The external deadline also bounds a broken detach/signal-wait regression.
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, module)| module);
    let test = format!("{module}::isolated_mpris_session");
    let output = ProcessCommand::new("timeout")
        .args(["30s", "dbus-run-session", "--"])
        .arg(std::env::current_exe()?)
        .args([
            "--exact",
            &test,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .output()?;
    assert!(
        output.status.success(),
        "isolated smoke failed ({:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn proxy<'a>(connection: &'a Connection, interface: &'a str) -> zbus::Result<Proxy<'a>> {
    Builder::new(connection)
        .destination(NAME)?
        .path(PATH)?
        .interface(interface)?
        // Query actual server state, never a property cache racing its signals.
        .cache_properties(CacheProperties::No)
        .build()
}

fn assert_transport_properties(player: &Proxy<'_>, seekable: bool) -> Result<()> {
    assert!(!player.get_property::<bool>("CanGoNext")?);
    assert!(!player.get_property::<bool>("CanGoPrevious")?);
    assert!(player.get_property::<bool>("CanPlay")?);
    assert!(player.get_property::<bool>("CanPause")?);
    assert!(player.get_property::<bool>("CanControl")?);
    assert_eq!(player.get_property::<bool>("CanSeek")?, seekable);
    Ok(())
}

fn exercise_session() -> Result<()> {
    let (sender, receiver) = super::super::command_channel(8);
    let mut session =
        MediaSession::register(sender).map_err(|error| format!("register: {error:?}"))?;
    let connection = Connection::session()?;
    let root = proxy(&connection, "org.mpris.MediaPlayer2")?;
    let player = proxy(&connection, PLAYER)?;
    let dbus = DBusProxy::new(&connection)?;
    assert!(dbus.name_has_owner(NAME.try_into()?)?);
    assert_eq!(root.get_property::<String>("Identity")?, "Kinema");

    let first = PlayerTarget {
        session_id: 41,
        generation: 3,
    };
    let mut snapshot = PlayerSnapshot {
        target: first,
        title: "Isolated catalog movie".into(),
        description: Some("Public fixture description".into()),
        playback: Playback::Paused,
        position: Duration::from_millis(72_500),
        duration: Some(Duration::from_secs(240)),
        volume: 0.37,
        seekable: false,
    };
    session.publish(Some(&snapshot))?;
    assert_transport_properties(&player, false)?;
    assert_eq!(player.get_property::<String>("PlaybackStatus")?, "Paused");
    assert_eq!(player.get_property::<i64>("Position")?, 72_500_000);
    assert_eq!(player.get_property::<f64>("Volume")?, 0.37);
    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata")?;
    assert_eq!(
        metadata
            .get("xesam:title")
            .and_then(|value| <&str>::try_from(value).ok()),
        Some("Isolated catalog movie")
    );
    assert_eq!(
        metadata
            .get("mpris:length")
            .and_then(|value| i64::try_from(value).ok()),
        Some(240_000_000)
    );
    assert!(!metadata.contains_key("mpris:artUrl"));

    let _: () = player.call("Play", &())?;
    assert_eq!(
        receiver.try_recv()?,
        Command::Transport {
            target: first,
            action: Transport::Play
        }
    );
    // An OS request does not optimistically mutate observed playback state.
    assert_eq!(player.get_property::<String>("PlaybackStatus")?, "Paused");
    println!(
        "MPRIS: registered Identity=Kinema; public metadata, duration=240000000us, position=72500000us, volume=0.37; Next/Previous=false; Seek=false; Play -> target 41/3"
    );

    let second = PlayerTarget {
        session_id: 72,
        generation: 9,
    };
    snapshot.target = second;
    snapshot.title = "Second selected catalog movie".into();
    snapshot.playback = Playback::Playing;
    snapshot.position = Duration::from_secs(10);
    snapshot.seekable = true;
    session.publish(Some(&snapshot))?;
    assert_transport_properties(&player, true)?;
    assert_eq!(player.get_property::<String>("PlaybackStatus")?, "Playing");
    assert_eq!(player.get_property::<i64>("Position")?, 10_000_000);
    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata")?;
    assert_eq!(
        metadata
            .get("xesam:title")
            .and_then(|value| <&str>::try_from(value).ok()),
        Some("Second selected catalog movie")
    );
    let _: () = player.call("Pause", &())?;
    assert_eq!(
        receiver.try_recv()?,
        Command::Transport {
            target: second,
            action: Transport::Pause
        }
    );
    println!(
        "MPRIS: selected target switched to 72/9; metadata/position updated; Next/Previous=false; Seek=true; Pause -> target 72/9"
    );

    session.publish(None)?;
    assert!(!player.get_property::<bool>("CanPlay")?);
    assert!(!player.get_property::<bool>("CanPause")?);
    assert!(!player.get_property::<bool>("CanSeek")?);
    assert_eq!(player.get_property::<String>("PlaybackStatus")?, "Stopped");
    assert_eq!(player.get_property::<i64>("Position")?, 0);
    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata")?;
    assert!(!metadata.contains_key("xesam:title"));
    let _: () = player.call("Play", &())?;
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));

    // Subscribe before detach, then wait for the actual bus-owner loss signal.
    // This avoids sleeps/polling or assuming cross-connection message ordering.
    let mut lost = dbus.receive_name_owner_changed_with_args(&[(0, NAME)])?;
    drop(session);
    let signal = lost.next().ok_or("bus owner-loss signal stream ended")?;
    let args = signal.args()?;
    assert_eq!(args.name().as_str(), NAME);
    assert!(args.new_owner().is_none());
    assert!(!dbus.name_has_owner(NAME.try_into()?)?);
    let after_drop: zbus::Result<()> = player.call("Play", &());
    assert!(after_drop.is_err());
    // Dropping registration drops the final callback sender, not merely its
    // subscription. Disconnected proves no callback owner remains attached.
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
    println!(
        "MPRIS: empty selection cleared metadata/capabilities; Drop emitted owner loss, removed bus name, rejected further Play, delivered no post-detach commands"
    );
    Ok(())
}
