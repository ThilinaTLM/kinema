// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QObject>
#include <QString>
#include <QStringList>
#include <QVariantList>

namespace kinema::config {
class SubtitleSettings;
}

namespace kinema::controllers {
class SubtitleController;
}

namespace kinema::ui::qml {

class SubtitleResultsModel;

/**
 * QML-side view-model for the Subtitles page.
 *
 * Wraps `controllers::SubtitleController` and `SubtitleResultsModel`.
 * Owns no business logic of its own: every action is forwarded to the
 * controller, every piece of derived state is read back through
 * `NOTIFY` signals.
 *
 * State machine — exposed as the single `state` string property
 * for `StackLayout.currentIndex` switching in QML:
 *
 *   - "notconfigured" — controller's downloadEnabled gate is off
 *     (no OpenSubtitles credentials in keychain)
 *   - "idle"          — gate is on but no search has run yet
 *   - "loading"       — `controller.isSearching()`
 *   - "error"         — last search failed, errorText non-empty
 *   - "empty"         — search ran but returned zero hits
 *   - "ready"         — at least one hit
 *
 * Primary action label / enabled state mirrors `SubtitlesDialog`'s
 * `updatePrimaryAction` exactly: Download / Use / Re-attach.
 */
class SubtitlesViewModel : public QObject
{
    Q_OBJECT
    // No `QML_ELEMENT` here — this type lives in `kinema_core`,
    // not in the `dev.tlmtech.kinema.app` QML module. It's exposed
    // to QML via `qmlRegisterUncreatableType` (in
    // `MainController::exposeContextProperties`) and as a context
    // property `subtitlesVm`.

    Q_PROPERTY(QString contextTitle READ contextTitle NOTIFY contextChanged)

    Q_PROPERTY(QStringList languages READ languages WRITE setLanguages NOTIFY filtersChanged)
    Q_PROPERTY(QString hi  READ hi  WRITE setHi  NOTIFY filtersChanged)
    Q_PROPERTY(QString fpo READ fpo WRITE setFpo NOTIFY filtersChanged)
    Q_PROPERTY(QString release READ release WRITE setRelease NOTIFY filtersChanged)

    Q_PROPERTY(QObject* results READ results CONSTANT)

    Q_PROPERTY(QString state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString errorText READ errorText NOTIFY stateChanged)
    Q_PROPERTY(QString statusLine READ statusLine NOTIFY statusLineChanged)

    Q_PROPERTY(int selectedRow READ selectedRow WRITE setSelectedRow NOTIFY selectedRowChanged)
    Q_PROPERTY(QString primaryActionText READ primaryActionText NOTIFY primarySemanticsChanged)
    Q_PROPERTY(QString primaryActionIcon READ primaryActionIcon NOTIFY primarySemanticsChanged)
    Q_PROPERTY(bool primaryActionEnabled READ primaryActionEnabled NOTIFY primarySemanticsChanged)

    Q_PROPERTY(bool attachOnDownload READ attachOnDownload NOTIFY attachOnDownloadChanged)

    /// Static helper data for QML's language picker.
    /// `[{ code: "eng", display: "English" }, ...]`.
    Q_PROPERTY(QVariantList commonLanguages READ commonLanguages CONSTANT)

public:
    SubtitlesViewModel(controllers::SubtitleController* controller,
        config::SubtitleSettings& settings,
        QObject* parent = nullptr);

    QString contextTitle() const { return m_contextTitle; }
    QStringList languages() const { return m_languages; }
    QString hi() const { return m_hi; }
    QString fpo() const { return m_fpo; }
    QString release() const { return m_release; }
    QObject* results() const;
    QVariantList commonLanguages() const;
    Q_INVOKABLE QString languageDisplayName(const QString& code) const;
    QString state() const;
    QString errorText() const;
    QString statusLine() const { return m_statusLine; }
    int selectedRow() const { return m_selectedRow; }
    QString primaryActionText() const { return m_primaryActionText; }
    QString primaryActionIcon() const { return m_primaryActionIcon; }
    bool primaryActionEnabled() const { return m_primaryActionEnabled; }
    bool attachOnDownload() const { return m_attachOnDownload; }

    /// Set the media context the page targets. Resets selection,
    /// repopulates filter defaults from `SubtitleSettings`, kicks
    /// an automatic search if credentials are configured.
    void setMedia(const api::PlaybackContext& ctx);
    /// Switch post-download behaviour. When true, an extra
    /// `downloadCompleted` is emitted with `attachOnDownload`-style
    /// semantics — consumers (the embedded player) sideload the
    /// file. When false (detail-page entry), only the model badges
    /// update.
    void setAttachOnDownload(bool on);

    void setLanguages(const QStringList& codes);
    void setHi(const QString& mode);
    void setFpo(const QString& mode);
    void setRelease(const QString& release);
    void setSelectedRow(int row);

public Q_SLOTS:
    /// Run a search with the current filter state.
    void runSearch();
    /// Restore filters to the saved settings defaults.
    void resetFilters();
    /// Trigger Download / Use / Re-attach for `selectedRow`.
    void runPrimaryAction();
    /// Open the local-file picker (handled in QML via FileDialog —
    /// QML calls `localFileResolved` once the path is known).
    void openLocalFile();
    /// Routed by QML once a `FileDialog` returns a path.
    void localFileResolved(const QUrl& url);
    /// Asks the host shell to push the Settings page on the
    /// "subtitles" category. Used when downloadEnabled is false.
    void openSettings();

Q_SIGNALS:
    /// Always emitted on a successful download. The embedded player
    /// listens via `MainController` and sideloads the file when
    /// `attachOnDownload` was true at request time.
    void downloadCompleted(api::PlaybackKey key,
        QString fileId,
        QString localPath,
        QString language,
        QString languageName);

    /// Local-file pick. Routed to the player when the dialog was
    /// opened from the player chrome, otherwise the consumer can
    /// cache it under the current `PlaybackKey`.
    void localFileChosen(api::PlaybackKey key, QString path);

    /// Asks the QML shell to push the Settings page on the given
    /// category (`MainController::requestSettings("subtitles")`).
    void settingsRequested();
    /// Pop the Subtitles page from the stack (Cancel button).
    void closeRequested();

    void contextChanged();
    void filtersChanged();
    void stateChanged();
    void statusLineChanged();
    void selectedRowChanged();
    void primarySemanticsChanged();
    void attachOnDownloadChanged();

private Q_SLOTS:
    void onHitsChanged();
    void onCacheChanged();
    void onSearchingChanged();
    void onErrorChanged();
    void onDownloadEnabledChanged();
    void onQuotaChanged();
    void onDownloadFinished(const QString& fileId,
        const QString& localPath,
        const QString& language,
        const QString& languageName);
    void onDownloadFailed(const QString& fileId, const QString& reason);

private:
    void updateState();
    void updateStatusLine();
    void updatePrimarySemantics();
    void refreshModelFlags();
    QString formatContextTitle(const api::PlaybackContext& ctx) const;

    controllers::SubtitleController* m_controller;
    config::SubtitleSettings& m_settings;
    SubtitleResultsModel* m_model {};

    api::PlaybackContext m_context;
    QString m_contextTitle;
    bool m_attachOnDownload = false;

    QStringList m_languages;
    QString m_hi = QStringLiteral("off");
    QString m_fpo = QStringLiteral("off");
    QString m_release;

    QString m_state = QStringLiteral("idle");
    QString m_statusLine;
    int m_selectedRow = -1;

    QString m_primaryActionText;
    QString m_primaryActionIcon;
    bool m_primaryActionEnabled = false;
};

} // namespace kinema::ui::qml
