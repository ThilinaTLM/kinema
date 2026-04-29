// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <memory>

#include <QObject>
#include <QStringList>
#include <QVariantMap>

namespace kinema::core {
class IdleInhibitor;
}

namespace kinema::controllers {

class PlayQueueController;
class PlaybackController;

class MprisController : public QObject
{
    Q_OBJECT
public:
    MprisController(PlaybackController& playback,
        PlayQueueController* queue,
        QObject* parent = nullptr);
    ~MprisController() override;

    QString identity() const;
    QString desktopEntry() const;
    QString playbackStatus() const;
    QVariantMap metadata() const;
    qlonglong positionUs() const;
    double volume() const;
    double rate() const;
    double minimumRate() const noexcept { return 0.25; }
    double maximumRate() const noexcept { return 4.0; }
    bool canGoNext() const;
    bool canGoPrevious() const;
    bool canPlay() const;
    bool canPause() const;
    bool canSeek() const;
    bool canControl() const;
    QStringList supportedUriSchemes() const;
    QStringList supportedMimeTypes() const;
    QString currentTrackObjectPath() const;

    void raise();
    void quit();
    void next();
    void previous();
    void pause();
    void playPause();
    void stop();
    void play();
    void seek(qlonglong offsetUs);
    void setPosition(const QString& trackPath, qlonglong positionUs);
    void setVolume(double value);
    void setRate(double value);

Q_SIGNALS:
    void raiseRequested();
    void quitRequested();

private Q_SLOTS:
    void refresh();
    void onSeeked(double seconds);

private:
    void ensureRegistration();
    void setRegistered(bool on);
    void emitPlayerPropertiesChanged();
    void emitRootPropertiesChanged();

    PlaybackController& m_playback;
    PlayQueueController* m_queue = nullptr;
    std::unique_ptr<core::IdleInhibitor> m_inhibitor;
    bool m_objectRegistered = false;
    bool m_serviceRegistered = false;
};

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV
