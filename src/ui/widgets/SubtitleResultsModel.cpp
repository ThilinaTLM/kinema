// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/SubtitleResultsModel.h"

#include <KLocalizedString>

#include <QIcon>
#include <QLocale>
#include <QPainter>
#include <QPixmap>
#include <QStringList>

namespace kinema::ui::widgets {

namespace {

QString formatRating(double rating)
{
    if (rating <= 0.0) {
        return {};
    }
    return QStringLiteral(u"\u2605 %1").arg(rating, 0, 'f', 1);
}

/// Composite the HI / FPO badges into a single horizontal pixmap so
/// the Flags column can sit on stock `Qt::DecorationRole` rendering.
/// Built once per cell access; rows are few and `data()` calls are
/// view-bound, so the cost is negligible compared to a custom delegate.
QIcon flagsIcon(bool hi, bool fpo)
{
    if (!hi && !fpo) {
        return {};
    }
    const QIcon hiIcon = hi
        ? QIcon::fromTheme(QStringLiteral("audio-headset-symbolic"),
              QIcon::fromTheme(QStringLiteral("audio-headset")))
        : QIcon {};
    const QIcon fpoIcon = fpo
        ? QIcon::fromTheme(QStringLiteral("flag-symbolic"),
              QIcon::fromTheme(QStringLiteral("flag")))
        : QIcon {};
    if (hi && !fpo) return hiIcon;
    if (fpo && !hi) return fpoIcon;

    const int sz = 16;
    QPixmap pm(sz * 2 + 2, sz);
    pm.fill(Qt::transparent);
    QPainter p(&pm);
    p.drawPixmap(0, 0, hiIcon.pixmap(sz, sz));
    p.drawPixmap(sz + 2, 0, fpoIcon.pixmap(sz, sz));
    p.end();
    return QIcon(pm);
}

QString flagsTooltip(bool hi, bool fpo)
{
    QStringList parts;
    if (hi) {
        parts << i18nc("@info:tooltip subtitle flag",
            "Hearing impaired");
    }
    if (fpo) {
        parts << i18nc("@info:tooltip subtitle flag",
            "Foreign parts only");
    }
    return parts.join(QStringLiteral(", "));
}

} // namespace

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

int SubtitleResultsModel::columnCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return ColumnCount;
}

QVariant SubtitleResultsModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= static_cast<int>(m_rows.size())) {
        return {};
    }
    const auto& h = m_rows.at(index.row());

    // Custom roles only resolve on column 0. Selection lookup uses
    // `current.siblingAtColumn(0).data(...)`.
    if (role >= FileIdRole && role <= ActiveRole) {
        if (index.column() != ReleaseColumn) {
            return {};
        }
        switch (role) {
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

    switch (role) {
        case Qt::DisplayRole:
            switch (index.column()) {
                case ReleaseColumn:
                    return h.releaseName.isEmpty() ? h.fileName : h.releaseName;
                case LangColumn:
                    return h.language.toUpper();
                case FormatColumn:
                    return h.format;
                case DownloadsColumn:
                    if (h.downloadCount <= 0) return QString {};
                    return i18ncp("@info downloads count",
                        "%1 download", "%1 downloads", h.downloadCount);
                case RatingColumn:
                    return formatRating(h.rating);
                case FlagsColumn:
                    return QString {};
                default:
                    return {};
            }

        case Qt::EditRole:
            // Numeric sort hints for the proxy model.
            switch (index.column()) {
                case DownloadsColumn: return h.downloadCount;
                case RatingColumn:    return h.rating;
                default:              return data(index, Qt::DisplayRole);
            }

        case Qt::DecorationRole:
            switch (index.column()) {
                case ReleaseColumn: {
                    // Status emblem on the row's primary cell.
                    if (m_active.contains(h.fileId)) {
                        return QIcon::fromTheme(QStringLiteral("emblem-checked"),
                            QIcon::fromTheme(QStringLiteral("dialog-ok")));
                    }
                    if (m_cached.contains(h.fileId)) {
                        return QIcon::fromTheme(QStringLiteral("emblem-downloads"),
                            QIcon::fromTheme(QStringLiteral("download")));
                    }
                    if (h.moviehashMatch) {
                        return QIcon::fromTheme(QStringLiteral("emblem-favorite"),
                            QIcon::fromTheme(QStringLiteral("starred-symbolic")));
                    }
                    return {};
                }
                case FlagsColumn: {
                    return flagsIcon(h.hearingImpaired, h.foreignPartsOnly);
                }
                default:
                    return {};
            }

        case Qt::TextAlignmentRole:
            switch (index.column()) {
                case DownloadsColumn:
                case RatingColumn:
                    return int(Qt::AlignRight | Qt::AlignVCenter);
                case LangColumn:
                case FormatColumn:
                case FlagsColumn:
                    return int(Qt::AlignCenter);
                default:
                    return int(Qt::AlignLeft | Qt::AlignVCenter);
            }

        case Qt::ToolTipRole:
            switch (index.column()) {
                case ReleaseColumn: {
                    QStringList lines;
                    if (!h.fileName.isEmpty()) lines << h.fileName;
                    if (m_active.contains(h.fileId)) {
                        lines << i18nc("@info:tooltip",
                            "Currently loaded in the player");
                    } else if (m_cached.contains(h.fileId)) {
                        lines << i18nc("@info:tooltip",
                            "Already downloaded");
                    }
                    if (h.moviehashMatch) {
                        lines << i18nc("@info:tooltip",
                            "Matches the file's moviehash");
                    }
                    return lines.join(QStringLiteral("\n"));
                }
                case LangColumn:
                    return h.languageName;
                case FlagsColumn:
                    return flagsTooltip(h.hearingImpaired, h.foreignPartsOnly);
                default:
                    return {};
            }

        default:
            return {};
    }
}

QVariant SubtitleResultsModel::headerData(int section,
    Qt::Orientation orientation, int role) const
{
    if (orientation != Qt::Horizontal || role != Qt::DisplayRole) {
        return QAbstractListModel::headerData(section, orientation, role);
    }
    switch (section) {
        case ReleaseColumn:
            return i18nc("@title:column subtitle release name", "Release");
        case LangColumn:
            return i18nc("@title:column subtitle language", "Lang");
        case FormatColumn:
            return i18nc("@title:column subtitle file format", "Format");
        case DownloadsColumn:
            return i18nc("@title:column subtitle download count",
                "Downloads");
        case RatingColumn:
            return i18nc("@title:column subtitle rating", "Rating");
        case FlagsColumn:
            return i18nc("@title:column subtitle flags (HI / FPO)", "Flags");
        default:
            return {};
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
        const int last = static_cast<int>(m_rows.size()) - 1;
        Q_EMIT dataChanged(index(0, ReleaseColumn),
            index(last, FlagsColumn),
            { CachedRole, Qt::DecorationRole, Qt::ToolTipRole });
    }
}

void SubtitleResultsModel::setActiveFileIds(const QSet<QString>& ids)
{
    if (m_active == ids) {
        return;
    }
    m_active = ids;
    if (!m_rows.isEmpty()) {
        const int last = static_cast<int>(m_rows.size()) - 1;
        Q_EMIT dataChanged(index(0, ReleaseColumn),
            index(last, FlagsColumn),
            { ActiveRole, Qt::DecorationRole, Qt::ToolTipRole });
    }
}

} // namespace kinema::ui::widgets
