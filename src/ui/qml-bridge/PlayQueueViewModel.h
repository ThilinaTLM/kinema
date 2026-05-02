// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/QueueItem.h"

#include <QAbstractListModel>
#include <QHash>
#include <QString>

namespace kinema::controllers {
class PlayQueueController;
class PlaybackController;
}

namespace kinema::ui::qml {

/**
 * QML-facing adapter over `controllers::PlayQueueController`. Wraps
 * the controller's `QList<api::QueueItem>` snapshot in a
 * `QAbstractListModel` whose rows the `QueuePage` `ListView`
 * binds against, and re-exposes the controller's mutation slots
 * through `Q_INVOKABLE` so QML can drive them directly.
 *
 * The model is event-driven: controller about-to/finished signals
 * translate 1:1 to the matching `beginInsertRows` /
 * `endInsertRows` / etc. calls so QML's `ListView` animates row
 * mutations.
 */
class PlayQueueViewModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
    Q_PROPERTY(int activeIndex READ activeIndex NOTIFY activeIndexChanged)
    Q_PROPERTY(bool empty READ empty NOTIFY countChanged)
    Q_PROPERTY(bool hasActiveItem READ hasActiveItem NOTIFY queueStateChanged)
    Q_PROPERTY(bool canClearExceptActive READ canClearExceptActive NOTIFY queueStateChanged)
    Q_PROPERTY(int failedCount READ failedCount NOTIFY queueStateChanged)
    Q_PROPERTY(int playedCount READ playedCount NOTIFY queueStateChanged)
    Q_PROPERTY(int pendingCount READ pendingCount NOTIFY queueStateChanged)
    Q_PROPERTY(bool canClearPlayed READ canClearPlayed NOTIFY queueStateChanged)
    Q_PROPERTY(int leadIndex READ leadIndex NOTIFY queueStateChanged)
    Q_PROPERTY(int remainingCount READ remainingCount NOTIFY queueStateChanged)
    Q_PROPERTY(bool embeddedPlaybackActive READ embeddedPlaybackActive NOTIFY playbackStateChanged)
    Q_PROPERTY(bool playbackPaused READ playbackPaused NOTIFY playbackStateChanged)
    Q_PROPERTY(double playbackPositionSeconds READ playbackPositionSeconds NOTIFY playbackStateChanged)
    Q_PROPERTY(double playbackDurationSeconds READ playbackDurationSeconds NOTIFY playbackStateChanged)
public:
    enum Roles {
        TitleRole = Qt::UserRole + 1,
        SubtitleRole,       ///< "S\u00d7E - episode title" or empty
        PosterUrlRole,      ///< QString
        ReleaseNameRole,
        ResolutionRole,
        QualityLabelRole,
        ProviderRole,
        SizeBytesRole,      ///< qint64; -1 if unknown
        StatusRole,         ///< api::QueueItem::Status as int
        IsActiveRole,
    };
    Q_ENUM(Roles)

    explicit PlayQueueViewModel(controllers::PlayQueueController* ctrl,
        QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    int activeIndex() const noexcept;
    bool empty() const noexcept { return rowCount() == 0; }
    bool hasActiveItem() const noexcept;
    bool canClearExceptActive() const noexcept;
    int failedCount() const noexcept;
    int playedCount() const noexcept;
    int pendingCount() const noexcept;
    bool canClearPlayed() const noexcept { return playedCount() > 0; }
    /// Index of the row that should receive the lead-row treatment
    /// in the queue page: the active item when something is
    /// playing, otherwise the first non-played row, or `-1` when
    /// the queue is empty / fully played.
    int leadIndex() const noexcept;
    int remainingCount() const noexcept;
    bool embeddedPlaybackActive() const noexcept;
    bool playbackPaused() const noexcept;
    double playbackPositionSeconds() const noexcept;
    double playbackDurationSeconds() const noexcept;

    void setPlaybackController(controllers::PlaybackController* ctrl);

public Q_SLOTS:
    void playAt(int index);
    void removeAt(int index);
    void moveTo(int from, int to);
    /// Open a reorder transaction so a drag-to-reorder gesture
    /// does not flush the database on every pointer crossing.
    /// Pair with `endReorder()`.
    void beginReorder();
    void endReorder();
    void clearAll();
    void clearAllExceptActive();
    void clearPlayed();
    void retryFailed(int index);
    void togglePause();
    void stopPlayback();
    void playPreviousItem();
    void playNextItem();

Q_SIGNALS:
    void countChanged();
    void activeIndexChanged();
    void queueStateChanged();
    void playbackStateChanged();

private Q_SLOTS:
    void onItemsAboutToReset();
    void onItemsReset();
    void onItemAboutToBeInserted(int index);
    void onItemInserted(int index);
    void onItemAboutToBeRemoved(int index);
    void onItemRemoved(int index);
    void onItemAboutToBeMoved(int from, int to);
    void onItemMoved(int from, int to);
    void onItemChanged(int index);
    void onActiveIndexChanged(int index);
    void onPlaybackSessionChanged();
    void onPlaybackPositionChanged(double seconds);
    void onPlaybackDurationChanged(double seconds);
    void onPlaybackPausedChanged(bool paused);

private:
    void emitQueueStateChanged();
    void emitPlaybackStateChanged();

    controllers::PlayQueueController* m_ctrl;
    controllers::PlaybackController* m_playbackCtrl = nullptr;
    bool m_moveInProgress = false;
    double m_playbackPositionSeconds = 0.0;
    double m_playbackDurationSeconds = 0.0;
    bool m_playbackPaused = false;
    bool m_hasEmbeddedPlaybackSession = false;
};

} // namespace kinema::ui::qml
