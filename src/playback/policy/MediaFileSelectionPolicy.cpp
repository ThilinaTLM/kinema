// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/MediaFileSelectionPolicy.h"

#include <KLocalizedString>

#include <QFileInfo>
#include <QHash>
#include <QRegularExpression>
#include <QSet>
#include <QStringList>

#include <algorithm>

namespace kinema::playback::policy {

namespace {

constexpr qint64 kTinyVideoBytes = 50LL * 1024LL * 1024LL;

QString normalized(const QString& s)
{
    QString out = s.toLower();
    out.replace(QRegularExpression(QStringLiteral("[^a-z0-9]+")),
        QStringLiteral(" "));
    return out.simplified();
}

QString episodeToken(int season, int episode)
{
    return QStringLiteral("s%1e%2")
        .arg(season, 2, 10, QLatin1Char('0'))
        .arg(episode, 2, 10, QLatin1Char('0'));
}

QString episodeTokenLoose(int season, int episode)
{
    return QStringLiteral("%1x%2").arg(season).arg(episode, 2, 10,
        QLatin1Char('0'));
}

bool matchesEpisode(const QString& path, int season, int episode)
{
    const QString n = normalized(path);
    const QString sxey = episodeToken(season, episode);
    const QString sxeLoose = QStringLiteral("s%1 e%2")
        .arg(season, 2, 10, QLatin1Char('0'))
        .arg(episode, 2, 10, QLatin1Char('0'));
    const QString xToken = episodeTokenLoose(season, episode);
    const QString bare = QStringLiteral("%1 %2")
        .arg(season).arg(episode, 2, 10, QLatin1Char('0'));
    QString compact = n;
    compact.remove(QLatin1Char(' '));
    return compact.contains(sxey)
        || n.contains(sxeLoose)
        || n.contains(xToken)
        || n.contains(bare);
}

struct EpisodeIdentity {
    int season = 0;
    int episode = 0;
};

std::optional<EpisodeIdentity> parseEpisodeIdentity(const QString& path)
{
    QString compact = normalized(path);
    compact.remove(QLatin1Char(' '));

    static const QRegularExpression kSxe(
        QStringLiteral("s(\\d{1,2})e(\\d{1,2})"));
    auto m = kSxe.match(compact);
    if (m.hasMatch()) {
        return EpisodeIdentity { m.captured(1).toInt(),
            m.captured(2).toInt() };
    }

    static const QRegularExpression kXToken(
        QStringLiteral("(\\d{1,2})x(\\d{1,2})"));
    m = kXToken.match(compact);
    if (m.hasMatch()) {
        return EpisodeIdentity { m.captured(1).toInt(),
            m.captured(2).toInt() };
    }

    return std::nullopt;
}

QVector<domain::MediaFileEntry> playableCandidates(
    const QVector<domain::MediaFileEntry>& files)
{
    QVector<domain::MediaFileEntry> candidates;
    for (const auto& f : files) {
        if (f.index < 0 || f.size <= 0 || !isVideoFilePath(f.path)
            || isLikelySampleOrExtra(f.path, f.size)) {
            continue;
        }
        candidates.append(f);
    }
    return candidates;
}

} // namespace

bool isVideoFilePath(const QString& path)
{
    const auto suffix = QFileInfo(path).suffix().toLower();
    static const QSet<QString> kVideoSuffixes {
        QStringLiteral("mkv"), QStringLiteral("mp4"),
        QStringLiteral("m4v"), QStringLiteral("avi"),
        QStringLiteral("mov"), QStringLiteral("webm"),
        QStringLiteral("ts")
    };
    return kVideoSuffixes.contains(suffix);
}

bool isLikelySampleOrExtra(const QString& path, qint64 sizeBytes)
{
    const QString n = normalized(path);
    if (sizeBytes > 0 && sizeBytes < kTinyVideoBytes) {
        return true;
    }
    static const QStringList kBadTokens {
        QStringLiteral("sample"), QStringLiteral("trailer"),
        QStringLiteral("extras"), QStringLiteral("extra"),
        QStringLiteral("featurette"), QStringLiteral("proof")
    };
    for (const auto& token : kBadTokens) {
        if (n.contains(token)) {
            return true;
        }
    }
    return false;
}

MediaFileSelection selectMediaFile(
    const QVector<domain::MediaFileEntry>& files,
    const domain::PlaybackContext& ctx)
{
    QVector<domain::MediaFileEntry> candidates = playableCandidates(files);

    if (candidates.isEmpty()) {
        return { std::nullopt,
            i18nc("@info:status",
                "No playable video file was found in this source.") };
    }

    std::sort(candidates.begin(), candidates.end(),
        [](const auto& a, const auto& b) { return a.size > b.size; });

    if (ctx.key.kind == domain::MediaKind::Series
        && ctx.key.season && ctx.key.episode) {
        QVector<domain::MediaFileEntry> episodeMatches;
        for (const auto& f : candidates) {
            if (matchesEpisode(f.path, *ctx.key.season, *ctx.key.episode)) {
                episodeMatches.append(f);
            }
        }
        if (episodeMatches.size() == 1) {
            return { episodeMatches.first(), {} };
        }
        if (episodeMatches.size() > 1) {
            std::sort(episodeMatches.begin(), episodeMatches.end(),
                [](const auto& a, const auto& b) { return a.size > b.size; });
            return { episodeMatches.first(), {} };
        }

        if (candidates.size() == 1) {
            return { candidates.first(), {} };
        }
        return { std::nullopt,
            i18nc("@info:status",
                "Could not identify the matching episode file in this source.") };
    }

    return { candidates.first(), {} };
}

std::optional<domain::EpisodeAdjacency> adjacentEpisodeFiles(
    const QVector<domain::MediaFileEntry>& files,
    int season,
    int episode)
{
    if (season <= 0 || episode <= 0) {
        return std::nullopt;
    }

    QHash<int, domain::EpisodeFileTarget> uniqueByEpisode;
    QSet<int> ambiguousEpisodes;

    const auto candidates = playableCandidates(files);
    for (const auto& f : candidates) {
        const auto id = parseEpisodeIdentity(f.path);
        if (!id || id->season != season || id->episode <= 0) {
            continue;
        }

        if (ambiguousEpisodes.contains(id->episode)) {
            continue;
        }
        if (uniqueByEpisode.contains(id->episode)) {
            uniqueByEpisode.remove(id->episode);
            ambiguousEpisodes.insert(id->episode);
            continue;
        }

        uniqueByEpisode.insert(id->episode,
            domain::EpisodeFileTarget { id->season, id->episode, f });
    }

    if (ambiguousEpisodes.contains(episode)
        || !uniqueByEpisode.contains(episode)) {
        return std::nullopt;
    }

    domain::EpisodeAdjacency nav;
    nav.current = uniqueByEpisode.value(episode);
    if (!ambiguousEpisodes.contains(episode - 1)
        && uniqueByEpisode.contains(episode - 1)) {
        nav.previous = uniqueByEpisode.value(episode - 1);
    }
    if (!ambiguousEpisodes.contains(episode + 1)
        && uniqueByEpisode.contains(episode + 1)) {
        nav.next = uniqueByEpisode.value(episode + 1);
    }
    return nav;
}

int playableCandidateCount(const QVector<domain::MediaFileEntry>& files)
{
    return playableCandidates(files).size();
}

} // namespace kinema::playback::policy
