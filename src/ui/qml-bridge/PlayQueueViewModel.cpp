// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/PlayQueueViewModel.h"

#ifdef KINEMA_HAVE_LIBMPV
#include "controllers/PlaybackController.h"
#endif
#include "controllers/PlayQueueController.h"
#include "ui/qml-bridge/QueueDisplay.h"

namespace kinema::ui::qml {

PlayQueueViewModel::PlayQueueViewModel(
    controllers::PlayQueueController* ctrl, QObject* parent)
    : QAbstractListModel(parent)
    , m_ctrl(ctrl)
{
    if (!m_ctrl) {
        return;
    }
    connect(m_ctrl, &controllers::PlayQueueController::itemsAboutToReset,
        this, &PlayQueueViewModel::onItemsAboutToReset);
    connect(m_ctrl, &controllers::PlayQueueController::itemsReset,
        this, &PlayQueueViewModel::onItemsReset);
    connect(m_ctrl, &controllers::PlayQueueController::itemAboutToBeInserted,
        this, &PlayQueueViewModel::onItemAboutToBeInserted);
    connect(m_ctrl, &controllers::PlayQueueController::itemInserted,
        this, &PlayQueueViewModel::onItemInserted);
    connect(m_ctrl, &controllers::PlayQueueController::itemAboutToBeRemoved,
        this, &PlayQueueViewModel::onItemAboutToBeRemoved);
    connect(m_ctrl, &controllers::PlayQueueController::itemRemoved,
        this, &PlayQueueViewModel::onItemRemoved);
    connect(m_ctrl, &controllers::PlayQueueController::itemAboutToBeMoved,
        this, &PlayQueueViewModel::onItemAboutToBeMoved);
    connect(m_ctrl, &controllers::PlayQueueController::itemMoved,
        this, &PlayQueueViewModel::onItemMoved);
    connect(m_ctrl, &controllers::PlayQueueController::itemChanged,
        this, &PlayQueueViewModel::onItemChanged);
    connect(m_ctrl, &controllers::PlayQueueController::activeIndexChanged,
        this, &PlayQueueViewModel::onActiveIndexChanged);
}

int PlayQueueViewModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid() || !m_ctrl) {
        return 0;
    }
    return m_ctrl->items().size();
}

QVariant PlayQueueViewModel::data(const QModelIndex& index, int role) const
{
    if (!m_ctrl || !index.isValid()) {
        return {};
    }
    const auto& items = m_ctrl->items();
    if (index.row() < 0 || index.row() >= items.size()) {
        return {};
    }
    const auto& it = items[index.row()];
    switch (role) {
    case TitleRole:
        return it.title;
    case SubtitleRole:
        return queue_display::subtitle(it);
    case PosterUrlRole:
        return it.poster.isValid() ? it.poster.toString() : QString {};
    case ReleaseNameRole:
        return it.streamRef.releaseName;
    case ResolutionRole:
        return it.streamRef.resolution;
    case QualityLabelRole:
        return it.streamRef.qualityLabel;
    case ProviderRole:
        return it.streamRef.provider;
    case SizeBytesRole:
        return it.streamRef.sizeBytes
            ? QVariant(static_cast<qlonglong>(*it.streamRef.sizeBytes))
            : QVariant(static_cast<qlonglong>(-1));
    case StatusRole:
        return static_cast<int>(it.status);
    case IsActiveRole:
        return index.row() == m_ctrl->activeIndex();
    default:
        return {};
    }
}

QHash<int, QByteArray> PlayQueueViewModel::roleNames() const
{
    return {
        { TitleRole, "title" },
        { SubtitleRole, "subtitle" },
        { PosterUrlRole, "posterUrl" },
        { ReleaseNameRole, "releaseName" },
        { ResolutionRole, "resolution" },
        { QualityLabelRole, "qualityLabel" },
        { ProviderRole, "provider" },
        { SizeBytesRole, "sizeBytes" },
        { StatusRole, "status" },
        { IsActiveRole, "isActive" },
    };
}

int PlayQueueViewModel::activeIndex() const noexcept
{
    return m_ctrl ? m_ctrl->activeIndex() : -1;
}

bool PlayQueueViewModel::hasActiveItem() const noexcept
{
    return activeIndex() >= 0 && activeIndex() < rowCount();
}

bool PlayQueueViewModel::canClearExceptActive() const noexcept
{
    return hasActiveItem() && rowCount() > 1;
}

int PlayQueueViewModel::failedCount() const noexcept
{
    if (!m_ctrl) {
        return 0;
    }

    int count = 0;
    for (const auto& item : m_ctrl->items()) {
        if (item.status == api::QueueItem::Status::Failed) {
            ++count;
        }
    }
    return count;
}

int PlayQueueViewModel::remainingCount() const noexcept
{
    if (!hasActiveItem()) {
        return rowCount();
    }
    const int remaining = rowCount() - activeIndex() - 1;
    return remaining > 0 ? remaining : 0;
}

bool PlayQueueViewModel::embeddedPlaybackActive() const noexcept
{
#ifdef KINEMA_HAVE_LIBMPV
    if (!m_playbackCtrl || !m_hasEmbeddedPlaybackSession || !hasActiveItem()) {
        return false;
    }

    const int idx = activeIndex();
    const auto& items = m_ctrl->items();
    return idx >= 0 && idx < items.size()
        && m_playbackCtrl->currentKey() == items.at(idx).key;
#else
    return false;
#endif
}

bool PlayQueueViewModel::playbackPaused() const noexcept
{
    return m_playbackPaused;
}

double PlayQueueViewModel::playbackPositionSeconds() const noexcept
{
    return m_playbackPositionSeconds;
}

double PlayQueueViewModel::playbackDurationSeconds() const noexcept
{
    return m_playbackDurationSeconds;
}

void PlayQueueViewModel::setPlaybackController(
    controllers::PlaybackController* ctrl)
{
    if (m_playbackCtrl == ctrl) {
        return;
    }

    m_playbackCtrl = ctrl;
#ifdef KINEMA_HAVE_LIBMPV
    if (m_playbackCtrl) {
        m_hasEmbeddedPlaybackSession = m_playbackCtrl->hasActiveSession();
        m_playbackPaused = m_playbackCtrl->isPaused();
        m_playbackPositionSeconds = m_playbackCtrl->currentPositionSeconds();
        m_playbackDurationSeconds = m_playbackCtrl->durationSeconds();

        connect(m_playbackCtrl,
            &controllers::PlaybackController::activeSessionChanged,
            this, [this](bool) { onPlaybackSessionChanged(); });
        connect(m_playbackCtrl,
            &controllers::PlaybackController::positionChanged,
            this, &PlayQueueViewModel::onPlaybackPositionChanged);
        connect(m_playbackCtrl,
            &controllers::PlaybackController::durationChanged,
            this, &PlayQueueViewModel::onPlaybackDurationChanged);
        connect(m_playbackCtrl,
            &controllers::PlaybackController::pausedChanged,
            this, &PlayQueueViewModel::onPlaybackPausedChanged);
    } else {
        m_hasEmbeddedPlaybackSession = false;
        m_playbackPaused = false;
        m_playbackPositionSeconds = 0.0;
        m_playbackDurationSeconds = 0.0;
    }
#else
    Q_UNUSED(ctrl);
    m_hasEmbeddedPlaybackSession = false;
    m_playbackPaused = false;
    m_playbackPositionSeconds = 0.0;
    m_playbackDurationSeconds = 0.0;
#endif

    emitPlaybackStateChanged();
}

void PlayQueueViewModel::playAt(int index)
{
    if (m_ctrl) {
        m_ctrl->playAt(index);
    }
}

void PlayQueueViewModel::removeAt(int index)
{
    if (m_ctrl) {
        m_ctrl->removeAt(index);
    }
}

void PlayQueueViewModel::moveTo(int from, int to)
{
    if (m_ctrl) {
        m_ctrl->moveTo(from, to);
    }
}

void PlayQueueViewModel::clearAll()
{
    if (m_ctrl) {
        m_ctrl->clearAll();
    }
}

void PlayQueueViewModel::clearAllExceptActive()
{
    if (m_ctrl) {
        m_ctrl->clearAllExceptActive();
    }
}

void PlayQueueViewModel::retryFailed(int index)
{
    if (m_ctrl) {
        m_ctrl->retryFailed(index);
    }
}

void PlayQueueViewModel::togglePause()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_playbackCtrl && embeddedPlaybackActive()) {
        m_playbackCtrl->playPause();
    }
#endif
}

void PlayQueueViewModel::stopPlayback()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_playbackCtrl && embeddedPlaybackActive()) {
        m_playbackCtrl->stop();
    }
#endif
}

void PlayQueueViewModel::playPreviousItem()
{
    if (m_ctrl) {
        m_ctrl->playPreviousItem();
    }
}

void PlayQueueViewModel::playNextItem()
{
    if (m_ctrl) {
        m_ctrl->playNextItem();
    }
}

void PlayQueueViewModel::onItemsAboutToReset()
{
    beginResetModel();
}

void PlayQueueViewModel::onItemsReset()
{
    endResetModel();
    Q_EMIT countChanged();
    Q_EMIT activeIndexChanged();
    emitQueueStateChanged();
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::onItemAboutToBeInserted(int index)
{
    beginInsertRows({}, index, index);
}

void PlayQueueViewModel::onItemInserted(int /*index*/)
{
    endInsertRows();
    Q_EMIT countChanged();
    emitQueueStateChanged();
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::onItemAboutToBeRemoved(int index)
{
    beginRemoveRows({}, index, index);
}

void PlayQueueViewModel::onItemRemoved(int /*index*/)
{
    endRemoveRows();
    Q_EMIT countChanged();
    emitQueueStateChanged();
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::onItemAboutToBeMoved(int from, int to)
{
    // QAbstractItemModel::beginMoveRows uses a destination row that
    // is interpreted as "the row in the source ordering BEFORE
    // which the moved items should land". For a move to a higher
    // index, that's `to + 1`; for a move to a lower index, that's
    // `to`.
    const int dest = to > from ? to + 1 : to;
    m_moveInProgress = beginMoveRows({}, from, from, {}, dest);
}

void PlayQueueViewModel::onItemMoved(int /*from*/, int /*to*/)
{
    if (m_moveInProgress) {
        endMoveRows();
        m_moveInProgress = false;
    } else {
        // Rare reset fallback; keeps QML's view consistent.
        beginResetModel();
        endResetModel();
    }
    // The active row may have shifted as a side effect of the move.
    Q_EMIT activeIndexChanged();
    emitQueueStateChanged();
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::onItemChanged(int index)
{
    const auto modelIdx = this->index(index);
    Q_EMIT dataChanged(modelIdx, modelIdx);
    emitQueueStateChanged();
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::onActiveIndexChanged(int /*index*/)
{
    // Refresh the IsActive role on every row that could have flipped.
    // The simplest correct path is to mark the whole list dirty for
    // the IsActive role only.
    const int n = rowCount();
    if (n > 0) {
        Q_EMIT dataChanged(this->index(0), this->index(n - 1),
            { IsActiveRole });
    }
    Q_EMIT activeIndexChanged();
    emitQueueStateChanged();
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::onPlaybackSessionChanged()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (!m_playbackCtrl) {
        return;
    }
    m_hasEmbeddedPlaybackSession = m_playbackCtrl->hasActiveSession();
    emitPlaybackStateChanged();
#endif
}

void PlayQueueViewModel::onPlaybackPositionChanged(double seconds)
{
    m_playbackPositionSeconds = seconds;
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::onPlaybackDurationChanged(double seconds)
{
    m_playbackDurationSeconds = seconds;
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::onPlaybackPausedChanged(bool paused)
{
    m_playbackPaused = paused;
    emitPlaybackStateChanged();
}

void PlayQueueViewModel::emitQueueStateChanged()
{
    Q_EMIT queueStateChanged();
}

void PlayQueueViewModel::emitPlaybackStateChanged()
{
    Q_EMIT playbackStateChanged();
}

} // namespace kinema::ui::qml
