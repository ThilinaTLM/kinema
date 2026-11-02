# Notifications

**Status:** Draft · **Area:** UI / platform

## Problem

In-app messages, such as personal-state toasts with Undo, notices and alert
strips, are placed inconsistently. Kinema also sends no native notifications for
important events, such as a download finishing or failing while the main window
is hidden.

## Current state

Each window presents its own messages: personal-state toasts with Undo,
notices and the stale alert strip (see
[state presentation](../architecture/ui.md#state-presentation)). There is no
shared toast primitive in `ui/primitives` and no native notification
dependency.

## Recommendation

1. **A shared `Toast` primitive** in `ui/primitives`. It uses one placement and
   stacking rule, at most one action, timeouts from `theme::motion::timing`, and
   presets for motion. It is announced to assistive technology. Existing ad-hoc
   messages move to it.
2. **Native notifications** for a short allowlist of events: acquisition
   failed, the main window was hidden to the tray, and critical storage errors.
   Use an existing cross-platform crate, check that it has no KDE dependency,
   and show only safe text (no URLs or paths).

## Open questions

- Which events justify a native notification, as opposed to an in-app one?
- Should Settings have a "do not disturb while playing" option?
