// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/BrowseSettings.h"

#include <KConfigGroup>

#include <QStringList>

namespace kinema::config {

namespace {

constexpr auto kGroup = "Browse";
constexpr auto kKeyKind = "kind";
constexpr auto kKeyGenres = "genreIds";
constexpr auto kKeyDateWindow = "dateWindow";
constexpr auto kKeyMinRatingPct = "minRatingPct";
constexpr auto kKeySort = "sort";
constexpr auto kKeyHideObscure = "hideObscure";

QString discoverSortToToken(api::DiscoverSort s)
{
    switch (s) {
    case api::DiscoverSort::Popularity:
        return QStringLiteral("popularity");
    case api::DiscoverSort::ReleaseDate:
        return QStringLiteral("releaseDate");
    case api::DiscoverSort::Rating:
        return QStringLiteral("rating");
    case api::DiscoverSort::TitleAsc:
        return QStringLiteral("title");
    }
    return QStringLiteral("popularity");
}

api::DiscoverSort discoverSortFromToken(const QString& s)
{
    if (s == QLatin1String("releaseDate")) {
        return api::DiscoverSort::ReleaseDate;
    }
    if (s == QLatin1String("rating")) {
        return api::DiscoverSort::Rating;
    }
    if (s == QLatin1String("title")) {
        return api::DiscoverSort::TitleAsc;
    }
    return api::DiscoverSort::Popularity;
}

} // namespace

BrowseSettings::BrowseSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

api::MediaKind BrowseSettings::kind() const
{
    const auto s = m_config->group(QString::fromLatin1(kGroup))
                       .readEntry(kKeyKind, QStringLiteral("Movie"));
    return s == QLatin1String("Series")
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
}

void BrowseSettings::setKind(api::MediaKind k)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyKind,
        k == api::MediaKind::Series
            ? QStringLiteral("Series")
            : QStringLiteral("Movie"));
    g.sync();
}

QList<int> BrowseSettings::genreIds() const
{
    const auto raw = m_config->group(QString::fromLatin1(kGroup))
                         .readEntry(kKeyGenres, QString {});
    if (raw.isEmpty()) {
        return {};
    }
    QList<int> out;
    const auto parts = raw.split(QLatin1Char(','), Qt::SkipEmptyParts);
    out.reserve(parts.size());
    for (const auto& p : parts) {
        bool ok = false;
        const int id = p.trimmed().toInt(&ok);
        if (ok && id > 0) {
            out.append(id);
        }
    }
    return out;
}

void BrowseSettings::setGenreIds(QList<int> ids)
{
    QStringList parts;
    parts.reserve(ids.size());
    for (int id : ids) {
        if (id > 0) {
            parts.append(QString::number(id));
        }
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyGenres, parts.join(QLatin1Char(',')));
    g.sync();
}

core::DateWindow BrowseSettings::dateWindow() const
{
    const auto s = m_config->group(QString::fromLatin1(kGroup))
                       .readEntry(kKeyDateWindow, QStringLiteral("year"));
    return core::dateWindowFromString(s, core::DateWindow::ThisYear);
}

void BrowseSettings::setDateWindow(core::DateWindow w)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyDateWindow, core::dateWindowToString(w));
    g.sync();
}

int BrowseSettings::minRatingPct() const
{
    const int raw = m_config->group(QString::fromLatin1(kGroup))
                        .readEntry(kKeyMinRatingPct, 0);
    if (raw < 0) {
        return 0;
    }
    if (raw > 100) {
        return 100;
    }
    return raw;
}

void BrowseSettings::setMinRatingPct(int pct)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyMinRatingPct, pct);
    g.sync();
}

api::DiscoverSort BrowseSettings::sort() const
{
    const auto s = m_config->group(QString::fromLatin1(kGroup))
                       .readEntry(kKeySort, QStringLiteral("popularity"));
    return discoverSortFromToken(s);
}

void BrowseSettings::setSort(api::DiscoverSort s)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeySort, discoverSortToToken(s));
    g.sync();
}

bool BrowseSettings::hideObscure() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyHideObscure, true);
}

void BrowseSettings::setHideObscure(bool on)
{
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyHideObscure, on);
    g.sync();
}

} // namespace kinema::config
