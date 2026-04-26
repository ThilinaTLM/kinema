// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "core/MpvChapterList.h"
#include "core/MpvTrackList.h"

#include <QAbstractListModel>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>
#include <QtQmlIntegration/qqmlintegration.h>

namespace kinema::ui::player {

class AudioTracksModel;
class ChaptersModel;
class MpvVideoItem;
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

    // ---- Subtitle download gating -----------------------------------
    // The QML picker shows a "Download subtitle…" footer entry that
    // bubbles through `requestSubtitlesDialog()` to MainWindow,
    // which opens the Qt Widgets `SubtitlesDialog`. The gate flag
    // mirrors `SubtitleController::downloadEnabled()` so the entry
    // can disable itself when OpenSubtitles isn't configured.
    Q_PROPERTY(bool subtitleDownloadEnabled READ subtitleDownloadEnabled
        NOTIFY subtitleDownloadEnabledChanged)

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

    bool subtitleDownloadEnabled() const noexcept
    {
        return m_subtitleDownloadEnabled;
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

    // Subtitle download gate (driven by SubtitleController via
    // MainWindow). Controls whether the QML "Download subtitle…"
    // entry is enabled.
    void setSubtitleDownloadEnabled(bool on);

    // Subtitle action slots invoked from QML.
    /// Asks MainWindow to open the Qt Widgets `SubtitlesDialog`
    /// parented to this player window.
    Q_INVOKABLE void requestSubtitlesDialog();
    /// Asks MainWindow to open a QFileDialog for sideloading a
    /// local subtitle file directly into mpv.
    Q_INVOKABLE void requestLocalSubtitleFile();
    /// Sideload an external subtitle file (called from MainWindow
    /// after the dialog or QFileDialog returns a path).
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

    /// Emitted from the QML "Download subtitle…" picker entry.
    /// MainWindow opens the Qt Widgets `SubtitlesDialog` in
    /// response.
    void subtitlesDialogRequested();
    /// Emitted from the QML "Open subtitle file…" picker entry.
    /// MainWindow opens a QFileDialog and (on accept) calls
    /// `attachExternalSubtitle()` back on this VM.
    void localSubtitleFileRequested();

    void subtitleDownloadEnabledChanged();

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

    bool m_subtitleDownloadEnabled = false;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
