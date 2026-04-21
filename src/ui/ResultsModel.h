// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Types.h"

#include <QAbstractListModel>
#include <QList>

namespace kinema::ui {

/**
 * List model backing the search results grid. Stores MetaSummary rows and
 * exposes them through custom roles so the delegate can render a poster card
 * without re-parsing anything.
 */
class ResultsModel : public QAbstractListModel
{
    Q_OBJECT
public:
    enum Roles {
        ImdbIdRole = Qt::UserRole + 1,
        TitleRole,
        YearRole,
        PosterUrlRole,
        DescriptionRole,
        RatingRole,
        KindRole,
        SummaryRole, ///< returns the full MetaSummary as a QVariant
    };
    Q_ENUM(Roles)

    explicit ResultsModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reset(QList<api::MetaSummary> rows);
    const QList<api::MetaSummary>& rows() const noexcept { return m_rows; }
    const api::MetaSummary* at(int row) const;

private:
    QList<api::MetaSummary> m_rows;
};

} // namespace kinema::ui
