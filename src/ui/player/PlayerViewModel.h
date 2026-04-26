// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "api/Subtitle.h"
#include "core/MpvChapterList.h"
#include "core/MpvTrackList.h"

#include <QAbstractListModel>
#include <QList>
#include <QObject>
#include <QPointer>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>
#include <QtQmlIntegration/qqmlintegration.h>

namespace kinema::ui::player {

class AudioTracksModel;
class ChaptersModel;
class MpvVideoItem;
class SubtitleSearchModel;
class SubtitleTracksModel;

/**
 * Chrome-state object exposed to the player QML scene as the
 * `playerVm` context property. Owns the audio / subtitle / chapter
 * list models that QML pickers bind against, plus the prompt
 * visibility flags that the C++ side toggles in response to
 * `PlaybackController` decisions (resume prompt, next-episode
 * banner, skip pill, cheat sheet).
 *
 * Stateful but inert: never calls into mpv directly. The only mpv
 * surface it touches is the `MpvVideoItem` it observes for track
 * and chapter list updates, and the action signals it re-emits
 * (`audioPicked` / `subtitlePicked` / …) which `PlayerWindow`
 * forwards to `MpvVideoItem` setters.
 *
 * Test surface: every observable transition is driven through a
 * public slot, so `tst_player_view_model` can assert visibility,
 * countdowns, and signal emissions without needing libmpv.
 */
class PlayerViewModel : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("PlayerViewModel is created in C++ and exposed "
                    "to QML as the playerVm context property.")

    // ---- Track / chapter models ---------------------------------------
    Q_PROPERTY(QAbstractListModel* audioTracks READ audioTracks CONSTANT)
    Q_PROPERTY(QAbstractListModel* subtitleTracks
        READ subtitleTracks CONSTANT)
    Q_PROPERTY(QAbstractListModel* chapters READ chapters CONSTANT)
    Q_PROPERTY(int currentAudioId READ currentAudioId
        NOTIFY currentAudioIdChanged)
    Q_PROPERTY(int currentSubtitleId READ currentSubtitleId
        NOTIFY currentSubtitleIdChanged)

    // ---- Title / subtitle / kind --------------------------------------
    Q_PROPERTY(QString mediaTitle READ mediaTitle
        NOTIFY mediaTitleChanged)
    Q_PROPERTY(QString mediaSubtitle READ mediaSubtitle
        NOTIFY mediaSubtitleChanged)
    Q_PROPERTY(QString mediaKind READ mediaKind
        NOTIFY mediaKindChanged)
    Q_PROPERTY(QStringList mediaChips READ mediaChips
        NOTIFY mediaChipsChanged)

    // ---- Prompts ------------------------------------------------------
    Q_PROPERTY(bool resumeVisible READ resumeVisible
        NOTIFY resumeVisibleChanged)
    Q_PROPERTY(qint64 resumeSeconds READ resumeSeconds
        NOTIFY resumeSecondsChanged)

    Q_PROPERTY(bool nextEpisodeVisible READ nextEpisodeVisible
        NOTIFY nextEpisodeVisibleChanged)
    Q_PROPERTY(QString nextEpisodeTitle READ nextEpisodeTitle
        NOTIFY nextEpisodeTitleChanged)
    Q_PROPERTY(QString nextEpisodeSubtitle READ nextEpisodeSubtitle
        NOTIFY nextEpisodeSubtitleChanged)
    Q_PROPERTY(int nextEpisodeCountdown READ nextEpisodeCountdown
        NOTIFY nextEpisodeCountdownChanged)

    Q_PROPERTY(bool skipVisible READ skipVisible
        NOTIFY skipVisibleChanged)
    Q_PROPERTY(QString skipKind READ skipKind NOTIFY skipKindChanged)
    Q_PROPERTY(QString skipLabel READ skipLabel NOTIFY skipLabelChanged)
    Q_PROPERTY(qint64 skipStartSec READ skipStartSec
        NOTIFY skipStartSecChanged)
    Q_PROPERTY(qint64 skipEndSec READ skipEndSec
        NOTIFY skipEndSecChanged)

    // ---- Info overlay (keyboard shortcuts + stream details) ---------
    // The overlay collapses two things the user wants discoverable
    // without leaving the player: the keyboard shortcut cheat sheet
    // (sectioned, populated by `core::shortcuts::renderSections`)
    // and an "about this stream" map populated from
    // `MpvVideoItem::VideoStats` plus the load context. QVariantList
    // / QVariantMap is enough for ~16 static rows + a flat dict;
    // a full QAbstractListModel would be overkill.
    Q_PROPERTY(bool infoOverlayVisible READ infoOverlayVisible
        NOTIFY infoOverlayVisibleChanged)
    Q_PROPERTY(QVariantList shortcutSections READ shortcutSections
        NOTIFY shortcutSectionsChanged)
    Q_PROPERTY(QVariantMap streamInfo READ streamInfo
        NOTIFY streamInfoChanged)

    // ---- Subtitle search & download ---------------------------------
    // The picker sheet opens against `subtitleSearchActive`; hits
    // come through `subtitleSearchModel`. Hard-gated by
    // `subtitleDownloadEnabled`, which the controller flips after a
    // successful login.
    Q_PROPERTY(bool subtitleSearchActive READ subtitleSearchActive
        NOTIFY subtitleSearchActiveChanged)
    Q_PROPERTY(QAbstractListModel* subtitleSearchModel
        READ subtitleSearchModel CONSTANT)
    Q_PROPERTY(bool subtitleDownloadEnabled READ subtitleDownloadEnabled
        NOTIFY subtitleDownloadEnabledChanged)
    Q_PROPERTY(QString subtitleSearchError READ subtitleSearchError
        NOTIFY subtitleSearchErrorChanged)
    Q_PROPERTY(bool subtitleSearchSheetOpen READ subtitleSearchSheetOpen
        NOTIFY subtitleSearchSheetOpenChanged)

public:
    explicit PlayerViewModel(QObject* parent = nullptr);
    ~PlayerViewModel() override;

    /// Connect to the video item's signals so the audio / subtitle /
    /// chapter models stay in sync with mpv. May be called at most
    /// once per lifetime; subsequent calls are ignored.
    void attach(MpvVideoItem* video);

    // ---- Property accessors ------------------------------------------

    QAbstractListModel* audioTracks() const;
    QAbstractListModel* subtitleTracks() const;
    QAbstractListModel* chapters() const;
    int currentAudioId() const noexcept { return m_currentAudioId; }
    int currentSubtitleId() const noexcept { return m_currentSubtitleId; }

    QString mediaTitle() const { return m_mediaTitle; }
    QString mediaSubtitle() const { return m_mediaSubtitle; }
    QString mediaKind() const { return m_mediaKind; }
    QStringList mediaChips() const { return m_mediaChips; }

    bool resumeVisible() const noexcept { return m_resumeVisible; }
    qint64 resumeSeconds() const noexcept { return m_resumeSeconds; }

    bool nextEpisodeVisible() const noexcept
    {
        return m_nextEpisodeVisible;
    }
    QString nextEpisodeTitle() const { return m_nextEpisodeTitle; }
    QString nextEpisodeSubtitle() const { return m_nextEpisodeSubtitle; }
    int nextEpisodeCountdown() const noexcept
    {
        return m_nextEpisodeCountdown;
    }

    bool skipVisible() const noexcept { return m_skipVisible; }
    QString skipKind() const { return m_skipKind; }
    QString skipLabel() const { return m_skipLabel; }
    qint64 skipStartSec() const noexcept { return m_skipStartSec; }
    qint64 skipEndSec() const noexcept { return m_skipEndSec; }

    bool infoOverlayVisible() const noexcept
    {
        return m_infoOverlayVisible;
    }
    QVariantList shortcutSections() const { return m_shortcutSections; }
    QVariantMap streamInfo() const { return m_streamInfo; }

    bool subtitleSearchActive() const noexcept { return m_subtitleSearchActive; }
    QAbstractListModel* subtitleSearchModel() const;
    bool subtitleDownloadEnabled() const noexcept
    {
        return m_subtitleDownloadEnabled;
    }
    QString subtitleSearchError() const { return m_subtitleSearchError; }
    bool subtitleSearchSheetOpen() const noexcept
    {
        return m_subtitleSearchSheetOpen;
    }

public Q_SLOTS:
    // Slots driven by `PlayerWindow` (forwarding from controllers).
    void setMediaContext(const QString& title,
        const QString& subtitle, const QString& kind);
    void setMediaChips(const QStringList& chips);
    void setShortcutSections(const QVariantList& sections);
    void setStreamInfo(const QVariantMap& info);

    void showResume(qint64 seconds);
    void hideResume();

    void showNextEpisode(const QString& title,
        const QString& subtitle, int countdownSec);
    void updateNextEpisodeCountdown(int seconds);
    void hideNextEpisode();

    void showSkip(const QString& kind, const QString& label,
        qint64 startSec, qint64 endSec);
    void hideSkip();

    void toggleInfoOverlay();
    void setInfoOverlayVisible(bool on);

    // Subtitle-search state mutators (driven by SubtitleController
    // signals via MainWindow).
    void setSubtitleSearchActive(bool on);
    void setSubtitleDownloadEnabled(bool on);
    void setSubtitleSearchError(const QString& msg);
    /// Apply hits + flag sets to the search model.
    void updateSubtitleSearchModel(const QList<api::SubtitleHit>& hits,
        const QSet<QString>& cached,
        const QSet<QString>& activePaths);
    /// Open / close the picker sheet.
    void openSubtitleSearchSheet();
    void closeSubtitleSearchSheet();

    // Subtitle-search action slots invoked from QML.
    Q_INVOKABLE void requestSubtitleSearch(const QStringList& languages,
        const QString& hearingImpaired,
        const QString& foreignPartsOnly,
        const QString& releaseFilter);
    Q_INVOKABLE void requestSubtitleDownload(const QString& fileId);
    Q_INVOKABLE void requestSubtitleSearchSheet();
    Q_INVOKABLE void requestLocalSubtitleFile();
    Q_INVOKABLE void attachExternalSubtitle(const QString& path,
        const QString& title, const QString& lang, bool select);

    // Slots invoked by QML (button clicks, picker selections). Each
    // emits a matching signal `PlayerWindow` re-emits on its own
    // surface so `PlaybackController` only sees the window.
    void requestResumeAccept();
    void requestResumeDecline();
    void requestSkip();
    void requestNextEpisodeAccept();
    void requestNextEpisodeCancel();
    void pickAudio(int trackId);
    void pickSubtitle(int trackId);
    void pickSpeed(double factor);
    void requestClose();
    void requestToggleFullscreen();

Q_SIGNALS:
    // Property notifications.
    void mediaTitleChanged();
    void mediaSubtitleChanged();
    void mediaKindChanged();
    void mediaChipsChanged();
    void currentAudioIdChanged();
    void currentSubtitleIdChanged();
    void resumeVisibleChanged();
    void resumeSecondsChanged();
    void nextEpisodeVisibleChanged();
    void nextEpisodeTitleChanged();
    void nextEpisodeSubtitleChanged();
    void nextEpisodeCountdownChanged();
    void skipVisibleChanged();
    void skipKindChanged();
    void skipLabelChanged();
    void skipStartSecChanged();
    void skipEndSecChanged();
    void infoOverlayVisibleChanged();
    void shortcutSectionsChanged();
    void streamInfoChanged();

    // User-action signals re-emitted by `PlayerWindow`. The eight
    // signals `PlaybackController` consumes drive resume, skip,
    // next-episode, and picker flows.
    void resumeAccepted();
    void resumeDeclined();
    void skipRequested();
    void nextEpisodeAccepted();
    void nextEpisodeCancelled();
    void audioPicked(int trackId);
    void subtitlePicked(int trackId);
    void speedPicked(double factor);
    void closeRequested();
    void fullscreenToggleRequested();

    // Subtitle-search action signals.
    void subtitleSearchRequested(const QStringList& languages,
        const QString& hearingImpaired,
        const QString& foreignPartsOnly,
        const QString& releaseFilter);
    void subtitleDownloadRequested(const QString& fileId);
    void subtitleSearchSheetRequested();
    void localSubtitleFileRequested();

    // Property notifications for the new properties.
    void subtitleSearchActiveChanged();
    void subtitleDownloadEnabledChanged();
    void subtitleSearchErrorChanged();
    void subtitleSearchSheetOpenChanged();

private Q_SLOTS:
    void onTrackListChanged(const core::tracks::TrackList& tracks);
    void onChaptersChanged(const core::chapters::ChapterList& chapters);
    void onAudioTrackChanged(int id);
    void onSubtitleTrackChanged(int id);

private:
    QPointer<MpvVideoItem> m_video;

    AudioTracksModel* m_audioModel = nullptr;
    SubtitleTracksModel* m_subtitleModel = nullptr;
    ChaptersModel* m_chaptersModel = nullptr;
    SubtitleSearchModel* m_subtitleSearchModel = nullptr;

    int m_currentAudioId = -1;
    int m_currentSubtitleId = -1;

    QString m_mediaTitle;
    QString m_mediaSubtitle;
    QString m_mediaKind;
    QStringList m_mediaChips;

    bool m_resumeVisible = false;
    qint64 m_resumeSeconds = 0;

    bool m_nextEpisodeVisible = false;
    QString m_nextEpisodeTitle;
    QString m_nextEpisodeSubtitle;
    int m_nextEpisodeCountdown = 0;

    bool m_skipVisible = false;
    QString m_skipKind;
    QString m_skipLabel;
    qint64 m_skipStartSec = 0;
    qint64 m_skipEndSec = 0;

    bool m_infoOverlayVisible = false;
    QVariantList m_shortcutSections;
    QVariantMap m_streamInfo;

    bool m_subtitleSearchActive = false;
    bool m_subtitleDownloadEnabled = false;
    bool m_subtitleSearchSheetOpen = false;
    QString m_subtitleSearchError;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
