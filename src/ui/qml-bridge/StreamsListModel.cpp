// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/StreamsListModel.h"

#include <KLocalizedString>

#include <QLocale>

namespace kinema::ui::qml {

StreamsListModel::StreamsListModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int StreamsListModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_items.size());
}

QString StreamsListModel::formatSize(std::optional<qint64> bytes)
{
    if (!bytes || *bytes <= 0) {
        return QStringLiteral("\u2014");
    }
    return QLocale::system().formattedDataSize(
        *bytes, 2, QLocale::DataSizeIecFormat);
}

QStringList StreamsListModel::chipsFor(const api::Stream& s)
{
    QStringList chips;
    if (!s.resolution.isEmpty() && s.resolution != QLatin1String("\u2014")) {
        chips.append(s.resolution);
    }
    if (s.rdCached) {
        chips.append(i18nc("@label stream chip", "RD+"));
    } else if (s.rdDownload) {
        chips.append(i18nc("@label stream chip", "RD"));
    }
    if (!s.provider.isEmpty()) {
        chips.append(s.provider);
    }
    return chips;
}

QVariant StreamsListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_items.size()) {
        return {};
    }
    const auto& s = m_items.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
    case ReleaseNameRole:
        return s.releaseName;
    case DetailsTextRole:
        return s.detailsText;
    case ResolutionRole:
        return s.resolution;
    case QualityLabelRole: {
        QString label = s.resolution;
        if (s.rdCached) {
            label += QStringLiteral(" \u00b7 [RD+]");
        } else if (s.rdDownload) {
            label += QStringLiteral(" \u00b7 [RD]");
        }
        return label;
    }
    case SizeBytesRole:
        return s.sizeBytes ? QVariant::fromValue(*s.sizeBytes)
                           : QVariant::fromValue<qint64>(-1);
    case SizeTextRole:
        return formatSize(s.sizeBytes);
    case SeedersRole:
        return s.seeders ? QVariant(*s.seeders) : QVariant(-1);
    case ProviderRole:
        return s.provider;
    case InfoHashRole:
        return s.infoHash;
    case DirectUrlRole:
        return s.directUrl.toString();
    case RdCachedRole:
        return s.rdCached;
    case RdDownloadRole:
        return s.rdDownload;
    case HasMagnetRole:
        return !s.infoHash.isEmpty();
    case HasDirectUrlRole:
        return !s.directUrl.isEmpty();
    case ChipsRole:
        return chipsFor(s);
    case StreamRole:
        return QVariant::fromValue(s);
    default:
        return {};
    }
}

QHash<int, QByteArray> StreamsListModel::roleNames() const
{
    return {
        { StreamRole, "stream" },
        { ReleaseNameRole, "releaseName" },
        { DetailsTextRole, "detailsText" },
        { ResolutionRole, "resolution" },
        { QualityLabelRole, "qualityLabel" },
        { SizeBytesRole, "sizeBytes" },
        { SizeTextRole, "sizeText" },
        { SeedersRole, "seeders" },
        { ProviderRole, "provider" },
        { InfoHashRole, "infoHash" },
        { DirectUrlRole, "directUrl" },
        { RdCachedRole, "rdCached" },
        { RdDownloadRole, "rdDownload" },
        { HasMagnetRole, "hasMagnet" },
        { HasDirectUrlRole, "hasDirectUrl" },
        { ChipsRole, "chips" },
    };
}

void StreamsListModel::resetState(State newState)
{
    if (m_state == newState) {
        return;
    }
    m_state = newState;
    Q_EMIT stateChanged();
}

void StreamsListModel::setIdle()
{
    if (!m_items.isEmpty()) {
        beginResetModel();
        m_items.clear();
        endResetModel();
        Q_EMIT countChanged();
    }
    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    if (!m_emptyExplanation.isEmpty()) {
        m_emptyExplanation.clear();
        Q_EMIT emptyExplanationChanged();
    }
    if (m_releaseDate.isValid()) {
        m_releaseDate = {};
        Q_EMIT releaseDateChanged();
    }
    resetState(State::Idle);
}

void StreamsListModel::setLoading()
{
    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    if (!m_emptyExplanation.isEmpty()) {
        m_emptyExplanation.clear();
        Q_EMIT emptyExplanationChanged();
    }
    if (m_releaseDate.isValid()) {
        m_releaseDate = {};
        Q_EMIT releaseDateChanged();
    }
    resetState(State::Loading);
}

void StreamsListModel::setItems(QList<api::Stream> visible,
    const QString& emptyExplanation)
{
    beginResetModel();
    m_items = std::move(visible);
    endResetModel();
    Q_EMIT countChanged();

    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    if (m_releaseDate.isValid()) {
        m_releaseDate = {};
        Q_EMIT releaseDateChanged();
    }
    if (m_emptyExplanation != emptyExplanation) {
        m_emptyExplanation = emptyExplanation;
        Q_EMIT emptyExplanationChanged();
    }
    resetState(m_items.isEmpty() ? State::Empty : State::Ready);
}

void StreamsListModel::setError(const QString& message)
{
    if (!m_items.isEmpty()) {
        beginResetModel();
        m_items.clear();
        endResetModel();
        Q_EMIT countChanged();
    }
    if (m_errorMessage != message) {
        m_errorMessage = message;
        Q_EMIT errorMessageChanged();
    }
    if (!m_emptyExplanation.isEmpty()) {
        m_emptyExplanation.clear();
        Q_EMIT emptyExplanationChanged();
    }
    if (m_releaseDate.isValid()) {
        m_releaseDate = {};
        Q_EMIT releaseDateChanged();
    }
    resetState(State::Error);
}

void StreamsListModel::setUnreleased(const QDate& date)
{
    if (!m_items.isEmpty()) {
        beginResetModel();
        m_items.clear();
        endResetModel();
        Q_EMIT countChanged();
    }
    if (!m_errorMessage.isEmpty()) {
        m_errorMessage.clear();
        Q_EMIT errorMessageChanged();
    }
    if (!m_emptyExplanation.isEmpty()) {
        m_emptyExplanation.clear();
        Q_EMIT emptyExplanationChanged();
    }
    if (m_releaseDate != date) {
        m_releaseDate = date;
        Q_EMIT releaseDateChanged();
    }
    resetState(State::Unreleased);
}

const api::Stream* StreamsListModel::at(int row) const
{
    if (row < 0 || row >= m_items.size()) {
        return nullptr;
    }
    return &m_items.at(row);
}

} // namespace kinema::ui::qml
