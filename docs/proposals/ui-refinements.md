# UI refinements

**Status:** Draft · **Area:** UI

## Problem

Several screens and behaviors lag behind the current design system. Each gap is
small, so they share one proposal. Split any of them into its own proposal once
it grows.

## Current state

The screens and Settings sections are described in [UI](../architecture/ui.md),
and the tokens and motion rules in the
[design system](../architecture/design-system.md). The reduced-motion setting's
"System" option cannot read the OS preference.

## Options

These are independent candidates, not alternatives:

1. **Settings redesign.** Make Settings easier to navigate, with search, clearer
   grouping, and inline guidance for credentials. Keep it a single-instance
   window that applies changes immediately.
2. **Streams page polish.** Bring it in line with the rest of the app: header
   density, tag readability, inspector layout at medium widths.
3. **Integrated titlebar on Windows and Linux.** Client-side decorations so the
   floating toolbar reaches the top edge, as it already does on macOS. Needs
   native drag regions and window controls, and must work on Wayland.
4. **OS reduced-motion signal.** GPUI does not expose it, so "System" currently
   means "not reduced". Add it through a small vendored patch, or a
   platform query when the app starts.
5. **Clearer busy and toggle affordances.** Check that the busy markers on
   `Button` read clearly at 1×, and that `Switch` and checkbox-like toggles
   look different from each other.
6. **Player chrome polish** to match the latest design-system refinements.

## Open questions

- Which of these block a first release?
