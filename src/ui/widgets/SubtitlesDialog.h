// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QDialog>

class QButtonGroup;
class QLabel;
class QLineEdit;
class QPushButton;
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

class LanguageChipBar;
class SubtitleResultDelegate;
class SubtitleResultsModel;

/**
 * Qt Widgets dialog for searching and downloading OpenSubtitles
 * results. Reused across both windows: opened from
 * `StreamsPanel`'s "Subtitles…" action (pre-play) and from the
 * player's `SubtitlePicker → Download…` entry (mid-play).
 *
 * The dialog is constructed once by `MainWindow` and `setMedia()` is
 * called on every open to swap the playback context. The dialog
 * never owns the controller; it consumes its signals while open and
 * disconnects on close to avoid double-paints when the controller
 * keeps fielding events for another media item.
 */
class SubtitlesDialog : public QDialog
{
    Q_OBJECT
public:
    SubtitlesDialog(controllers::SubtitleController* controller,
        config::SubtitleSettings& settings,
        QWidget* parent = nullptr);

    /// Set the media this dialog targets. Updates the title strip
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
    void onRowActivated(const QModelIndex& index);
    void onViewClicked(const QModelIndex& index);
    void onLocalFile();
    void refreshState();
    void refreshGate();
    void refreshStatusLine();

    QString currentHi() const;
    QString currentFpo() const;
    void setHi(const QString& mode);
    void setFpo(const QString& mode);

    controllers::SubtitleController* m_controller;
    config::SubtitleSettings& m_settings;

    api::PlaybackContext m_context;
    bool m_attachOnDownload = false;

    QLabel* m_titleLabel {};
    LanguageChipBar* m_chipBar {};
    QButtonGroup* m_hiGroup {};
    QButtonGroup* m_fpoGroup {};
    QLineEdit* m_releaseEdit {};
    QPushButton* m_searchButton {};
    QPushButton* m_resetButton {};
    QPushButton* m_openFileButton {};
    QPushButton* m_closeButton {};

    QStackedWidget* m_resultsStack {};
    ui::StateWidget* m_state {};
    QTreeView* m_view {};
    SubtitleResultsModel* m_model {};
    SubtitleResultDelegate* m_delegate {};

    QLabel* m_statusLabel {};
};

} // namespace kinema::ui::widgets
