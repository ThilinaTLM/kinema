//! tray-icon KSNI on Linux (no GTK/AppIndicator), native Windows/macOS elsewhere.
use super::{Command, CommandSender};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem},
};

static CALLBACKS_OWNED: AtomicBool = AtomicBool::new(false);
const HEALTH_MAX_AGE: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum TrayError {
    AlreadyOwned,
    Backend(String),
    HostUnavailable,
    HostChanged,
}

#[derive(Default)]
struct HostHealth {
    owner: Option<String>,
    checked: Option<Instant>,
}

impl HostHealth {
    fn observe(
        &mut self,
        observation: Result<String, TrayError>,
        now: Instant,
    ) -> Result<(), TrayError> {
        match observation {
            Ok(owner) if self.owner.as_ref().is_none_or(|old| old == &owner) => {
                self.owner = Some(owner);
                self.checked = Some(now);
                Ok(())
            }
            observation => {
                self.checked = None;
                match observation {
                    Ok(_) => Err(TrayError::HostChanged),
                    Err(error) => Err(error),
                }
            }
        }
    }

    fn usable_at(&self, now: Instant) -> bool {
        self.checked
            .is_some_and(|checked| now.saturating_duration_since(checked) < HEALTH_MAX_AGE)
    }
}

struct CallbackLease;
impl CallbackLease {
    fn claim() -> Result<Self, TrayError> {
        CALLBACKS_OWNED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| TrayError::AlreadyOwned)
    }
}
impl Drop for CallbackLease {
    fn drop(&mut self) {
        MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
        TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
        CALLBACKS_OWNED.store(false, Ordering::Release);
    }
}

/// Thread-bound native lifetime. Linux: construct/use/drop on an owned worker;
/// Windows/macOS: construct/use/drop on GPUI's running main event-loop thread.
/// The application must reserve tray-icon/muda global event handlers for this owner.
pub struct Tray {
    // Drop callback lease before the native handle. Callback guard also covers
    // callbacks already dispatched by the OS when teardown begins.
    _lease: CallbackLease,
    handle: TrayIcon,
    commands: Arc<Mutex<HashMap<MenuId, Command>>>,
    callback_alive: Arc<AtomicBool>,
    #[cfg(target_os = "linux")]
    sender: CommandSender,
    health: HostHealth,
}

#[cfg(target_os = "windows")]
struct TrayMediaHost {
    _icon: TrayIcon,
    hwnd: std::num::NonZeroIsize,
}

#[cfg(target_os = "windows")]
impl super::media_session::WindowsMediaHost for TrayMediaHost {
    fn hwnd(&self) -> std::num::NonZeroIsize {
        self.hwnd
    }
}

impl Tray {
    #[cfg(target_os = "linux")]
    pub fn create_on_worker(icon: Icon, sender: CommandSender) -> Result<Self, TrayError> {
        Self::create(icon, sender)
    }

    /// Call from an owned GPUI task after the native application loop has started.
    #[cfg(not(target_os = "linux"))]
    pub fn create_on_ui_thread(
        _cx: &gpui::App,
        icon: Icon,
        sender: CommandSender,
    ) -> Result<Self, TrayError> {
        Self::create(icon, sender)
    }

    /// Runtime is created and polled on GPUI's running native UI thread.
    #[cfg(not(target_os = "linux"))]
    pub(super) fn create_on_native_thread(
        icon: Icon,
        sender: CommandSender,
    ) -> Result<Self, TrayError> {
        Self::create(icon, sender)
    }

    /// tray-icon documents this HWND as valid for the icon lifetime. Retain its
    /// native Rc handle rather than borrowing Main or inventing a host window.
    #[cfg(target_os = "windows")]
    pub(super) fn retained_media_host(
        &self,
    ) -> Option<std::rc::Rc<dyn super::media_session::WindowsMediaHost>> {
        let hwnd = std::num::NonZeroIsize::new(self.handle.window_handle() as isize)?;
        Some(std::rc::Rc::new(TrayMediaHost {
            _icon: self.handle.clone(),
            hwnd,
        }))
    }

    fn create(icon: Icon, sender: CommandSender) -> Result<Self, TrayError> {
        let lease = CallbackLease::claim()?;
        let builder = TrayIconBuilder::new();
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(icon);
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(icon);
        let handle = builder
            .with_tooltip("Kinema")
            .with_title("Kinema")
            .build()
            .map_err(|e| TrayError::Backend(e.to_string()))?;
        let commands = Arc::new(Mutex::new(HashMap::<MenuId, Command>::new()));
        let callback_alive = Arc::new(AtomicBool::new(true));
        let callback_commands = commands.clone();
        let alive = callback_alive.clone();
        let menu_sender = sender.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if !alive.load(Ordering::Acquire) {
                return;
            }
            let Ok(commands) = callback_commands.try_lock() else {
                return;
            };
            if let Some(command) = commands.get(&event.id) {
                menu_sender.try_send(command.clone());
            }
        }));
        let tray_id = handle.id().clone();
        let alive = callback_alive.clone();
        let tray_sender = sender.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if alive.load(Ordering::Acquire)
                && event.id() == &tray_id
                && matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    }
                )
            {
                tray_sender.try_send(Command::ShowMain);
            }
        }));
        let mut tray = Self {
            _lease: lease,
            handle,
            commands,
            callback_alive,
            #[cfg(target_os = "linux")]
            sender,
            health: HostHealth::default(),
        };
        tray.update_menu(false)?;
        // Creating an icon does NOT prove a host exists (KSNI assumes one).
        Ok(tray)
    }

    pub fn update_menu(&mut self, main_hidden: bool) -> Result<(), TrayError> {
        let menu = Menu::new();
        let mut commands = HashMap::new();
        for (label, command) in menu_entries(main_hidden) {
            append(&menu, &mut commands, label, Some(command))?;
        }
        // Replace the routing map before the menu. Old menu events now find no ID.
        *self
            .commands
            .lock()
            .map_err(|_| TrayError::Backend("tray routing poisoned".into()))? = commands;
        self.handle.set_menu(Some(Box::new(menu)));
        Ok(())
    }

    /// Linux only: blocking D-Bus query. Schedule about every two seconds on the
    /// tray worker, never from GPUI render. Host restart requires tray recreation.
    #[cfg(target_os = "linux")]
    pub fn refresh_host(&mut self, now: Instant) -> Result<(), TrayError> {
        let result = self.health.observe(linux_host_owner(), now);
        if result.is_err() {
            self.sender.try_send(Command::TrayHostLost);
        }
        result
    }

    /// Native registration is usable on Windows/macOS, but tray-icon exposes no
    /// shell-host loss notifications there. This is weaker than Linux probing.
    #[cfg(not(target_os = "linux"))]
    pub fn refresh_host(&mut self, now: Instant) -> Result<(), TrayError> {
        self.health.observe(Ok("native-registration".into()), now)
    }

    /// Recheck immediately before hiding Main. Stale health is a failure, not success.
    pub fn usable_at(&self, now: Instant) -> bool {
        self.health.usable_at(now)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.callback_alive.store(false, Ordering::Release);
    }
}

/// Pure menu contract: independent players and media controls are not tray entries.
fn menu_entries(main_hidden: bool) -> [(&'static str, Command); 3] {
    [
        if main_hidden {
            ("Show", Command::ShowMain)
        } else {
            ("Hide", Command::HideMain)
        },
        ("Settings", Command::Settings),
        ("Quit", Command::Quit),
    ]
}

fn backend(error: impl std::fmt::Display) -> TrayError {
    TrayError::Backend(error.to_string())
}
fn append(
    menu: &Menu,
    commands: &mut HashMap<MenuId, Command>,
    label: &str,
    command: Option<Command>,
) -> Result<(), TrayError> {
    let item = MenuItem::new(label, command.is_some(), None);
    if let Some(command) = command {
        commands.insert(item.id().clone(), command);
    }
    menu.append(&item).map_err(backend)
}

#[cfg(target_os = "linux")]
fn linux_host_owner() -> Result<String, TrayError> {
    // The protocol name contains KDE but requires no KDE library or desktop.
    let connection = zbus::blocking::Connection::session().map_err(backend)?;
    let watcher = zbus::blocking::Proxy::new(
        &connection,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .map_err(backend)?;
    let registered: bool = watcher
        .get_property("IsStatusNotifierHostRegistered")
        .map_err(backend)?;
    if !registered {
        return Err(TrayError::HostUnavailable);
    }
    let dbus = zbus::blocking::fdo::DBusProxy::new(&connection).map_err(backend)?;
    let name = "org.kde.StatusNotifierWatcher"
        .try_into()
        .map_err(backend)?;
    dbus.get_name_owner(name)
        .map(|owner| owner.to_string())
        .map_err(backend)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_contains_only_visibility_settings_and_quit() {
        assert_eq!(
            menu_entries(false),
            [
                ("Hide", Command::HideMain),
                ("Settings", Command::Settings),
                ("Quit", Command::Quit),
            ]
        );
        assert_eq!(
            menu_entries(true),
            [
                ("Show", Command::ShowMain),
                ("Settings", Command::Settings),
                ("Quit", Command::Quit),
            ]
        );
    }

    #[test]
    fn fake_host_loss_and_restart_never_leave_a_usable_tray() {
        let now = Instant::now();
        let mut health = HostHealth::default();
        assert!(!health.usable_at(now));
        assert!(health.observe(Ok(":1.42".into()), now).is_ok());
        assert!(health.usable_at(now));
        assert!(matches!(
            health.observe(Err(TrayError::HostUnavailable), now),
            Err(TrayError::HostUnavailable)
        ));
        assert!(!health.usable_at(now));
        assert!(health.observe(Ok(":1.42".into()), now).is_ok());
        assert!(matches!(
            health.observe(Ok(":1.43".into()), now),
            Err(TrayError::HostChanged)
        ));
        assert!(!health.usable_at(now));
        // Changed owner stays rejected until the adapter is recreated.
        assert!(health.observe(Ok(":1.43".into()), now).is_err());
    }

    #[test]
    fn stale_health_cannot_authorize_hiding_main() {
        let now = Instant::now();
        let mut health = HostHealth::default();
        assert!(health.observe(Ok(":1.1".into()), now).is_ok());
        assert!(health.usable_at(now + Duration::from_secs(4)));
        assert!(!health.usable_at(now + HEALTH_MAX_AGE));
    }
}
