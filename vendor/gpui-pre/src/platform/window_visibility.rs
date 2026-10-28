//! Main-thread, per-window visibility for retained desktop windows.
//! Unsupported backends deliberately return false; minimizing is a separate policy.
#[cfg(any(target_os = "linux", all(target_os = "macos", feature = "objc2")))]
use raw_window_handle::RawWindowHandle;
use raw_window_handle::WindowHandle;

pub(super) fn set_visible(handle: WindowHandle<'_>, _visible: bool) -> bool {
    match handle.as_raw() {
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xcb(handle) => x11_set_visible(handle.window.get(), _visible),
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xlib(handle) => {
            u32::try_from(handle.window).is_ok_and(|window| x11_set_visible(window, _visible))
        }
        #[cfg(all(target_os = "macos", feature = "objc2"))]
        RawWindowHandle::AppKit(handle) => {
            use objc2::{
                msg_send,
                runtime::{AnyObject, Bool},
            };
            // GPUI owns this NSView for the lifetime of PlatformWindow. Calls
            // originate on the native UI thread; the NSWindow is only borrowed.
            unsafe {
                let view = handle.ns_view.as_ptr().cast::<AnyObject>();
                let window: *mut AnyObject = msg_send![view, window];
                if window.is_null() {
                    return false;
                }
                if _visible {
                    let _: () = msg_send![window, orderFront: std::ptr::null::<AnyObject>()];
                } else {
                    let _: () = msg_send![window, orderOut: std::ptr::null::<AnyObject>()];
                }
                let is_visible: Bool = msg_send![window, isVisible];
                is_visible.as_bool() == _visible
            }
        }
        // In particular, raw wl_surface handles are not enough to safely
        // unmap/remap xdg_toplevel state. Keep taskbar recovery via minimize.
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn x11_set_visible(window: u32, visible: bool) -> bool {
    use x11rb::{connection::Connection, protocol::xproto::ConnectionExt};
    // Same DISPLAY selection as GPUI's Linux backend. This independent client
    // never owns/destroys the retained window. UnmapNotify/MapNotify update
    // GPUI's native visibility tracking without its close callback.
    let Ok((connection, _)) = x11rb::connect(None) else {
        return false;
    };
    let result = if visible {
        connection.map_window(window)
    } else {
        connection.unmap_window(window)
    };
    result.is_ok_and(|cookie| cookie.check().is_ok()) && connection.flush().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_is_unsupported_without_accessing_the_surface() {
        let raw = raw_window_handle::RawWindowHandle::Wayland(
            raw_window_handle::WaylandWindowHandle::new(std::ptr::NonNull::dangling()),
        );
        // The unsupported branch never dereferences this dummy surface.
        let handle = unsafe { WindowHandle::borrow_raw(raw) };
        assert!(!set_visible(handle, false));
        assert!(!set_visible(handle, true));
    }
}
