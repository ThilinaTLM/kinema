// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/QueueItem.h"

#include <QAbstractListModel>
#include <QHash>
#include <QString>

namespace kinema::controllers {
class PlayQueueController;
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

public Q_SLOTS:
    void playAt(int index);
    void removeAt(int index);
    void moveTo(int from, int to);
    void clearAll();
    void retryFailed(int index);

Q_SIGNALS:
    void countChanged();
    void activeIndexChanged();

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

private:
    controllers::PlayQueueController* m_ctrl;
    bool m_moveInProgress = false;
};

} // namespace kinema::ui::qml
