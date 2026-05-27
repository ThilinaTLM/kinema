// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QObject>
#include <QString>

namespace kinema::controllers {
class SeriesPlaybackSessionController;
}

namespace kinema::playback::series {

/**
 * Facade over `controllers::SeriesPlaybackSessionController`.
 *
 * Provides the long-term API surface for season-pack adjacency,
 * auto-next, and current-stream size hydration. The wrapped
 * controller will be deleted in a later phase once
 * `SessionFileCatalog` + `MediaFileSelectionPolicy` drive the
 * navigation directly.
 */
class SeriesSessionService : public QObject
{
    Q_OBJECT
public:
    explicit SeriesSessionService(
        controllers::SeriesPlaybackSessionController& inner,
        QObject* parent = nullptr);
    ~SeriesSessionService() override;

    bool navigationVisible() const noexcept;
    bool canGoPrevious() const noexcept;
    bool canGoNext() const noexcept;

public Q_SLOTS:
    void playPreviousEpisode();
    void playNextEpisode();

Q_SIGNALS:
    void navigationChanged();
    void packAdjacencyResolved(bool nextAvailable,
        int nextSeason, int nextEpisode);
    void currentStreamSizeResolved(const QString& infoHash,
        int fileIndex, qint64 sizeBytes);

    /// Forwarded so the shell can close the player window when an
    /// auto-next picks up.
    void windowCloseRequested();

public:
    /// Public access to the wrapped controller for ServiceContainer
    /// wiring of the rest of the app. Will be removed once the
    /// inner controller goes away.
    controllers::SeriesPlaybackSessionController* inner() const noexcept
    { return &m_inner; }

private:
    controllers::SeriesPlaybackSessionController& m_inner;
};

} // namespace kinema::playback::series

#endif // KINEMA_HAVE_LIBMPV
