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

QStringList StreamsListModel::tagsFor(const api::Stream& s,
    const core::stream_tokens::Tokens& t)
{
    Q_UNUSED(s);
    QStringList tags;
    // Codec (with 10-bit suffix when applicable).
    const auto codec = core::stream_tokens::codecLabel(t.codec, t.tenBit);
    if (!codec.isEmpty()) {
        tags << codec;
    }
    // HDR profile (DV / HDR10+ / HDR10).
    const auto hdr = core::stream_tokens::hdrLabel(t.hdr);
    if (!hdr.isEmpty()) {
        tags << hdr;
    }
    // Languages — small uppercased ISO codes ("EN", "FR").
    for (const auto& lang : t.languages) {
        tags << lang.toUpper();
    }
    // Multi-audio marker (Dual / Multi).
    if (t.multiAudio) {
        tags << i18nc("@label stream chip multi audio", "Multi");
    }
    // Release group (only when meaningful).
    if (!t.releaseGroup.isEmpty()) {
        tags << t.releaseGroup;
    }
    return tags;
}

QString StreamsListModel::summaryLineFor(const api::Stream& s,
    const core::stream_tokens::Tokens& t)
{
    Q_UNUSED(s);
    QStringList parts;
    const auto src = core::stream_tokens::sourceLabel(t.source);
    if (!src.isEmpty()) {
        parts << src;
    }
    const auto codec = core::stream_tokens::codecLabel(t.codec, t.tenBit);
    if (!codec.isEmpty()) {
        parts << codec;
    }
    const auto hdr = core::stream_tokens::hdrLabel(t.hdr);
    if (!hdr.isEmpty()) {
        parts << hdr;
    }
    if (!t.audio.isEmpty()) {
        parts << t.audio.join(QStringLiteral(" \u00b7 "));
    }
    return parts.join(QStringLiteral(" \u00b7 "));
}

const core::stream_tokens::Tokens& StreamsListModel::tokensAt(int index) const
{
    auto it = m_tokenCache.find(index);
    if (it == m_tokenCache.end()) {
        it = m_tokenCache.insert(index, core::stream_tokens::parse(m_items.at(index)));
    }
    return it.value();
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
    case SourceRole:
        return core::stream_tokens::sourceLabel(tokensAt(index.row()).source);
    case CodecRole: {
        const auto& t = tokensAt(index.row());
        return core::stream_tokens::codecLabel(t.codec, t.tenBit);
    }
    case HdrRole:
        return core::stream_tokens::hdrLabel(tokensAt(index.row()).hdr);
    case AudioSummaryRole:
        return tokensAt(index.row()).audio.join(QStringLiteral(" \u00b7 "));
    case LanguagesRole:
        return tokensAt(index.row()).languages;
    case MultiAudioRole:
        return tokensAt(index.row()).multiAudio;
    case ReleaseGroupRole:
        return tokensAt(index.row()).releaseGroup;
    case SummaryLineRole:
        return summaryLineFor(s, tokensAt(index.row()));
    case TagsRole:
        return tagsFor(s, tokensAt(index.row()));
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
        { SourceRole, "source" },
        { CodecRole, "codec" },
        { HdrRole, "hdr" },
        { AudioSummaryRole, "audioSummary" },
        { LanguagesRole, "languages" },
        { MultiAudioRole, "multiAudio" },
        { ReleaseGroupRole, "releaseGroup" },
        { SummaryLineRole, "summaryLine" },
        { TagsRole, "tags" },
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
        m_tokenCache.clear();
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
    m_tokenCache.clear();
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
        m_tokenCache.clear();
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
        m_tokenCache.clear();
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
