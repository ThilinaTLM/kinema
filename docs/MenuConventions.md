# Menu Conventions

Every `QQC2.Menu` / `KinemaMenu` in the app follows the rules below.
They are enforced by the shared primitives
`src/ui/qml/components/menus/KinemaMenu.qml` and
`src/ui/qml/components/menus/KinemaMenuItem.qml`.

## Labels

- Short, **title case**, **verb-first**: `Play`, `Open Folder`,
  `Mark Watched`, `Copy Title`, `Open on TMDb`.
- Drop redundant prepositions and qualifiers: `Open Folder` (not
  "Open the folder"); `Copy Magnet` (not "Copy magnet link").
- Toggle items are **single** items whose label and icon flip with
  state: `Mark Watched` ⇄ `Mark Unwatched`, `Pin` ⇄ `Unpin`.
- `Details` is reserved for "navigate to the detail page" — the
  card's default activation — paired with the Lucide `info` icon.
  The streams-page jump has its own one-word label: `Streams`
  (Lucide `list-video`). Other navigations keep an `Open` verb
  prefix that names the target: `Open Folder`, `Open on TMDb`,
  `Open on IMDb`. Bare `Open` is not used.

## No ellipses

Never use a trailing `…` on a menu item, regardless of whether the
action opens a confirm dialog. Clear short labels carry the same
information without the visual noise; the dialog (when one opens)
is its own affordance.

- ✅ `Remove` (opens confirm dialog)
- ✅ `Delete` (opens confirm dialog)
- ✅ `Stop` (opens pause-while-playing confirm)
- ✅ `Find Streams` (pushes a page)
- ✅ `Open Folder` (launches a file manager)

`KinemaMenuItem` deliberately has no `confirms` property; the label
renders verbatim from `label`. If you find yourself wanting to
flag "this opens a dialog" visually, add the dialog — the menu
item stays short.

## Ordering

Top to bottom, with `QQC2.MenuSeparator` between groups; empty groups
collapse.

1. **Primary** — what the user most often wants
   (`Play` / `Resume` / `Details` / `Streams`).
2. **Navigate** — alternate destinations
   (`Streams`, `Details`, `Open Folder`).
3. **State** — toggles and pins
   (`Mark Watched`, `Pin`).
4. **Copy / External** — clipboard and external apps
   (`Copy Title`, `Copy Magnet`, `Open Magnet`, `Open on TMDb`,
   `Open on IMDb`).
5. **Advanced** — power-user options
   (`Play via Torrent`, `Subtitles`).

Labels are short — prefer one or two words. Drop redundant context
that the card already conveys: a Library card menu just says
`Remove`, not `Remove from Library`; an episode row's title-copy
item is `Copy Title`, not `Copy Episode Title`.
6. **Destructive** — `Remove`, `Delete`, `Stop`. Always last; always
   `destructive: true` on `KinemaMenuItem`.

## Visual

- Icons via `iconName:` on `KinemaMenuItem` (resolves through
  `AppIcons.url`). Default tint is `AppIcons.controlColor(enabled,
  false)`.
- Destructive items: `destructive: true` →
  `icon.color: AppIcons.negative`.
- Disabled items stay **visible** so the user sees the affordance and
  can hover for a reason; use `enabled:`. Exception: state-mutually-
  exclusive items (`Pin` vs `Unpin`, `Mark Watched` vs
  `Mark Unwatched`) collapse to one item that flips its label/icon.
  Use `visible:` only for that one case.

## Activation paths

A right-clickable card must reach the same menu from:

- Mouse right-click (`TapHandler` on `acceptedButtons:
  Qt.RightButton` or chassis `contextMenuRequested` signal).
- Keyboard menu key (`Keys.onMenuPressed` on the focused card
  delegate).

`BaseListCard` already does this; `BasePosterCard` and
`EpisodeRailCard` were updated as part of the same change.

## Example

```qml
import dev.tlmtech.kinema.app

KinemaMenu {
    id: menu
    property int row: -1

    KinemaMenuItem {
        iconName: "play"
        label: i18nc("@action:inmenu", "Play")
        onTriggered: vm.play(menu.row)
    }
    KinemaMenuItem {
        iconName: "list-video"
        label: i18nc("@action:inmenu", "Find Streams")
        onTriggered: vm.findStreams(menu.row)
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "copy"
        label: i18nc("@action:inmenu", "Copy Title")
        onTriggered: vm.copyTitle(menu.row)
    }
    QQC2.MenuSeparator { }
    KinemaMenuItem {
        iconName: "trash-2"
        label: i18nc("@action:inmenu", "Delete")
        destructive: true
        onTriggered: confirmDialog.open()
    }
}
```
