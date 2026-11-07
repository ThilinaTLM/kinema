# Notifications

**Status:** Draft · **Area:** UI / platform

## Problem

Kinema sends no native notifications for important events, such as a download
finishing or failing while the main window is hidden.

## Current state

In-app messages are toasts in the main window, and the header bell keeps a
session log of them (see [UI › Toasts](../architecture/ui.md#toasts)). Both are
only seen while the window is visible, and the log is not persisted. The only native notification is the one-time "Kinema is
still running" message when Main hides to the tray
([runtime](../architecture/runtime.md)), sent through GPUI's
`show_system_notification`.

## Recommendation

Native notifications for a short allowlist of events, such as acquisition
failed and critical storage errors. Reuse the GPUI path that already sends the
tray message (confirm its behavior on each OS and that it has no KDE
dependency), and show only safe text (no URLs or paths). Derive them from the same state that drives
toasts, and send one only when the main window is hidden, so an event is never
reported twice.

## Open questions

- Which events justify a native notification, as opposed to a toast?
- Should Settings have a "do not disturb while playing" option?
