// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "playback/desktop/MprisPlaybackProjection.h"

#include "core/io/IdleInhibitor.h"
#include "core/mpv/MprisMetadata.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/PlayerPort.h"
#include "playback/series/SeriesSessionService.h"
#include "playback/session/PlaybackSessionManager.h"
#include "kinema_log_controller.h"

#include <QDBusAbstractAdaptor>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusObjectPath>

#include <algorithm>
#include <variant>

namespace kinema::playback::desktop {

namespace {

constexpr auto kService = "org.mpris.MediaPlayer2.kinema";
constexpr auto kPath = "/org/mpris/MediaPlayer2";
constexpr auto kRootInterface = "org.mpris.MediaPlayer2";
constexpr auto kPlayerInterface = "org.mpris.MediaPlayer2.Player";

class MediaPlayer2Adaptor final : public QDBusAbstractAdaptor
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.mpris.MediaPlayer2")
    Q_PROPERTY(bool CanQuit READ canQuit)
    Q_PROPERTY(bool CanRaise READ canRaise)
    Q_PROPERTY(bool HasTrackList READ hasTrackList)
    Q_PROPERTY(QString Identity READ identity)
    Q_PROPERTY(QString DesktopEntry READ desktopEntry)
    Q_PROPERTY(QStringList SupportedUriSchemes READ supportedUriSchemes)
    Q_PROPERTY(QStringList SupportedMimeTypes READ supportedMimeTypes)

public:
    explicit MediaPlayer2Adaptor(MprisPlaybackProjection* projection)
        : QDBusAbstractAdaptor(projection)
        , m_projection(projection)
    {
    }

    bool canQuit() const { return true; }
    bool canRaise() const { return true; }
    bool hasTrackList() const { return false; }
    QString identity() const { return m_projection->identity(); }
    QString desktopEntry() const { return m_projection->desktopEntry(); }
    QStringList supportedUriSchemes() const
    {
        return m_projection->supportedUriSchemes();
    }
    QStringList supportedMimeTypes() const
    {
        return m_projection->supportedMimeTypes();
    }

public Q_SLOTS:
    void Raise() { m_projection->raise(); }
    void Quit() { m_projection->quit(); }

private:
    MprisPlaybackProjection* m_projection;
};

class MediaPlayer2PlayerAdaptor final : public QDBusAbstractAdaptor
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.mpris.MediaPlayer2.Player")
    Q_PROPERTY(QString PlaybackStatus READ playbackStatus)
    Q_PROPERTY(QVariantMap Metadata READ metadata)
    Q_PROPERTY(double Volume READ volume WRITE SetVolume)
    Q_PROPERTY(qlonglong Position READ position)
    Q_PROPERTY(double Rate READ rate WRITE SetRate)
    Q_PROPERTY(double MinimumRate READ minimumRate)
    Q_PROPERTY(double MaximumRate READ maximumRate)
    Q_PROPERTY(bool CanGoNext READ canGoNext)
    Q_PROPERTY(bool CanGoPrevious READ canGoPrevious)
    Q_PROPERTY(bool CanPlay READ canPlay)
    Q_PROPERTY(bool CanPause READ canPause)
    Q_PROPERTY(bool CanSeek READ canSeek)
    Q_PROPERTY(bool CanControl READ canControl)

public:
    explicit MediaPlayer2PlayerAdaptor(MprisPlaybackProjection* projection)
        : QDBusAbstractAdaptor(projection)
        , m_projection(projection)
    {
    }

    QString playbackStatus() const { return m_projection->playbackStatus(); }
    QVariantMap metadata() const { return m_projection->metadata(); }
    double volume() const { return m_projection->volume(); }
    qlonglong position() const { return m_projection->positionUs(); }
    double rate() const { return m_projection->rate(); }
    double minimumRate() const { return m_projection->minimumRate(); }
    double maximumRate() const { return m_projection->maximumRate(); }
    bool canGoNext() const { return m_projection->canGoNext(); }
    bool canGoPrevious() const { return m_projection->canGoPrevious(); }
    bool canPlay() const { return m_projection->canPlay(); }
    bool canPause() const { return m_projection->canPause(); }
    bool canSeek() const { return m_projection->canSeek(); }
    bool canControl() const { return m_projection->canControl(); }

public Q_SLOTS:
    void Next() { m_projection->next(); }
    void Previous() { m_projection->previous(); }
    void Pause() { m_projection->pause(); }
    void PlayPause() { m_projection->playPause(); }
    void Stop() { m_projection->stop(); }
    void Play() { m_projection->play(); }
    void Seek(qlonglong offsetUs) { m_projection->seek(offsetUs); }
    void SetPosition(const QDBusObjectPath& trackId, qlonglong positionUs)
    {
        m_projection->setPosition(trackId.path(), positionUs);
    }
    void SetVolume(double value) { m_projection->setVolume(value); }
    void SetRate(double value) { m_projection->setRate(value); }

Q_SIGNALS:
    void Seeked(qlonglong position);

private:
    MprisPlaybackProjection* m_projection;
};

} // namespace

MprisPlaybackProjection::MprisPlaybackProjection(
    events::PlaybackEventStream& events,
    session::PlaybackSessionManager& sessions,
    ports::PlayerPort* player,
    series::SeriesSessionService* series,
    QObject* parent)
    : QObject(parent)
    , m_events(events)
    , m_sessions(sessions)
    , m_player(player)
    , m_series(series)
    , m_inhibitor(std::make_unique<core::IdleInhibitor>())
{
    new MediaPlayer2Adaptor(this);
    new MediaPlayer2PlayerAdaptor(this);

    ensureObjectRegistered();

    connect(&m_events, &events::PlaybackEventStream::eventPublished,
        this, &MprisPlaybackProjection::onEvent);
    if (m_series) {
        connect(m_series,
            &series::SeriesSessionService::navigationChanged,
            this, &MprisPlaybackProjection::onSeriesNavigationChanged);
    }
}

MprisPlaybackProjection::~MprisPlaybackProjection()
{
    setServiceRegistered(false);
    if (m_objectRegistered) {
        QDBusConnection::sessionBus().unregisterObject(
            QString::fromLatin1(kPath));
    }
}

// ---------------------- Root properties ----------------------

QString MprisPlaybackProjection::identity() const
{
    return QStringLiteral("Kinema");
}

QString MprisPlaybackProjection::desktopEntry() const
{
    return QStringLiteral("dev.tlmtech.kinema");
}

QStringList MprisPlaybackProjection::supportedUriSchemes() const
{
    return { QStringLiteral("file"), QStringLiteral("http"),
        QStringLiteral("https") };
}

QStringList MprisPlaybackProjection::supportedMimeTypes() const
{
    return {};
}

// ---------------------- Player properties ----------------------

QString MprisPlaybackProjection::playbackStatus() const
{
    if (!m_sessionActive) {
        return QStringLiteral("Stopped");
    }
    return isActivelyPlaying()
        ? QStringLiteral("Playing")
        : QStringLiteral("Paused");
}

QVariantMap MprisPlaybackProjection::metadata() const
{
    if (!m_sessionActive) {
        return {};
    }
    return core::mpris::metadata(m_ctx, m_duration);
}

qlonglong MprisPlaybackProjection::positionUs() const
{
    if (m_player) {
        const auto snap = m_player->snapshot();
        if (snap.active) {
            return static_cast<qlonglong>(snap.positionSec * 1000000.0);
        }
    }
    return static_cast<qlonglong>(m_position * 1000000.0);
}

double MprisPlaybackProjection::volume() const
{
    double percent = 100.0;
    if (m_player) {
        percent = m_player->snapshot().volumePercent;
    }
    if (percent < 0.0) {
        return 1.0;
    }
    return percent / 100.0;
}

double MprisPlaybackProjection::rate() const
{
    if (m_player) {
        return m_player->snapshot().playbackRate;
    }
    return 1.0;
}

bool MprisPlaybackProjection::canGoNext() const
{
    return m_series && m_series->canGoNext();
}

bool MprisPlaybackProjection::canGoPrevious() const
{
    return m_series && m_series->canGoPrevious();
}

bool MprisPlaybackProjection::canPlay() const
{
    return m_sessionActive;
}

bool MprisPlaybackProjection::canPause() const
{
    return m_sessionActive;
}

bool MprisPlaybackProjection::canSeek() const
{
    return m_sessionActive;
}

bool MprisPlaybackProjection::canControl() const
{
    return m_sessionActive;
}

QString MprisPlaybackProjection::currentTrackObjectPath() const
{
    if (!m_sessionActive) {
        return {};
    }
    return core::mpris::trackObjectPath(m_ctx.key);
}

bool MprisPlaybackProjection::isActivelyPlaying() const noexcept
{
    return m_sessionActive && m_playing && !m_paused;
}

// ---------------------- Root commands ----------------------

void MprisPlaybackProjection::raise()
{
    Q_EMIT raiseRequested();
}

void MprisPlaybackProjection::quit()
{
    Q_EMIT quitRequested();
}

// ---------------------- Player commands ----------------------

void MprisPlaybackProjection::next()
{
    if (!m_series || !m_series->canGoNext()) {
        return;
    }
    m_series->playNextEpisode();
}

void MprisPlaybackProjection::previous()
{
    if (!m_series || !m_series->canGoPrevious()) {
        return;
    }
    m_series->playPreviousEpisode();
}

void MprisPlaybackProjection::pause()
{
    m_sessions.pause();
}

void MprisPlaybackProjection::playPause()
{
    m_sessions.playPause();
}

void MprisPlaybackProjection::stop()
{
    m_sessions.stop();
}

void MprisPlaybackProjection::play()
{
    // MPRIS `Play` resumes the current track. Without an active
    // session there is nothing meaningful to start; clients that
    // want to start a fresh stream go through the UI.
    m_sessions.resume();
}

void MprisPlaybackProjection::seek(qlonglong offsetUs)
{
    m_sessions.seekRelativeSeconds(
        static_cast<double>(offsetUs) / 1000000.0);
}

void MprisPlaybackProjection::setPosition(const QString& trackPath,
    qlonglong positionUs)
{
    if (trackPath != currentTrackObjectPath()) {
        return;
    }
    if (positionUs < 0) {
        return;
    }
    m_sessions.seekAbsoluteSeconds(
        static_cast<double>(positionUs) / 1000000.0);
}

void MprisPlaybackProjection::setVolume(double value)
{
    m_sessions.setVolumePercent(std::max(0.0, value) * 100.0);
}

void MprisPlaybackProjection::setRate(double value)
{
    if (value <= 0.0) {
        return;
    }
    m_sessions.setPlaybackRate(value);
}

// ---------------------- Event handling ----------------------

void MprisPlaybackProjection::onEvent(const events::PlaybackEvent& e)
{
    std::visit([this](const auto& payload) {
        using T = std::decay_t<decltype(payload)>;
        if constexpr (std::is_same_v<T, events::PlaybackRequested>) {
            onPlaybackRequested(payload);
        } else if constexpr (std::is_same_v<T, events::PlayerLoaded>) {
            onPlayerLoaded(payload);
        } else if constexpr (std::is_same_v<T, events::PlaybackStateChanged>) {
            onPlaybackStateChanged(payload);
        } else if constexpr (std::is_same_v<T, events::PositionTicked>) {
            onPositionTicked(payload);
        } else if constexpr (std::is_same_v<T, events::DurationChanged>) {
            onDurationChanged(payload);
        } else if constexpr (std::is_same_v<T, events::PlaybackEnded>) {
            onPlaybackEnded(payload);
        } else if constexpr (std::is_same_v<T, events::PlaybackFailed>) {
            onPlaybackFailed(payload);
        }
    }, e);
}

void MprisPlaybackProjection::onPlaybackRequested(
    const events::PlaybackRequested& e)
{
    m_sessionId = e.sessionId;
    m_ctx = e.ctx;
    m_sessionActive = true;
    m_playing = false;
    m_paused = false;
    m_position = 0.0;
    m_duration = 0.0;
    setServiceRegistered(true);
    refreshIdleInhibitor();
    emitPropertiesChanged();
}

void MprisPlaybackProjection::onPlayerLoaded(const events::PlayerLoaded& e)
{
    if (e.sessionId != m_sessionId) {
        return;
    }
    // PlayerLoaded does not by itself imply `Playing` — the
    // `PlaybackStateChanged` that follows when mpv unpauses tells
    // us the real state. Still, refresh metadata-derived
    // properties so clients see the track id / title / duration.
    emitPropertiesChanged();
}

void MprisPlaybackProjection::onPlaybackStateChanged(
    const events::PlaybackStateChanged& e)
{
    if (e.sessionId != m_sessionId) {
        return;
    }
    m_playing = e.playing;
    m_paused = e.paused;
    refreshIdleInhibitor();
    emitPropertiesChanged();
}

void MprisPlaybackProjection::onPositionTicked(
    const events::PositionTicked& e)
{
    if (e.sessionId != m_sessionId) {
        return;
    }
    m_position = e.seconds;
    // Per the MPRIS spec, position changes are polled via the
    // `Position` property rather than broadcast — we deliberately
    // do not emit PropertiesChanged on every tick.
}

void MprisPlaybackProjection::onDurationChanged(
    const events::DurationChanged& e)
{
    if (e.sessionId != m_sessionId) {
        return;
    }
    if (qFuzzyCompare(m_duration + 1.0, e.seconds + 1.0)) {
        return;
    }
    m_duration = e.seconds;
    emitPropertiesChanged();
}

void MprisPlaybackProjection::onPlaybackEnded(
    const events::PlaybackEnded& e)
{
    if (e.sessionId != m_sessionId) {
        return;
    }
    m_sessionActive = false;
    m_playing = false;
    m_paused = false;
    m_position = 0.0;
    m_duration = 0.0;
    refreshIdleInhibitor();
    setServiceRegistered(false);
    emitPropertiesChanged();
}

void MprisPlaybackProjection::onPlaybackFailed(
    const events::PlaybackFailed& e)
{
    if (e.sessionId != m_sessionId) {
        return;
    }
    m_sessionActive = false;
    m_playing = false;
    m_paused = false;
    refreshIdleInhibitor();
    setServiceRegistered(false);
    emitPropertiesChanged();
}

void MprisPlaybackProjection::onSeriesNavigationChanged()
{
    emitPropertiesChanged();
}

// ---------------------- D-Bus plumbing ----------------------

void MprisPlaybackProjection::ensureObjectRegistered()
{
    if (m_objectRegistered) {
        return;
    }
    m_objectRegistered = QDBusConnection::sessionBus().registerObject(
        QString::fromLatin1(kPath), this,
        QDBusConnection::ExportAdaptors);
    if (!m_objectRegistered) {
        qCWarning(KINEMA_CONTROLLER)
            << "MprisPlaybackProjection: failed to register object at"
            << kPath;
    }
}

void MprisPlaybackProjection::setServiceRegistered(bool on)
{
    if (m_serviceRegistered == on) {
        return;
    }
    if (on) {
        m_serviceRegistered = QDBusConnection::sessionBus()
            .registerService(QString::fromLatin1(kService));
        if (!m_serviceRegistered) {
            qCWarning(KINEMA_CONTROLLER)
                << "MprisPlaybackProjection: failed to acquire service name"
                << kService;
        }
        return;
    }
    QDBusConnection::sessionBus().unregisterService(
        QString::fromLatin1(kService));
    m_serviceRegistered = false;
}

void MprisPlaybackProjection::emitPropertiesChanged()
{
    if (!m_serviceRegistered) {
        return;
    }
    QVariantMap changed;
    changed.insert(QStringLiteral("PlaybackStatus"), playbackStatus());
    changed.insert(QStringLiteral("Metadata"), metadata());
    changed.insert(QStringLiteral("Volume"), volume());
    changed.insert(QStringLiteral("Rate"), rate());
    changed.insert(QStringLiteral("MinimumRate"), minimumRate());
    changed.insert(QStringLiteral("MaximumRate"), maximumRate());
    changed.insert(QStringLiteral("CanGoNext"), canGoNext());
    changed.insert(QStringLiteral("CanGoPrevious"), canGoPrevious());
    changed.insert(QStringLiteral("CanPlay"), canPlay());
    changed.insert(QStringLiteral("CanPause"), canPause());
    changed.insert(QStringLiteral("CanSeek"), canSeek());
    changed.insert(QStringLiteral("CanControl"), canControl());

    auto msg = QDBusMessage::createSignal(QString::fromLatin1(kPath),
        QStringLiteral("org.freedesktop.DBus.Properties"),
        QStringLiteral("PropertiesChanged"));
    msg << QString::fromLatin1(kPlayerInterface) << changed
        << QStringList();
    QDBusConnection::sessionBus().send(msg);
}

void MprisPlaybackProjection::emitSeeked(double seconds)
{
    if (!m_serviceRegistered) {
        return;
    }
    auto msg = QDBusMessage::createSignal(QString::fromLatin1(kPath),
        QString::fromLatin1(kPlayerInterface),
        QStringLiteral("Seeked"));
    msg << static_cast<qlonglong>(seconds * 1000000.0);
    QDBusConnection::sessionBus().send(msg);
}

void MprisPlaybackProjection::refreshIdleInhibitor()
{
    m_inhibitor->setActive(isActivelyPlaying(),
        QStringLiteral("Playing media in Kinema"));
}

} // namespace kinema::playback::desktop

#include "MprisPlaybackProjection.moc"

#endif // KINEMA_HAVE_LIBMPV
