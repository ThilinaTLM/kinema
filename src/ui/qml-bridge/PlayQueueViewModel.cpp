// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/PlayQueueViewModel.h"

#include "controllers/PlayQueueController.h"

#include <KLocalizedString>

namespace kinema::ui::qml {

namespace {

QString subtitleFor(const api::QueueItem& it)
{
    if (it.key.kind != api::MediaKind::Series) {
        return {};
    }
    if (it.key.season && it.key.episode) {
        const auto sxe = QStringLiteral("S%1E%2")
                             .arg(*it.key.season,
                                 /*fieldWidth=*/2, /*base=*/10,
                                 QLatin1Char('0'))
                             .arg(*it.key.episode,
                                 /*fieldWidth=*/2, /*base=*/10,
                                 QLatin1Char('0'));
        if (!it.episodeTitle.isEmpty()) {
            return i18nc("@label queue row episode subtitle, "
                         "%1 SxE, %2 episode title",
                "%1 - %2", sxe, it.episodeTitle);
        }
        return sxe;
    }
    return it.episodeTitle;
}

} // namespace

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
        return subtitleFor(it);
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

void PlayQueueViewModel::retryFailed(int index)
{
    if (m_ctrl) {
        m_ctrl->retryFailed(index);
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
}

void PlayQueueViewModel::onItemAboutToBeInserted(int index)
{
    beginInsertRows({}, index, index);
}

void PlayQueueViewModel::onItemInserted(int /*index*/)
{
    endInsertRows();
    Q_EMIT countChanged();
}

void PlayQueueViewModel::onItemAboutToBeRemoved(int index)
{
    beginRemoveRows({}, index, index);
}

void PlayQueueViewModel::onItemRemoved(int /*index*/)
{
    endRemoveRows();
    Q_EMIT countChanged();
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
}

void PlayQueueViewModel::onItemChanged(int index)
{
    const auto modelIdx = this->index(index);
    Q_EMIT dataChanged(modelIdx, modelIdx);
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
}

} // namespace kinema::ui::qml
