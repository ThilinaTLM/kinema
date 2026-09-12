// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/streams/StreamsListModel.h"

#include <QLocale>

#include <KLocalizedString>

namespace kinema::ui::qml {

StreamsListModel::StreamsListModel(QObject* parent) : QAbstractListModel(parent) { }

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
    return QLocale::system().formattedDataSize(*bytes, 2, QLocale::DataSizeIecFormat);
}

QStringList StreamsListModel::chipsFor(const domain::Stream& s)
{
    QStringList chips;
    if (!s.resolution.isEmpty() && s.resolution != QLatin1String("\u2014")) {
        chips.append(s.resolution);
    }
    if (!s.provider.isEmpty()) {
        chips.append(s.provider);
    }
    return chips;
}

QStringList StreamsListModel::tagsFor(const domain::Stream& s, const core::stream_tokens::Tokens& t)
{
    Q_UNUSED(s);
    QStringList tags;
    // Keep the chip row for secondary metadata only. Codec / HDR /
    // audio live in `summaryLineFor()` so the row doesn't repeat the
    // same technical facts twice.
    for (const auto& lang : t.languages) {
        tags << lang.toUpper();
    }
    if (t.multiAudio) {
        tags << i18nc("@label stream chip multi audio", "Multi");
    }
    if (!t.releaseGroup.isEmpty()) {
        tags << t.releaseGroup;
    }
    return tags;
}

QString StreamsListModel::summaryLineFor(const domain::Stream& s,
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

const core::stream_tokens::PackHint& StreamsListModel::packHintAt(int index) const
{
    auto it = m_packCache.find(index);
    if (it == m_packCache.end()) {
        it = m_packCache.insert(index,
                                core::stream_tokens::classifyPack(m_items.at(index), m_mediaKind));
    }
    return it.value();
}

QVariant StreamsListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_items.size()) {
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
    case QualityLabelRole:
        return s.resolution;
    case SizeBytesRole:
        return s.sizeBytes ? QVariant::fromValue(*s.sizeBytes) : QVariant::fromValue<qint64>(-1);
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
    case DebridProviderRole:
        return domain::providerToString(s.debridProvider);
    case DebridCachedRole:
        return s.debridCached;
    case PackKindRole:
        return core::stream_tokens::packKindToken(packHintAt(index.row()).kind);
    case PackLabelRole:
        return core::stream_tokens::packLabel(packHintAt(index.row()).kind);
    case PackClaimRole:
        return packHintAt(index.row()).claim;
    case StreamRole:
        return QVariant::fromValue(s);
    default:
        return {};
    }
}

QHash<int, QByteArray> StreamsListModel::roleNames() const
{
    return {
        {StreamRole, "stream"},
        {ReleaseNameRole, "releaseName"},
        {DetailsTextRole, "detailsText"},
        {ResolutionRole, "resolution"},
        {QualityLabelRole, "qualityLabel"},
        {SizeBytesRole, "sizeBytes"},
        {SizeTextRole, "sizeText"},
        {SeedersRole, "seeders"},
        {ProviderRole, "provider"},
        {InfoHashRole, "infoHash"},
        {DirectUrlRole, "directUrl"},
        {HasMagnetRole, "hasMagnet"},
        {HasDirectUrlRole, "hasDirectUrl"},
        {ChipsRole, "chips"},
        {SourceRole, "source"},
        {CodecRole, "codec"},
        {HdrRole, "hdr"},
        {AudioSummaryRole, "audioSummary"},
        {LanguagesRole, "languages"},
        {MultiAudioRole, "multiAudio"},
        {ReleaseGroupRole, "releaseGroup"},
        {SummaryLineRole, "summaryLine"},
        {TagsRole, "tags"},
        {DebridProviderRole, "debridProvider"},
        {DebridCachedRole, "debridCached"},
        {PackKindRole, "packKind"},
        {PackLabelRole, "packLabel"},
        {PackClaimRole, "packClaim"},
    };
}

void StreamsListModel::setMediaKind(domain::MediaKind kind)
{
    if (m_mediaKind == kind) {
        return;
    }
    m_mediaKind = kind;
    // The pack classifier consults the kind on every row, so the
    // existing cache is now stale. Tokens are kind-independent and
    // stay valid. We don't emit per-row dataChanged because the
    // expected call-site is the owning VM constructor, before any
    // rows have been inserted; if a runtime caller ever flips the
    // kind we still need rows to repaint, so issue a full reset.
    if (!m_items.isEmpty()) {
        beginResetModel();
        m_packCache.clear();
        endResetModel();
    } else {
        m_packCache.clear();
    }
}

void StreamsListModel::resetState(State newState)
{
    if (m_state == newState) {
        return;
    }
    m_state = newState;
    Q_EMIT stateChanged();
}

void StreamsListModel::clearItemsIfAny()
{
    if (m_items.isEmpty()) {
        return;
    }
    beginResetModel();
    m_items.clear();
    m_tokenCache.clear();
    m_packCache.clear();
    endResetModel();
    Q_EMIT countChanged();
}

void StreamsListModel::clearErrorIfAny()
{
    if (m_errorMessage.isEmpty()) {
        return;
    }
    m_errorMessage.clear();
    Q_EMIT errorMessageChanged();
}

void StreamsListModel::clearEmptyExplanationIfAny()
{
    if (m_emptyExplanation.isEmpty()) {
        return;
    }
    m_emptyExplanation.clear();
    Q_EMIT emptyExplanationChanged();
}

void StreamsListModel::clearReleaseDateIfAny()
{
    if (!m_releaseDate.isValid()) {
        return;
    }
    m_releaseDate = {};
    Q_EMIT releaseDateChanged();
}

void StreamsListModel::setIdle()
{
    clearItemsIfAny();
    clearErrorIfAny();
    clearEmptyExplanationIfAny();
    clearReleaseDateIfAny();
    resetState(State::Idle);
}

void StreamsListModel::setLoading()
{
    clearErrorIfAny();
    clearEmptyExplanationIfAny();
    clearReleaseDateIfAny();
    resetState(State::Loading);
}

void StreamsListModel::setItems(QList<domain::Stream> visible, const QString& emptyExplanation)
{
    beginResetModel();
    m_items = std::move(visible);
    m_tokenCache.clear();
    m_packCache.clear();
    endResetModel();
    Q_EMIT countChanged();

    clearErrorIfAny();
    clearReleaseDateIfAny();
    if (m_emptyExplanation != emptyExplanation) {
        m_emptyExplanation = emptyExplanation;
        Q_EMIT emptyExplanationChanged();
    }
    resetState(m_items.isEmpty() ? State::Empty : State::Ready);
}

void StreamsListModel::setError(const QString& message)
{
    clearItemsIfAny();
    if (m_errorMessage != message) {
        m_errorMessage = message;
        Q_EMIT errorMessageChanged();
    }
    clearEmptyExplanationIfAny();
    clearReleaseDateIfAny();
    resetState(State::Error);
}

void StreamsListModel::setUnreleased(const QDate& date)
{
    clearItemsIfAny();
    clearErrorIfAny();
    clearEmptyExplanationIfAny();
    if (m_releaseDate != date) {
        m_releaseDate = date;
        Q_EMIT releaseDateChanged();
    }
    resetState(State::Unreleased);
}

const domain::Stream* StreamsListModel::at(int row) const
{
    if (row < 0 || row >= m_items.size()) {
        return nullptr;
    }
    return &m_items.at(row);
}

bool StreamsListModel::hydrateSize(const QString& infoHash, int fileIndex, qint64 sizeBytes)
{
    if (sizeBytes <= 0 || infoHash.isEmpty()) {
        return false;
    }
    // Linear scan: stream lists are bounded by Torrentio's row cap
    // (a few hundred at the absolute worst, typically < 50). Match
    // on `(infoHash, fileIndex)` when the caller pinned a file,
    // otherwise fall back to first hit by infoHash.
    for (int row = 0; row < m_items.size(); ++row) {
        auto& s = m_items[row];
        if (s.infoHash.compare(infoHash, Qt::CaseInsensitive) != 0) {
            continue;
        }
        if (fileIndex >= 0 && s.fileIndex >= 0 && s.fileIndex != fileIndex) {
            continue;
        }
        if (s.sizeBytes.has_value() && *s.sizeBytes == sizeBytes) {
            // Idempotent: nothing to change.
            return true;
        }
        s.sizeBytes = sizeBytes;
        const auto idx = index(row);
        Q_EMIT dataChanged(idx, idx, {SizeBytesRole, SizeTextRole});
        return true;
    }
    return false;
}

} // namespace kinema::ui::qml
