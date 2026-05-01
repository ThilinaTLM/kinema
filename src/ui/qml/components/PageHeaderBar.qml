// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Single-bar page chrome for filter-driven pages (Search, Browse,
// Streams, Subtitles).
//
// Replaces the older two-bar arrangement (Kirigami's auto title bar
// stacked above a `QQC2.ToolBar` of filters) by inlining the page
// title with the filter widgets, the optional "More filters" affordance,
// and the page-level actions on a single row:
//
//   [\u2190 back?] [Title] [\u2014 default-property filter widgets \u2014]
//   [More filters\u2026 (N)] [page actions\u2026]
//
// Caller-side shape:
//
//   header: PageHeaderBar {
//       title: page.title
//       pageActions: page.actions
//       advancedFiltersDialog: filtersDlg            // optional
//       advancedFilterCount: filtersVm.advancedActive // optional
//
//       MediaKindSelect { /* \u2026 */ }
//       FilterMenuButton { /* \u2026 */ }
//       /* default property: any QtQuick.Layouts-friendly Items */
//   }
//
// The default property forwards children into an inner fill-width
// `RowLayout`, so callers can use `Layout.fillWidth` / `Layout.alignment`
// on their filter widgets the same way they would inside any RowLayout.
//
// On `Component.onCompleted` the bar walks up to the parent
// `Kirigami.Page` and pins `globalToolBarStyle` to `None`, so Kirigami's
// auto-title bar disappears for the page that hosts this header. Pages
// pushed on top of the navigation root re-introduce a back affordance
// via the bar's leftmost ToolButton.
QQC2.ToolBar {
    id: root

    // ---- API -----------------------------------------------------
    property string title: ""
    // list<Kirigami.Action> rendered on the right of the bar. Use the
    // page's existing `actions:` declaration to populate.
    property var pageActions: []
    // Cap for the title label's natural width before it starts
    // eliding, so individual pages can opt into longer titles without
    // changing the shared default.
    property int titleMaximumWidth: Kirigami.Units.gridUnit * 14
    // Optional. When set, the bar renders a filters ToolButton that
    // calls `open()` on the dialog.
    property var advancedFiltersDialog: null
    // Optional override for the filters button label. When empty, the
    // default "More filters…" / "More filters" wording is used.
    property string advancedFiltersButtonText: ""
    // Drives the "(N)" count in the filters button label.
    property int advancedFilterCount: 0

    // Default property: filter widgets between the title and the
    // right-side actions. Forwarded to the inner fill-width RowLayout.
    default property alias filterContent: filtersRow.data

    // ---- Theming -------------------------------------------------
    Kirigami.Theme.colorSet: Kirigami.Theme.Header
    Kirigami.Theme.inherit: false

    leftPadding: Theme.pageMargin
    rightPadding: Theme.pageMargin
    topPadding: Theme.inlineSpacing
    bottomPadding: Theme.inlineSpacing

    // ---- Suppress Kirigami's auto title bar on the parent page ---
    Component.onCompleted: {
        let p = root.parent;
        while (p) {
            if (p instanceof Kirigami.Page) {
                p.globalToolBarStyle =
                    Kirigami.ApplicationHeaderStyle.None;
                break;
            }
            p = p.parent;
        }
    }

    // ---- Layout --------------------------------------------------
    contentItem: RowLayout {
        spacing: Theme.groupSpacing

        // ---- Back (only on pushed pages) -------------------------
        QQC2.ToolButton {
            id: backButton
            visible: {
                const w = (typeof applicationWindow === "function")
                    ? applicationWindow() : null;
                return w && w.pageStack && w.pageStack.depth > 1;
            }
            display: QQC2.AbstractButton.IconOnly
            icon.source: AppIcons.url("chevron-left")
            icon.color: AppIcons.controlColor(enabled, false)
            text: i18nc("@action:button", "Back")
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: {
                const w = applicationWindow();
                if (w && w.pageStack && w.pageStack.depth > 1) {
                    w.pageStack.pop();
                }
            }
        }

        // ---- Title ----------------------------------------------
        // Elides at the right; tooltip surfaces the full string when
        // truncated. Cap the natural width so a long pushed-page
        // title (e.g. "Movie X \u00b7 12 streams") cannot push the
        // filter row off-screen. Pages with naturally longer labels
        // can raise `titleMaximumWidth`.
        Kirigami.Heading {
            id: titleLabel
            level: 2
            text: root.title
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            Layout.alignment: Qt.AlignVCenter
            Layout.maximumWidth: root.titleMaximumWidth

            HoverHandler { id: titleHover }
            QQC2.ToolTip.text: root.title
            QQC2.ToolTip.visible: titleHover.hovered && titleLabel.truncated
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }

        // ---- Filters slot ---------------------------------------
        // Caller children land here (default property forwards to
        // `data`). Layout.fillWidth claims the gap between the
        // title and the right-side actions.
        RowLayout {
            id: filtersRow
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Theme.inlineSpacing
        }

        // ---- More filters\u2026 (N) -----------------------------
        QQC2.ToolButton {
            visible: root.advancedFiltersDialog !== null
                && root.advancedFiltersDialog !== undefined
            flat: true
            display: QQC2.AbstractButton.TextBesideIcon
            icon.source: AppIcons.url("funnel")
            icon.color: AppIcons.controlColor(enabled, false)
            text: {
                if (root.advancedFilterCount > 0) {
                    const label = root.advancedFiltersButtonText.length > 0
                        ? root.advancedFiltersButtonText
                        : i18nc("@action:button open advanced filters dialog",
                            "More filters");
                    return i18nc(
                        "@action:button open filters dialog, %1 label, %2 active count",
                        "%1 (%2)", label, root.advancedFilterCount);
                }
                return root.advancedFiltersButtonText.length > 0
                    ? root.advancedFiltersButtonText
                    : i18nc("@action:button open advanced filters dialog",
                        "More filters\u2026");
            }
            onClicked: if (root.advancedFiltersDialog) {
                root.advancedFiltersDialog.open();
            }
        }

        // ---- Page actions (right edge) --------------------------
        Repeater {
            model: root.pageActions
            delegate: QQC2.ToolButton {
                required property var modelData
                action: modelData
                icon.color: modelData && modelData.icon && modelData.icon.color
                    ? modelData.icon.color
                    : AppIcons.controlColor(enabled, highlighted || checked)
                display: (modelData && modelData.displayHint
                        === Kirigami.DisplayHint.IconOnly)
                    ? QQC2.AbstractButton.IconOnly
                    : QQC2.AbstractButton.TextBesideIcon
                QQC2.ToolTip.text: modelData ? modelData.text : ""
                QQC2.ToolTip.visible: hovered
                    && display === QQC2.AbstractButton.IconOnly
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }
}
