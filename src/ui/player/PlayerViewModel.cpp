// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/PlayerViewModel.h"

#include "ui/player/AudioTracksModel.h"
#include "ui/player/ChaptersModel.h"
#include "ui/player/MpvVideoItem.h"
#include "ui/player/SubtitleTracksModel.h"

namespace kinema::ui::player {

PlayerViewModel::PlayerViewModel(QObject* parent)
    : QObject(parent)
    , m_audioModel(new AudioTracksModel(this))
    , m_subtitleModel(new SubtitleTracksModel(this))
    , m_chaptersModel(new ChaptersModel(this))
{
}

PlayerViewModel::~PlayerViewModel() = default;

void PlayerViewModel::attach(MpvVideoItem* video)
{
    if (m_video || !video) {
        return;
    }
    m_video = video;
    connect(video, &MpvVideoItem::trackListChanged,
        this, &PlayerViewModel::onTrackListChanged);
    connect(video, &MpvVideoItem::chaptersChanged,
        this, &PlayerViewModel::onChaptersChanged);
    connect(video, &MpvVideoItem::audioTrackChanged,
        this, &PlayerViewModel::onAudioTrackChanged);
    connect(video, &MpvVideoItem::subtitleTrackChanged,
        this, &PlayerViewModel::onSubtitleTrackChanged);
    // Seed the models with whatever the item already cached. This
    // covers the case where the item received track-list events
    // before attach() was called.
    onTrackListChanged(video->tracks());
    onChaptersChanged(video->chapters());
    onAudioTrackChanged(video->audioTrackId());
    onSubtitleTrackChanged(video->subtitleTrackId());
}

QAbstractListModel* PlayerViewModel::audioTracks() const
{
    return m_audioModel;
}

QAbstractListModel* PlayerViewModel::subtitleTracks() const
{
    return m_subtitleModel;
}

QAbstractListModel* PlayerViewModel::chapters() const
{
    return m_chaptersModel;
}

// ---- Mutators driven by PlayerWindow ------------------------------------

void PlayerViewModel::setMediaContext(const QString& title,
    const QString& subtitle, const QString& kind)
{
    if (m_mediaTitle != title) {
        m_mediaTitle = title;
        Q_EMIT mediaTitleChanged();
    }
    if (m_mediaSubtitle != subtitle) {
        m_mediaSubtitle = subtitle;
        Q_EMIT mediaSubtitleChanged();
    }
    if (m_mediaKind != kind) {
        m_mediaKind = kind;
        Q_EMIT mediaKindChanged();
    }
}

void PlayerViewModel::setMediaChips(const QStringList& chips)
{
    if (m_mediaChips == chips) {
        return;
    }
    m_mediaChips = chips;
    Q_EMIT mediaChipsChanged();
}

void PlayerViewModel::setShortcutSections(const QVariantList& sections)
{
    if (m_shortcutSections == sections) {
        return;
    }
    m_shortcutSections = sections;
    Q_EMIT shortcutSectionsChanged();
}

void PlayerViewModel::setStreamInfo(const QVariantMap& info)
{
    if (m_streamInfo == info) {
        return;
    }
    m_streamInfo = info;
    Q_EMIT streamInfoChanged();
}

void PlayerViewModel::showResume(qint64 seconds)
{
    if (m_resumeSeconds != seconds) {
        m_resumeSeconds = seconds;
        Q_EMIT resumeSecondsChanged();
    }
    if (!m_resumeVisible) {
        m_resumeVisible = true;
        Q_EMIT resumeVisibleChanged();
    }
}

void PlayerViewModel::hideResume()
{
    if (!m_resumeVisible) {
        return;
    }
    m_resumeVisible = false;
    Q_EMIT resumeVisibleChanged();
}

void PlayerViewModel::showNextEpisode(const QString& title,
    const QString& subtitle, int countdownSec)
{
    if (m_nextEpisodeTitle != title) {
        m_nextEpisodeTitle = title;
        Q_EMIT nextEpisodeTitleChanged();
    }
    if (m_nextEpisodeSubtitle != subtitle) {
        m_nextEpisodeSubtitle = subtitle;
        Q_EMIT nextEpisodeSubtitleChanged();
    }
    if (m_nextEpisodeCountdown != countdownSec) {
        m_nextEpisodeCountdown = countdownSec;
        Q_EMIT nextEpisodeCountdownChanged();
    }
    if (!m_nextEpisodeVisible) {
        m_nextEpisodeVisible = true;
        Q_EMIT nextEpisodeVisibleChanged();
    }
}

void PlayerViewModel::updateNextEpisodeCountdown(int seconds)
{
    if (m_nextEpisodeCountdown == seconds) {
        return;
    }
    m_nextEpisodeCountdown = seconds;
    Q_EMIT nextEpisodeCountdownChanged();
}

void PlayerViewModel::hideNextEpisode()
{
    if (!m_nextEpisodeVisible) {
        return;
    }
    m_nextEpisodeVisible = false;
    Q_EMIT nextEpisodeVisibleChanged();
}

void PlayerViewModel::showSkip(const QString& kind,
    const QString& label, qint64 startSec, qint64 endSec)
{
    if (m_skipKind != kind) {
        m_skipKind = kind;
        Q_EMIT skipKindChanged();
    }
    if (m_skipLabel != label) {
        m_skipLabel = label;
        Q_EMIT skipLabelChanged();
    }
    if (m_skipStartSec != startSec) {
        m_skipStartSec = startSec;
        Q_EMIT skipStartSecChanged();
    }
    if (m_skipEndSec != endSec) {
        m_skipEndSec = endSec;
        Q_EMIT skipEndSecChanged();
    }
    if (!m_skipVisible) {
        m_skipVisible = true;
        Q_EMIT skipVisibleChanged();
    }
}

void PlayerViewModel::hideSkip()
{
    if (!m_skipVisible) {
        return;
    }
    m_skipVisible = false;
    Q_EMIT skipVisibleChanged();
}

void PlayerViewModel::toggleInfoOverlay()
{
    setInfoOverlayVisible(!m_infoOverlayVisible);
}

void PlayerViewModel::setInfoOverlayVisible(bool on)
{
    if (m_infoOverlayVisible == on) {
        return;
    }
    m_infoOverlayVisible = on;
    Q_EMIT infoOverlayVisibleChanged();
}

// ---- Subtitle download gate + dialog hand-off --------------------------

void PlayerViewModel::setSubtitleDownloadEnabled(bool on)
{
    if (m_subtitleDownloadEnabled == on) {
        return;
    }
    m_subtitleDownloadEnabled = on;
    Q_EMIT subtitleDownloadEnabledChanged();
}

void PlayerViewModel::requestSubtitlesDialog()
{
    Q_EMIT subtitlesDialogRequested();
}

void PlayerViewModel::requestLocalSubtitleFile()
{
    Q_EMIT localSubtitleFileRequested();
}

void PlayerViewModel::attachExternalSubtitle(const QString& path,
    const QString& title, const QString& lang, bool select)
{
    if (!m_video || path.isEmpty()) {
        return;
    }
    m_video->addSubtitleFile(path, title, lang, select);
}

// ---- QML-side actions ---------------------------------------------------

void PlayerViewModel::requestResumeAccept()
{
    Q_EMIT resumeAccepted();
}

void PlayerViewModel::requestResumeDecline()
{
    Q_EMIT resumeDeclined();
}

void PlayerViewModel::requestSkip()
{
    Q_EMIT skipRequested();
}

void PlayerViewModel::requestNextEpisodeAccept()
{
    Q_EMIT nextEpisodeAccepted();
}

void PlayerViewModel::requestNextEpisodeCancel()
{
    Q_EMIT nextEpisodeCancelled();
}

void PlayerViewModel::pickAudio(int trackId)
{
    Q_EMIT audioPicked(trackId);
}

void PlayerViewModel::pickSubtitle(int trackId)
{
    Q_EMIT subtitlePicked(trackId);
}

void PlayerViewModel::pickSpeed(double factor)
{
    Q_EMIT speedPicked(factor);
}

void PlayerViewModel::requestClose()
{
    Q_EMIT closeRequested();
}

void PlayerViewModel::requestToggleFullscreen()
{
    Q_EMIT fullscreenToggleRequested();
}

// ---- Mirror MpvVideoItem signals ---------------------------------------

void PlayerViewModel::onTrackListChanged(
    const core::tracks::TrackList& tracks)
{
    m_audioModel->reset(tracks);
    m_subtitleModel->reset(tracks);
}

void PlayerViewModel::onChaptersChanged(
    const core::chapters::ChapterList& chapters)
{
    m_chaptersModel->reset(chapters);
}

void PlayerViewModel::onAudioTrackChanged(int id)
{
    if (m_currentAudioId == id) {
        return;
    }
    m_currentAudioId = id;
    Q_EMIT currentAudioIdChanged();
}

void PlayerViewModel::onSubtitleTrackChanged(int id)
{
    if (m_currentSubtitleId == id) {
        return;
    }
    m_currentSubtitleId = id;
    Q_EMIT currentSubtitleIdChanged();
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
