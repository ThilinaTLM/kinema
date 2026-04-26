// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/SubtitleResultsModel.h"

namespace kinema::ui::widgets {

SubtitleResultsModel::SubtitleResultsModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int SubtitleResultsModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant SubtitleResultsModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= static_cast<int>(m_rows.size())) {
        return {};
    }
    const auto& h = m_rows.at(index.row());
    switch (role) {
        case Qt::DisplayRole:
            // Used by accessibility and as a fallback for stock views.
            return h.releaseName.isEmpty() ? h.fileName : h.releaseName;
        case FileIdRole:           return h.fileId;
        case ReleaseRole:          return h.releaseName;
        case FileNameRole:         return h.fileName;
        case LanguageRole:         return h.language;
        case LanguageNameRole:     return h.languageName;
        case FormatRole:           return h.format;
        case DownloadCountRole:    return h.downloadCount;
        case RatingRole:           return h.rating;
        case UploaderRole:         return h.uploader;
        case FpsRole:              return h.fps;
        case HearingImpairedRole:  return h.hearingImpaired;
        case ForeignPartsOnlyRole: return h.foreignPartsOnly;
        case MoviehashMatchRole:   return h.moviehashMatch;
        case CachedRole:           return m_cached.contains(h.fileId);
        case ActiveRole:           return m_active.contains(h.fileId);
        default:                   return {};
    }
}

QHash<int, QByteArray> SubtitleResultsModel::roleNames() const
{
    return {
        { FileIdRole,           "fileId" },
        { ReleaseRole,          "release" },
        { FileNameRole,         "fileName" },
        { LanguageRole,         "language" },
        { LanguageNameRole,     "languageName" },
        { FormatRole,           "format" },
        { DownloadCountRole,    "downloadCount" },
        { RatingRole,           "rating" },
        { UploaderRole,         "uploader" },
        { FpsRole,              "fps" },
        { HearingImpairedRole,  "hearingImpaired" },
        { ForeignPartsOnlyRole, "foreignPartsOnly" },
        { MoviehashMatchRole,   "moviehashMatch" },
        { CachedRole,           "cached" },
        { ActiveRole,           "active" },
    };
}

void SubtitleResultsModel::setHits(const QList<api::SubtitleHit>& hits)
{
    beginResetModel();
    m_rows = hits;
    endResetModel();
}

void SubtitleResultsModel::setCachedFileIds(const QSet<QString>& ids)
{
    if (m_cached == ids) {
        return;
    }
    m_cached = ids;
    if (!m_rows.isEmpty()) {
        Q_EMIT dataChanged(index(0), index(static_cast<int>(m_rows.size()) - 1),
            { CachedRole });
    }
}

void SubtitleResultsModel::setActiveFileIds(const QSet<QString>& ids)
{
    if (m_active == ids) {
        return;
    }
    m_active = ids;
    if (!m_rows.isEmpty()) {
        Q_EMIT dataChanged(index(0), index(static_cast<int>(m_rows.size()) - 1),
            { ActiveRole });
    }
}

} // namespace kinema::ui::widgets
