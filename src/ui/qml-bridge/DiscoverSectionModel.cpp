// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DiscoverSectionModel.h"

namespace kinema::ui::qml {

DiscoverSectionModel::DiscoverSectionModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

DiscoverSectionModel::DiscoverSectionModel(QString title, QObject* parent)
    : QAbstractListModel(parent)
    , m_title(std::move(title))
{
}

void DiscoverSectionModel::setTitle(const QString& title)
{
    if (m_title == title) {
        return;
    }
    m_title = title;
    Q_EMIT titleChanged();
}

int DiscoverSectionModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_items.size());
}

QVariant DiscoverSectionModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_items.size()) {
        return {};
    }
    const auto& it = m_items.at(index.row());

    switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
        return it.title;
    case TmdbIdRole:
        return it.tmdbId;
    case YearRole:
        return it.year ? QVariant(*it.year) : QVariant();
    case PosterUrlRole:
        // QML treats a QUrl directly fine, but the image provider
        // expects a percent-encoded `u=` parameter; we return the
        // raw URL as a string and let QML do the encode at the
        // delegate boundary. Empty URL → empty string so QML can
        // skip building a request.
        return it.poster.isValid() ? it.poster.toString() : QString();
    case OverviewRole:
        return it.overview;
    case VoteAverageRole:
        return it.voteAverage ? QVariant(*it.voteAverage) : QVariant();
    case KindRole:
        // Expose as int so QML can compare to api::MediaKind via
        // the enum's underlying type without needing a Q_ENUM
        // declaration on the alias.
        return static_cast<int>(it.kind);
    case ItemRole:
        return QVariant::fromValue(it);
    case ProgressRole: {
        if (index.row() < m_progress.size()) {
            return m_progress.at(index.row());
        }
        return -1.0;
    }
    case LastReleaseRole: {
        if (index.row() < m_lastReleases.size()) {
            return m_lastReleases.at(index.row());
        }
        return QString();
    }
    default:
        return {};
    }
}

QHash<int, QByteArray> DiscoverSectionModel::roleNames() const
{
    return {
        { TmdbIdRole, "tmdbId" },
        { TitleRole, "title" },
        { YearRole, "year" },
        { PosterUrlRole, "posterUrl" },
        { OverviewRole, "overview" },
        { VoteAverageRole, "voteAverage" },
        { KindRole, "kind" },
        { ItemRole, "item" },
        { ProgressRole, "progress" },
        { LastReleaseRole, "lastRelease" },
    };
}

void DiscoverSectionModel::resetState(State newState)
{
    if (m_state == newState) {
        return;
    }
    m_state = newState;
    Q_EMIT stateChanged();
}

void DiscoverSectionModel::setLoading()
{
    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    resetState(State::Loading);
}

void DiscoverSectionModel::setItems(QList<api::DiscoverItem> items)
{
    beginResetModel();
    m_items = std::move(items);
    // Overlay data is meaningful only for the matching item set.
    m_progress.clear();
    m_lastReleases.clear();
    endResetModel();
    Q_EMIT countChanged();

    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    resetState(m_items.isEmpty() ? State::Empty : State::Ready);
}

void DiscoverSectionModel::appendItems(QList<api::DiscoverItem> items)
{
    if (items.isEmpty()) {
        return;
    }
    const int first = static_cast<int>(m_items.size());
    const int last = first + static_cast<int>(items.size()) - 1;
    beginInsertRows({}, first, last);
    m_items.append(items);
    endInsertRows();
    Q_EMIT countChanged();

    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    resetState(State::Ready);
}

void DiscoverSectionModel::setError(const QString& message)
{
    if (m_errorMessage != message) {
        m_errorMessage = message;
        Q_EMIT errorMessageChanged();
    }
    resetState(State::Error);
}

void DiscoverSectionModel::setProgressList(QList<double> progress)
{
    m_progress = std::move(progress);
    if (!m_items.isEmpty()) {
        Q_EMIT dataChanged(index(0), index(rowCount() - 1),
            { ProgressRole });
    }
}

void DiscoverSectionModel::setLastReleaseList(QStringList releases)
{
    m_lastReleases = std::move(releases);
    if (!m_items.isEmpty()) {
        Q_EMIT dataChanged(index(0), index(rowCount() - 1),
            { LastReleaseRole });
    }
}

const api::DiscoverItem* DiscoverSectionModel::itemAt(int row) const
{
    if (row < 0 || row >= m_items.size()) {
        return nullptr;
    }
    return &m_items.at(row);
}

} // namespace kinema::ui::qml
