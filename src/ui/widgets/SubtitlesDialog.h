// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QDialog>

class KCollapsibleGroupBox;
class QButtonGroup;
class QDialogButtonBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QSortFilterProxyModel;
class QStackedWidget;
class QTreeView;

namespace kinema::config {
class SubtitleSettings;
}

namespace kinema::controllers {
class SubtitleController;
}

namespace kinema::ui {
class StateWidget;
}

namespace kinema::ui::widgets {

class LanguagePickerButton;
class SubtitleResultsModel;

/**
 * Native Breeze dialog for searching and downloading OpenSubtitles
 * results. Reused across both windows: opened from
 * `StreamsPanel`'s "Subtitles…" action (pre-play) and from the
 * player's `SubtitlePicker → Download…` entry (mid-play).
 *
 * The dialog is constructed once by `MainWindow` and `setMedia()` is
 * called on every open to swap the playback context. The dialog
 * never owns the controller; it consumes its signals while open and
 * disconnects on close to avoid double-paints when the controller
 * keeps fielding events for another media item.
 *
 * UI shape: window title carries the media; body is a `QFormLayout`
 * for filters, a multi-column `QTreeView` for results (no custom
 * delegate), and a `QDialogButtonBox` footer with a selection-driven
 * primary action (`Download` / `Use` / `Re-attach`).
 */
class SubtitlesDialog : public QDialog
{
    Q_OBJECT
public:
    SubtitlesDialog(controllers::SubtitleController* controller,
        config::SubtitleSettings& settings,
        QWidget* parent = nullptr);

    /// Set the media this dialog targets. Updates the window title
    /// and stashes the playback key for download requests. Safe to
    /// call repeatedly; clears the previous result list.
    void setMedia(const api::PlaybackContext& ctx);

    /// Switch the dialog's "post-download" behaviour:
    ///   - true  → an `attachRequested(path, lang, languageName)`
    ///             signal fires after every successful download so
    ///             the player can sideload the file immediately.
    ///   - false → the dialog only updates its row badges; the
    ///             caller takes care of `rememberedSubtitleLang`.
    void setAttachOnDownload(bool on);

Q_SIGNALS:
    /// Fired after a successful download. Always emitted; the
    /// `attachOnDownload` flag only controls whether the receiver
    /// should auto-load the file. Receivers that don't need the
    /// content can ignore it.
    void downloadCompleted(const api::PlaybackKey& key,
        const QString& fileId,
        const QString& localPath,
        const QString& language,
        const QString& languageName);

    /// Fired when the user picks a local file via the "Open file…"
    /// footer button. The dialog hands the path back so MainWindow
    /// can sideload it (player) or cache it under the current
    /// PlaybackKey (detail pane) — both are the caller's concern.
    void localFileChosen(const api::PlaybackKey& key,
        const QString& path);

    /// User clicked "Open settings…" on the not-configured state.
    /// MainWindow opens the settings dialog on this signal.
    void settingsRequested();

protected:
    void showEvent(QShowEvent* event) override;
    void hideEvent(QHideEvent* event) override;

private:
    void buildUi();
    void wireController();
    void unwireController();

    void runSearch();
    void runPrimaryAction();
    void onLocalFile();
    void refreshState();
    void refreshGate();
    void refreshStatusLine();
    void updatePrimaryAction();

    /// Open the advanced-filters collapsible if the current HI/FPO
    /// state is non-default, so users with a saved non-`off` preset
    /// see those controls without hunting. Never auto-collapses.
    void syncAdvancedExpansion();

    QString currentHi() const;
    QString currentFpo() const;
    void setHi(const QString& mode);
    void setFpo(const QString& mode);

    /// Map the view's `currentIndex()` to the source model's
    /// column-0 index (where custom roles resolve). Returns an
    /// invalid index when nothing is selected.
    QModelIndex currentSourceIndex() const;

    controllers::SubtitleController* m_controller;
    config::SubtitleSettings& m_settings;

    api::PlaybackContext m_context;
    bool m_attachOnDownload = false;

    LanguagePickerButton* m_languagePicker {};
    QButtonGroup* m_hiGroup {};
    QButtonGroup* m_fpoGroup {};
    QLineEdit* m_releaseEdit {};
    QPushButton* m_searchButton {};
    QPushButton* m_resetButton {};
    KCollapsibleGroupBox* m_advancedBox {};

    QStackedWidget* m_resultsStack {};
    ui::StateWidget* m_state {};
    QTreeView* m_view {};
    SubtitleResultsModel* m_model {};
    QSortFilterProxyModel* m_proxy {};

    QLabel* m_statusLabel {};
    QDialogButtonBox* m_buttonBox {};
    QPushButton* m_primaryButton {};
    QPushButton* m_openFileButton {};
    QPushButton* m_settingsButton {};
};

} // namespace kinema::ui::widgets
