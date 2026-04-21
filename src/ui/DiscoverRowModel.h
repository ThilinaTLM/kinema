// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"

#include <QAbstractListModel>
#include <QList>

namespace kinema::ui {

/**
 * List model backing one horizontal strip on the Discover page (or the
 * "More like this" strip on a detail pane). Stores DiscoverItem rows
 * and exposes them through custom roles so the delegate can render a
 * poster card without re-parsing anything.
 *
 * Intentionally parallel in shape to ResultsModel but over DiscoverItem
 * (which carries a TMDB id, not an IMDB id). The two models are kept
 * separate because the click-through semantics differ: clicking a
 * DiscoverItem triggers a TMDB→IMDB resolution step before the rest
 * of the pipeline runs.
 */
class DiscoverRowModel : public QAbstractListModel
{
    Q_OBJECT
public:
    enum Roles {
        TmdbIdRole = Qt::UserRole + 1,
        TitleRole,
        YearRole,
        PosterUrlRole,
        OverviewRole,
        VoteAverageRole,
        KindRole,
        ItemRole, ///< returns the full DiscoverItem as a QVariant
    };
    Q_ENUM(Roles)

    explicit DiscoverRowModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reset(QList<api::DiscoverItem> rows);

    /// Append more rows to the existing list (used by the Browse page's
    /// "Load more" pagination). Emits standard rowsInserted signals so
    /// the attached DiscoverGridView reflows its fixed height.
    void append(const QList<api::DiscoverItem>& more);

    const QList<api::DiscoverItem>& rows() const noexcept { return m_rows; }
    const api::DiscoverItem* at(int row) const;

private:
    QList<api::DiscoverItem> m_rows;
};

} // namespace kinema::ui
