// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Types.h"

#include <QAbstractListModel>
#include <QList>

namespace kinema::ui {

/**
 * List model for a single season's episodes. The owning SeriesPicker
 * rebuilds the model whenever the user switches seasons.
 *
 * Roles are designed so EpisodeDelegate can paint without touching the
 * Episode struct: number, title, release date and thumbnail URL are all
 * reachable directly.
 */
class EpisodesModel : public QAbstractListModel
{
    Q_OBJECT
public:
    enum Roles {
        NumberRole = Qt::UserRole + 1,
        TitleRole,
        DescriptionRole,
        ReleasedRole,
        /// True when the release date is strictly after today. Used by
        /// EpisodeDelegate to draw the "Upcoming" pill.
        FutureReleaseRole,
        ThumbnailUrlRole,
        EpisodeRole, ///< full Episode struct
    };
    Q_ENUM(Roles)

    explicit EpisodesModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;

    void reset(QList<api::Episode> rows);
    const QList<api::Episode>& rows() const noexcept { return m_rows; }
    const api::Episode* at(int row) const;

private:
    QList<api::Episode> m_rows;
};

} // namespace kinema::ui
