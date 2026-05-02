// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Sort picker for the Streams page. Replaces the old inline
// `FilterMenuButton` + adjacent "Descending" toggle so:
//
//   * The trigger label on the page header can stay short — most
//     importantly, `Smart` no longer has to spell out
//     "(cached, quality, seeders)" in the bar.
//   * The ascending / descending choice lives next to the mode it
//     applies to, with a description that explains why `Smart`
//     ignores it.
//
// Live-update semantics: every radio writes back to the VM
// immediately on `onToggled`. The footer carries a single Close
// button.
//
// Implementation note: built on `FormCard.FormCardDialog` rather
// than `Kirigami.Dialog` + a nested `ColumnLayout` of `FormCard`s.
// The latter binding-loops on the dialog's centring `y` because
// the dialog width is bound back into the layout via `FormCard`'s
// internal `Layout.fillWidth`. `FormCardDialog` is the addon's
// primitive for exactly this case (delegates as direct children,
// `FormDelegateSeparator` between groups).
//
// Caller shape:
//
//     StreamSortDialog {
//         id: streamSortDialog
//         vm: page.detailVm
//     }
FormCard.FormCardDialog {
    id: root

    /// View-model exposing `sortMode` (int / `StreamsListModel.SortMode`)
    /// and `sortDescending` (bool). Defaults to `movieDetailVm`; the
    /// series page rebinds it.
    property var vm: movieDetailVm

    // Disabled when Smart is selected — Smart fixes the row shape
    // internally and ignores the toggle.
    readonly property bool directionEnabled: root.vm
        && root.vm.sortMode !== StreamsListModel.Smart

    title: i18nc("@title:dialog stream sort", "Sort streams")
    standardButtons: QQC2.Dialog.Close

    FormCard.FormSectionText {
        text: i18nc("@title:group sort mode", "Order by")
    }
    FormCard.FormRadioDelegate {
        text: i18nc("@option:radio sort streams", "Smart")
        description: i18nc("@info sort mode hint",
            "Cached on Real-Debrid first, then by quality, "
            + "then by seeders. Ignores the order toggle.")
        checked: root.vm
            && root.vm.sortMode === StreamsListModel.Smart
        onToggled: if (checked && root.vm) {
            root.vm.sortMode = StreamsListModel.Smart;
        }
    }
    FormCard.FormRadioDelegate {
        text: i18nc("@option:radio sort streams", "Seeders")
        description: i18nc("@info sort mode hint",
            "Healthier swarms tend to start faster.")
        checked: root.vm
            && root.vm.sortMode === StreamsListModel.Seeders
        onToggled: if (checked && root.vm) {
            root.vm.sortMode = StreamsListModel.Seeders;
        }
    }
    FormCard.FormRadioDelegate {
        text: i18nc("@option:radio sort streams", "Size")
        checked: root.vm
            && root.vm.sortMode === StreamsListModel.Size
        onToggled: if (checked && root.vm) {
            root.vm.sortMode = StreamsListModel.Size;
        }
    }
    FormCard.FormRadioDelegate {
        text: i18nc("@option:radio sort streams", "Quality")
        checked: root.vm
            && root.vm.sortMode === StreamsListModel.Quality
        onToggled: if (checked && root.vm) {
            root.vm.sortMode = StreamsListModel.Quality;
        }
    }
    FormCard.FormRadioDelegate {
        text: i18nc("@option:radio sort streams", "Provider")
        checked: root.vm
            && root.vm.sortMode === StreamsListModel.Provider
        onToggled: if (checked && root.vm) {
            root.vm.sortMode = StreamsListModel.Provider;
        }
    }
    FormCard.FormRadioDelegate {
        text: i18nc("@option:radio sort streams", "Release name")
        checked: root.vm
            && root.vm.sortMode === StreamsListModel.ReleaseName
        onToggled: if (checked && root.vm) {
            root.vm.sortMode = StreamsListModel.ReleaseName;
        }
    }

    FormCard.FormDelegateSeparator {}

    FormCard.FormSectionText {
        text: i18nc("@title:group sort direction", "Order")
    }
    FormCard.FormRadioDelegate {
        text: i18nc("@option:radio sort direction", "Descending")
        description: i18nc("@info sort direction hint",
            "Largest values first.")
        enabled: root.directionEnabled
        checked: root.vm && root.vm.sortDescending
        onToggled: if (checked && root.vm) {
            root.vm.sortDescending = true;
        }
    }
    FormCard.FormRadioDelegate {
        text: i18nc("@option:radio sort direction", "Ascending")
        description: i18nc("@info sort direction hint",
            "Smallest values first.")
        enabled: root.directionEnabled
        checked: root.vm && !root.vm.sortDescending
        onToggled: if (checked && root.vm) {
            root.vm.sortDescending = false;
        }
    }
}
