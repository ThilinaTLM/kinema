// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/TorrentsModel.h"

#include <KLocalizedString>

#include <QIcon>
#include <QLocale>

namespace kinema::ui {

TorrentsModel::TorrentsModel(QObject* parent)
    : QAbstractTableModel(parent)
{
}

int TorrentsModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

int TorrentsModel::columnCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return ColCount;
}

QString TorrentsModel::formatSize(std::optional<qint64> bytes)
{
    if (!bytes || *bytes <= 0) {
        return QStringLiteral("—");
    }
    return QLocale::system().formattedDataSize(*bytes, 2, QLocale::DataSizeIecFormat);
}

QVariant TorrentsModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& s = m_rows.at(index.row());

    if (role == StreamRole) {
        return QVariant::fromValue(s);
    }

    if (role == SortKeyRole) {
        switch (index.column()) {
        case ColSize:
            return QVariant::fromValue(s.sizeBytes.value_or(0));
        case ColSeeders:
            return QVariant::fromValue(s.seeders.value_or(0));
        case ColQuality: {
            // Sort quality by numeric resolution so 2160p > 1080p > 720p.
            const auto& r = s.resolution;
            if (r == QLatin1String("2160p")) {
                return 2160;
            }
            if (r == QLatin1String("1440p")) {
                return 1440;
            }
            if (r == QLatin1String("1080p")) {
                return 1080;
            }
            if (r == QLatin1String("720p")) {
                return 720;
            }
            if (r == QLatin1String("480p")) {
                return 480;
            }
            if (r == QLatin1String("360p")) {
                return 360;
            }
            return 0;
        }
        default:
            break;
        }
    }

    if (role == Qt::DecorationRole && index.column() == ColQuality) {
        if (s.rdCached) {
            return QIcon::fromTheme(QStringLiteral("emblem-success"),
                QIcon::fromTheme(QStringLiteral("emblem-checked")));
        }
        if (s.rdDownload) {
            return QIcon::fromTheme(QStringLiteral("emblem-warning"));
        }
        return {};
    }

    if (role != Qt::DisplayRole && role != Qt::ToolTipRole) {
        return {};
    }

    switch (index.column()) {
    case ColQuality: {
        QString label = s.resolution;
        if (s.rdCached) {
            label += QStringLiteral(" · [RD+]");
        } else if (s.rdDownload) {
            label += QStringLiteral(" · [RD]");
        }
        return label;
    }
    case ColRelease:
        if (role == Qt::ToolTipRole) {
            QString tip = s.releaseName;
            if (!s.detailsText.isEmpty()) {
                tip += QLatin1Char('\n') + s.detailsText;
            }
            if (s.rdCached) {
                tip += QLatin1Char('\n')
                    + i18nc("@info:tooltip", "Cached on Real-Debrid");
            } else if (s.rdDownload) {
                tip += QLatin1Char('\n')
                    + i18nc("@info:tooltip",
                        "Uncached \u2014 Real-Debrid will download to cache on demand.");
            }
            return tip;
        }
        return s.releaseName;
    case ColSize:
        return formatSize(s.sizeBytes);
    case ColSeeders:
        return s.seeders ? QString::number(*s.seeders) : QStringLiteral("—");
    case ColProvider:
        return s.provider.isEmpty() ? QStringLiteral("—") : s.provider;
    default:
        return {};
    }
}

QVariant TorrentsModel::headerData(int section, Qt::Orientation o, int role) const
{
    if (role != Qt::DisplayRole || o != Qt::Horizontal) {
        return {};
    }
    switch (section) {
    case ColQuality:
        return i18nc("@title:column", "Quality");
    case ColRelease:
        return i18nc("@title:column", "Release");
    case ColSize:
        return i18nc("@title:column", "Size");
    case ColSeeders:
        return i18nc("@title:column", "Seeders");
    case ColProvider:
        return i18nc("@title:column", "Provider");
    default:
        return {};
    }
}

void TorrentsModel::reset(QList<api::Stream> rows)
{
    beginResetModel();
    m_rows = std::move(rows);
    endResetModel();
}

const api::Stream* TorrentsModel::at(int row) const
{
    if (row < 0 || row >= m_rows.size()) {
        return nullptr;
    }
    return &m_rows.at(row);
}

} // namespace kinema::ui
