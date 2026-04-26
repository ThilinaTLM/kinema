// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/ResultsListModel.h"

#include <QVariant>

namespace kinema::ui::qml {

ResultsListModel::ResultsListModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int ResultsListModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant ResultsListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& r = m_rows.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
        return r.year
            ? QStringLiteral("%1 (%2)").arg(r.title).arg(*r.year)
            : r.title;
    case ImdbIdRole:
        return r.imdbId;
    case YearRole:
        return r.year ? QVariant(*r.year) : QVariant();
    case PosterUrlRole:
        // Returned as QString (not QUrl) so the QML delegate can
        // `encodeURIComponent(...)` it without coercion surprises,
        // matching the DiscoverSectionModel convention.
        return r.poster.toString();
    case DescriptionRole:
        return r.description;
    case RatingRole:
        return r.imdbRating ? QVariant(*r.imdbRating) : QVariant();
    case KindRole:
        return QVariant::fromValue(r.kind);
    case SummaryRole:
        return QVariant::fromValue(r);
    default:
        return {};
    }
}

QHash<int, QByteArray> ResultsListModel::roleNames() const
{
    auto names = QAbstractListModel::roleNames();
    names.insert(ImdbIdRole, "imdbId");
    names.insert(TitleRole, "title");
    names.insert(YearRole, "year");
    names.insert(PosterUrlRole, "posterUrl");
    names.insert(DescriptionRole, "description");
    names.insert(RatingRole, "rating");
    names.insert(KindRole, "kind");
    names.insert(SummaryRole, "summary");
    return names;
}

void ResultsListModel::resetState(State newState)
{
    if (m_state == newState) {
        return;
    }
    m_state = newState;
    Q_EMIT stateChanged();
}

void ResultsListModel::setIdle()
{
    if (!m_rows.isEmpty()) {
        beginResetModel();
        m_rows.clear();
        endResetModel();
        Q_EMIT countChanged();
    }
    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    resetState(State::Idle);
}

void ResultsListModel::setLoading()
{
    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    resetState(State::Loading);
}

void ResultsListModel::setResults(QList<api::MetaSummary> rows)
{
    beginResetModel();
    m_rows = std::move(rows);
    endResetModel();
    Q_EMIT countChanged();

    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    resetState(m_rows.isEmpty() ? State::Empty : State::Results);
}

void ResultsListModel::setError(const QString& message)
{
    if (!m_rows.isEmpty()) {
        beginResetModel();
        m_rows.clear();
        endResetModel();
        Q_EMIT countChanged();
    }
    if (m_errorMessage != message) {
        m_errorMessage = message;
        Q_EMIT errorMessageChanged();
    }
    resetState(State::Error);
}

const api::MetaSummary* ResultsListModel::at(int row) const
{
    if (row < 0 || row >= m_rows.size()) {
        return nullptr;
    }
    return &m_rows.at(row);
}

} // namespace kinema::ui::qml
