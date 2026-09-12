// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/library/LibraryRailModel.h"

namespace kinema::ui::qml {

LibraryRailModel::LibraryRailModel(QObject* parent) : ListModelBase<LibraryRailRow>(parent) { }

QVariant LibraryRailModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& r = m_rows.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
        return r.title;
    case KindRole:
        return static_cast<int>(r.kind);
    case ImdbIdRole:
        return r.imdbId;
    case SeasonRole:
        return r.season ? QVariant(*r.season) : QVariant();
    case EpisodeRole:
        return r.episode ? QVariant(*r.episode) : QVariant();
    case PosterUrlRole:
        return r.posterUrl;
    case BackdropUrlRole:
        return r.backdropUrl;
    case ThumbnailUrlRole:
        return r.thumbnailUrl;
    case PrimaryLineRole:
        return r.primaryLine;
    case SecondaryLineRole:
        return r.secondaryLine;
    case TertiaryLineRole:
        return r.tertiaryLine;
    case ProgressRole:
        return r.progress;
    default:
        return {};
    }
}

QHash<int, QByteArray> LibraryRailModel::roleNames() const
{
    return {
        {KindRole, "kind"},
        {ImdbIdRole, "imdbId"},
        {SeasonRole, "season"},
        {EpisodeRole, "episode"},
        {TitleRole, "title"},
        {PosterUrlRole, "posterUrl"},
        {BackdropUrlRole, "backdropUrl"},
        {ThumbnailUrlRole, "thumbnailUrl"},
        {PrimaryLineRole, "primaryLine"},
        {SecondaryLineRole, "secondaryLine"},
        {TertiaryLineRole, "tertiaryLine"},
        {ProgressRole, "progress"},
    };
}

void LibraryRailModel::setRows(QList<LibraryRailRow> rows)
{
    replaceRows(std::move(rows));
    Q_EMIT countChanged();
}

} // namespace kinema::ui::qml
