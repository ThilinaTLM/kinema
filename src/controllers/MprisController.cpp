// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "controllers/MprisController.h"

#include "controllers/PlaybackController.h"
#include "controllers/SeriesPlaybackSessionController.h"
#include "core/IdleInhibitor.h"
#include "core/MprisMetadata.h"
#include "kinema_log_controller.h"

#include <QDBusAbstractAdaptor>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusObjectPath>

#include <algorithm>

namespace kinema::controllers {

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
    explicit MediaPlayer2Adaptor(MprisController* controller)
        : QDBusAbstractAdaptor(controller)
        , m_controller(controller)
    {
    }

    bool canQuit() const { return true; }
    bool canRaise() const { return true; }
    bool hasTrackList() const { return false; }
    QString identity() const { return m_controller->identity(); }
    QString desktopEntry() const { return m_controller->desktopEntry(); }
    QStringList supportedUriSchemes() const
    {
        return m_controller->supportedUriSchemes();
    }
    QStringList supportedMimeTypes() const
    {
        return m_controller->supportedMimeTypes();
    }

public Q_SLOTS:
    void Raise() { m_controller->raise(); }
    void Quit() { m_controller->quit(); }

private:
    MprisController* m_controller;
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
    explicit MediaPlayer2PlayerAdaptor(MprisController* controller)
        : QDBusAbstractAdaptor(controller)
        , m_controller(controller)
    {
    }

    QString playbackStatus() const { return m_controller->playbackStatus(); }
    QVariantMap metadata() const { return m_controller->metadata(); }
    double volume() const { return m_controller->volume(); }
    qlonglong position() const { return m_controller->positionUs(); }
    double rate() const { return m_controller->rate(); }
    double minimumRate() const { return m_controller->minimumRate(); }
    double maximumRate() const { return m_controller->maximumRate(); }
    bool canGoNext() const { return m_controller->canGoNext(); }
    bool canGoPrevious() const { return m_controller->canGoPrevious(); }
    bool canPlay() const { return m_controller->canPlay(); }
    bool canPause() const { return m_controller->canPause(); }
    bool canSeek() const { return m_controller->canSeek(); }
    bool canControl() const { return m_controller->canControl(); }

public Q_SLOTS:
    void Next() { m_controller->next(); }
    void Previous() { m_controller->previous(); }
    void Pause() { m_controller->pause(); }
    void PlayPause() { m_controller->playPause(); }
    void Stop() { m_controller->stop(); }
    void Play() { m_controller->play(); }
    void Seek(qlonglong offsetUs) { m_controller->seek(offsetUs); }
    void SetPosition(const QDBusObjectPath& trackId, qlonglong positionUs)
    {
        m_controller->setPosition(trackId.path(), positionUs);
    }
    void SetVolume(double value) { m_controller->setVolume(value); }
    void SetRate(double value) { m_controller->setRate(value); }

Q_SIGNALS:
    void Seeked(qlonglong position);

private:
    MprisController* m_controller;
};

} // namespace

MprisController::MprisController(PlaybackController& playback,
    SeriesPlaybackSessionController* seriesSession,
    QObject* parent)
    : QObject(parent)
    , m_playback(playback)
    , m_seriesSession(seriesSession)
    , m_inhibitor(std::make_unique<core::IdleInhibitor>())
{
    new MediaPlayer2Adaptor(this);
    new MediaPlayer2PlayerAdaptor(this);

    ensureRegistration();

    connect(&m_playback, &PlaybackController::sessionStateChanged,
        this, &MprisController::refresh);
    connect(&m_playback, &PlaybackController::seeked,
        this, &MprisController::onSeeked);
    if (m_seriesSession) {
        connect(m_seriesSession,
            &SeriesPlaybackSessionController::navigationChanged,
            this, &MprisController::refresh);
    }

    refresh();
}

MprisController::~MprisController()
{
    setRegistered(false);
    if (m_objectRegistered) {
        QDBusConnection::sessionBus().unregisterObject(QString::fromLatin1(kPath));
    }
}

QString MprisController::identity() const
{
    return QStringLiteral("Kinema");
}

QString MprisController::desktopEntry() const
{
    return QStringLiteral("dev.tlmtech.kinema");
}

QString MprisController::playbackStatus() const
{
    if (!m_playback.hasActiveSession()) {
        return QStringLiteral("Stopped");
    }
    return m_playback.isActivelyPlaying()
        ? QStringLiteral("Playing")
        : QStringLiteral("Paused");
}

QVariantMap MprisController::metadata() const
{
    if (!m_playback.hasActiveSession()) {
        return {};
    }
    return core::mpris::metadata(m_playback.currentContext(),
        m_playback.durationSeconds());
}

qlonglong MprisController::positionUs() const
{
    return static_cast<qlonglong>(
        m_playback.currentPositionSeconds() * 1000000.0);
}

double MprisController::volume() const
{
    const double percent = m_playback.volumePercent();
    if (percent < 0.0) {
        return 1.0;
    }
    return percent / 100.0;
}

double MprisController::rate() const
{
    return m_playback.playbackRate();
}

bool MprisController::canGoNext() const
{
    return m_seriesSession && m_seriesSession->canGoNext();
}

bool MprisController::canGoPrevious() const
{
    return m_seriesSession && m_seriesSession->canGoPrevious();
}

bool MprisController::canPlay() const
{
    return m_playback.hasActiveSession();
}

bool MprisController::canPause() const
{
    return m_playback.hasActiveSession();
}

bool MprisController::canSeek() const
{
    return m_playback.hasActiveSession();
}

bool MprisController::canControl() const
{
    return m_playback.hasActiveSession();
}

QStringList MprisController::supportedUriSchemes() const
{
    return { QStringLiteral("file"), QStringLiteral("http"),
        QStringLiteral("https") };
}

QStringList MprisController::supportedMimeTypes() const
{
    return {};
}

QString MprisController::currentTrackObjectPath() const
{
    if (!m_playback.hasActiveSession()) {
        return {};
    }
    return core::mpris::trackObjectPath(m_playback.currentKey());
}

void MprisController::raise()
{
    Q_EMIT raiseRequested();
}

void MprisController::quit()
{
    Q_EMIT quitRequested();
}

void MprisController::next()
{
    if (!m_seriesSession || !canGoNext()) {
        return;
    }
    m_seriesSession->playNextEpisode();
}

void MprisController::previous()
{
    if (!m_seriesSession || !canGoPrevious()) {
        return;
    }
    m_seriesSession->playPreviousEpisode();
}

void MprisController::pause()
{
    m_playback.pause();
}

void MprisController::playPause()
{
    m_playback.playPause();
}

void MprisController::stop()
{
    m_playback.stop();
}

void MprisController::play()
{
    m_playback.resume();
}

void MprisController::seek(qlonglong offsetUs)
{
    m_playback.seekRelativeSeconds(static_cast<double>(offsetUs) / 1000000.0);
}

void MprisController::setPosition(const QString& trackPath,
    qlonglong positionUs)
{
    if (trackPath != currentTrackObjectPath()) {
        return;
    }
    m_playback.seekAbsoluteSeconds(
        static_cast<double>(positionUs) / 1000000.0);
}

void MprisController::setVolume(double value)
{
    m_playback.setVolumePercent(std::max(0.0, value) * 100.0);
}

void MprisController::setRate(double value)
{
    if (value <= 0.0) {
        return;
    }
    m_playback.setPlaybackRate(value);
}

void MprisController::refresh()
{
    setRegistered(m_playback.hasActiveSession());
    m_inhibitor->setActive(m_playback.isActivelyPlaying(),
        QStringLiteral("Playing media in Kinema"));
    if (m_serviceRegistered) {
        emitPlayerPropertiesChanged();
    }
}

void MprisController::onSeeked(double seconds)
{
    if (!m_serviceRegistered) {
        return;
    }
    auto msg = QDBusMessage::createSignal(QString::fromLatin1(kPath),
        QString::fromLatin1(kPlayerInterface), QStringLiteral("Seeked"));
    msg << static_cast<qlonglong>(seconds * 1000000.0);
    QDBusConnection::sessionBus().send(msg);
}

void MprisController::ensureRegistration()
{
    if (m_objectRegistered) {
        return;
    }
    m_objectRegistered = QDBusConnection::sessionBus().registerObject(
        QString::fromLatin1(kPath), this, QDBusConnection::ExportAdaptors);
    if (!m_objectRegistered) {
        qCWarning(KINEMA_CONTROLLER) << "failed to register MPRIS object";
    }
}

void MprisController::setRegistered(bool on)
{
    if (m_serviceRegistered == on) {
        return;
    }
    if (on) {
        m_serviceRegistered = QDBusConnection::sessionBus().registerService(
            QString::fromLatin1(kService));
        if (!m_serviceRegistered) {
            qCWarning(KINEMA_CONTROLLER) << "failed to acquire MPRIS service name"
                              << kService;
        }
        return;
    }
    QDBusConnection::sessionBus().unregisterService(QString::fromLatin1(kService));
    m_serviceRegistered = false;
}

void MprisController::emitPlayerPropertiesChanged()
{
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
    msg << QString::fromLatin1(kPlayerInterface) << changed << QStringList();
    QDBusConnection::sessionBus().send(msg);
}

void MprisController::emitRootPropertiesChanged()
{
}

} // namespace kinema::controllers

#include "MprisController.moc"

#endif // KINEMA_HAVE_LIBMPV
