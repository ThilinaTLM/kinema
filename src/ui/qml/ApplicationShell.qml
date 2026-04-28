// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.settings as KirigamiSettings

import dev.tlmtech.kinema.app

// Phase 02 shell. Owns:
//
//  * top-level navigation between Discover / Search / Browse / Settings
//    pages via `Kirigami.PageRow`
//  * icon-only primary navigation with a bottom Settings action
//  * application-wide keyboard shortcuts (Quit, Preferences,
//    Find, Help, Esc-pop)
//  * close-to-tray decision routed through `mainController`
//  * passive notifications surfaced from the C++ side
//
// Real per-page content (Discover hero / sections, Search field,
// Browse filter overlay, detail views) lands in phases 03–06.
Kirigami.ApplicationWindow {
    id: root

    title: i18nc("@title:window", "Kinema")

    width: Kirigami.Units.gridUnit * 60
    height: Kirigami.Units.gridUnit * 40
    minimumWidth: Kirigami.Units.gridUnit * 30
    minimumHeight: Kirigami.Units.gridUnit * 24

    // Kirigami.ApplicationWindow's pageStack is a PageRow. By default it
    // adapts pushed pages into multiple side-by-side columns on wide windows.
    // Kinema's title/detail flow wants stack semantics without that external
    // split: detail pages are full-width, and any useful split happens inside
    // the detail page itself.
    Binding {
        target: root.pageStack
        property: "defaultColumnWidth"
        value: Math.max(root.pageStack.width,
            Kirigami.Units.gridUnit * 20)
        restoreMode: Binding.RestoreBindingOrValue
    }

    // ---- close-to-tray --------------------------------------------------
    // The C++ side owns the decision matrix (reallyQuit /
    // closeToTray pref / tray availability / first-time toast).
    // We flip `close.accepted` to false when the controller has
    // taken responsibility for hiding the window itself.
    onClosing: function (close) {
        if (!mainController.handleWindowCloseRequested()) {
            close.accepted = false;
        }
    }

    // ---- application-level shortcuts -----------------------------------
    // Replaces the KStandardAction wiring in MainWindow. Each
    // shortcut routes through `mainController` so the C++ side
    // decides what "Settings" or "About" actually means at the
    // current phase (placeholder push now, real page in phase 06).
    Shortcut {
        sequences: [ StandardKey.Quit ]
        context: Qt.ApplicationShortcut
        onActivated: mainController.requestQuit()
    }
    Shortcut {
        sequences: [ StandardKey.Preferences ]
        context: Qt.ApplicationShortcut
        onActivated: mainController.requestSettings()
    }
    Shortcut {
        sequences: [ StandardKey.Find ]
        context: Qt.ApplicationShortcut
        onActivated: mainController.requestFocusSearch()
    }
    Shortcut {
        sequence: "F1"
        context: Qt.ApplicationShortcut
        onActivated: mainController.requestAbout()
    }
    Shortcut {
        sequence: "Esc"
        context: Qt.WindowShortcut
        onActivated: {
            if (root.pageStack.depth > 1) {
                root.pageStack.pop();
            }
        }
    }
    // ---- global drawer --------------------------------------------------
    // Always-collapsed icon rail: top `actions` list = primary
    // navigation, bottom footer button = Settings. The default
    // collapse/expand affordance is hidden so the rail stays compact.
    globalDrawer: Kirigami.GlobalDrawer {
        id: drawer
        title: i18n("Kinema")
        titleIcon: "dev.tlmtech.kinema"
        modal: false
        collapsible: true
        collapsed: true
        collapseButtonVisible: false

        actions: [
            Kirigami.Action {
                icon.name: "go-home"
                text: i18nc("@action drawer entry", "Discover")
                checkable: true
                checked: root.currentNavKey === "discover"
                onTriggered: root.showPage("discover")
            },
            Kirigami.Action {
                icon.name: "search"
                text: i18nc("@action drawer entry", "Search")
                checkable: true
                checked: root.currentNavKey === "search"
                onTriggered: root.showPage("search")
            },
            Kirigami.Action {
                icon.name: "view-list-icons"
                text: i18nc("@action drawer entry", "Browse")
                checkable: true
                checked: root.currentNavKey === "browse"
                onTriggered: root.showPage("browse")
            },
            // Settings opens a separate `KirigamiSettings.ConfigurationView`
            // window on desktop (layer push on mobile) — it is not a top-
            // level page in this PageRow, so the action is non-checkable.
            Kirigami.Action {
                icon.name: "settings-configure"
                text: i18nc("@action drawer entry", "Settings")
                onTriggered: mainController.requestSettings()
            }
        ]
    }

    // ---- settings configuration view -----------------------------------
    // KDE6 settings pattern: a separate ConfigWindow on desktop, layer push
    // on mobile. `MainController::requestSettings(category)` lands here via
    // `onShowSettingsRequested` and forwards to `open(category)`.
    KirigamiSettings.ConfigurationView {
        id: configurationView
        window: root
        modules: [
            KirigamiSettings.ConfigurationModule {
                moduleId: "general"
                text: i18nc("@title:tab settings page", "General")
                icon.name: "preferences-other"
                page: () => Qt.createComponent(
                    "dev.tlmtech.kinema.app", "GeneralSettingsPage")
            },
            KirigamiSettings.ConfigurationModule {
                moduleId: "tmdb"
                text: i18nc("@title:tab settings page", "TMDB (Discover)")
                icon.name: "applications-multimedia"
                page: () => Qt.createComponent(
                    "dev.tlmtech.kinema.app", "TmdbSettingsPage")
            },
            KirigamiSettings.ConfigurationModule {
                moduleId: "realdebrid"
                text: i18nc("@title:tab settings page", "Real-Debrid")
                icon.name: "network-server"
                page: () => Qt.createComponent(
                    "dev.tlmtech.kinema.app", "RealDebridSettingsPage")
            },
            KirigamiSettings.ConfigurationModule {
                moduleId: "filters"
                text: i18nc("@title:tab settings page", "Filters")
                icon.name: "view-filter"
                page: () => Qt.createComponent(
                    "dev.tlmtech.kinema.app", "FiltersSettingsPage")
            },
            KirigamiSettings.ConfigurationModule {
                moduleId: "player"
                text: i18nc("@title:tab settings page", "Player")
                icon.name: "media-playback-start"
                page: () => Qt.createComponent(
                    "dev.tlmtech.kinema.app", "PlayerSettingsPage")
            },
            KirigamiSettings.ConfigurationModule {
                moduleId: "subtitles"
                text: i18nc("@title:tab settings page", "Subtitles")
                icon.name: "media-view-subtitles-symbolic"
                page: () => Qt.createComponent(
                    "dev.tlmtech.kinema.app", "SubtitlesSettingsPage")
            },
            KirigamiSettings.ConfigurationModule {
                moduleId: "appearance"
                text: i18nc("@title:tab settings page", "Appearance")
                icon.name: "preferences-desktop-theme"
                page: () => Qt.createComponent(
                    "dev.tlmtech.kinema.app", "AppearanceSettingsPage")
            },
            // The About page lives inside the settings dialog — the modern
            // KDE6 convention. F1 still opens it via
            // `mainController.requestAbout()` which routes here.
            KirigamiSettings.ConfigurationModule {
                moduleId: "about"
                text: i18nc("@title:tab settings page", "About Kinema")
                icon.name: "help-about"
                category: i18nc("@title:group settings categories", "About")
                page: () => Qt.createComponent(
                    "org.kde.kirigamiaddons.formcard", "AboutPage")
                initialProperties: () => ({
                    aboutData: mainController.aboutData
                })
            }
        ]
    }

    // ---- pageStack and navigation --------------------------------------
    // Primary pages are single top-level surfaces. Detail and helper
    // pages are pushed on top to preserve back navigation, but the PageRow
    // is configured above to behave like a single full-width stack.
    // Discover / Search / Browse unwind to the stack root and replace it.
    // Settings and About open in a separate `KirigamiSettings.ConfigurationView`
    // window and never enter this page stack.
    readonly property string currentNavKey: pageStack.currentItem
        ? (pageStack.currentItem.objectName || "")
        : ""

    Component.onCompleted: root.setTopLevelPage(discoverComp, {})

    function createPage(component, properties) {
        // Kirigami.PageRow accepts Component objects, but on the current
        // Qt/Kirigami stack it creates them with a non-visual helper
        // parent first, which produces noisy "not placed in scene"
        // warnings. Create with a temporary visual parent so the page is
        // in-scene at birth, then detach before handing it to PageRow so it
        // can own insertion without thinking the page is already present.
        const page = component.createObject(root.contentItem,
            properties || {});
        if (page) {
            page.parent = null;
        }
        return page;
    }

    function setTopLevelPage(component, properties) {
        const page = root.createPage(component, properties || {});
        if (!page) {
            return;
        }

        // Do not call pageStack.clear() here: Kirigami's global toolbar
        // briefly sees an empty PageRow and emits binding/type warnings.
        // `PageRow.pop(null)` is documented as unwinding to the first
        // page, but on the current Kirigami/ColumnView stack it can leave
        // the row transiently empty before replace(), which produces
        // "There's no page to replace" and toolbar null-binding noise.
        // Instead make the root page current and let PageRow.replace()
        // remove anything above it while replacing the root in one step.
        if (pageStack.depth === 0) {
            pageStack.push(page, {});
            return;
        }
        pageStack.currentIndex = 0;
        pageStack.replace(page, {});
    }

    function pushCreated(component, properties) {
        const page = root.createPage(component, properties || {});
        if (page) {
            pageStack.push(page, {});
        }
    }

    function showPage(key) {
        if (currentNavKey === key) {
            // Re-clicking the active drawer entry should stay put;
            // the checkable state already pinned it.
            return;
        }
        switch (key) {
        case "discover":
            root.setTopLevelPage(discoverComp, {});
            break;
        case "search":
            root.setTopLevelPage(searchComp, {});
            break;
        case "browse":
            root.setTopLevelPage(browseComp, {});
            break;
        }
    }

    // Page factories. Re-instantiated on each navigation so a
    // returning visit lands on a fresh state. Phases 03–06 swap
    // these out for the real Kirigami pages backed by view-models.
    // Phase 03 brought up the Discover page; phase 04 brought up
    // Search and Browse. Detail pages remain stubbed until phase 05.
    Component { id: discoverComp; DiscoverPage { } }
    Component { id: searchComp;   SearchPage   { } }
    Component { id: browseComp;   BrowsePage   { } }

    // Detail pages are pushed on top of the current nav page so Esc
    // pops back to it with state preserved (the shell-level Esc
    // shortcut handles depth>1 already).
    Component { id: movieDetailComp;  MovieDetailPage  { } }
    Component { id: seriesDetailComp; SeriesDetailPage { } }

    // Subtitles is a pushed helper page. About and Settings open in the
    // ConfigurationView window (see above), not the page stack.
    Component {
        id: subtitlesComp
        SubtitlesPage { }
    }
    Component {
        id: streamsComp
        StreamsPage { }
    }

    // ---- C++-driven flows ----------------------------------------------
    Connections {
        target: mainController

        function onShowSettingsRequested(category) {
            // Open the addons ConfigurationView. Empty category lands on
            // the first module; an explicit category is the
            // `defaultModule` and the dialog opens directly on it.
            configurationView.open(category);
        }
        function onShowSubtitlesRequested() {
            const top = root.pageStack.currentItem;

            // Detail pages expose an inline subtitles panel. The C++ side has
            // already prepared subtitlesVm with the requested playback context.
            // Player requests set attachOnDownload=true and must keep using the
            // standalone page so completed downloads attach back to the player.
            if (!subtitlesVm.attachOnDownload
                && top
                && typeof top.openSubtitlesPanel === "function") {
                top.openSubtitlesPanel();
                return;
            }

            if (top && top.objectName === "subtitles") {
                // Already on the page — the VM has refreshed its context, no
                // further push needed.
                return;
            }
            root.pushCreated(subtitlesComp, {});
        }
        function onPopPageRequested() {
            if (root.pageStack.depth > 1) {
                root.pageStack.pop();
            }
        }
        function onShowAboutRequested() {
            // About lives inside Settings. F1 opens the dialog on the
            // "about" module.
            configurationView.open("about");
        }
        function onFocusSearchRequested() {
            // Prefer focusing the field on the current Search page
            // when one is visible; otherwise navigate to it. The
            // SearchPage exposes `focusSearchField()` for this
            // exact handshake.
            const top = root.pageStack.currentItem;
            if (top && typeof top.focusSearchField === "function") {
                top.focusSearchField();
                return;
            }
            root.showPage("search");
            // After replace() the new page is the top item; it
            // auto-focuses on `Component.onCompleted`, so no extra
            // call is needed here.
        }
        function onNavigateToBrowseRequested() {
            // "Show all →" from a Discover rail. The preset has
            // already been applied to `browseVm` by
            // `MainController::applyBrowsePreset` before this
            // signal fires; we just swap the page row.
            root.showPage("browse");
        }
        function onShowMovieDetailRequested() {
            // Detail pages stack on top of the current nav surface.
            // If we're already showing a movie detail (e.g. user
            // clicked a similar carousel card), push a fresh page
            // so Esc walks back through the breadcrumb of titles.
            root.pushCreated(movieDetailComp, {});
        }
        function onShowSeriesDetailRequested() {
            root.pushCreated(seriesDetailComp, {});
        }
        function onShowStreamsRequested(detailVm) {
            // If we are already showing a streams page, replace its
            // VM binding instead of stacking another one. This
            // happens when a series detail page tap-selects a new
            // episode while the previous streams page is still on
            // top via Esc-back navigation — in practice the page
            // is popped first, so this is just defensive.
            const top = root.pageStack.currentItem;
            if (top && top.objectName === "streams") {
                top.detailVm = detailVm;
                return;
            }
            root.pushCreated(streamsComp, {
                detailVm: detailVm,
                objectName: "streams"
            });
        }
        function onPassiveMessage(text, durationMs) {
            root.showPassiveNotification(text, durationMs);
        }
    }
}
