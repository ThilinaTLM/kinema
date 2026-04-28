// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/BrowseSettings.h"

#include "config/ConfigAccess.h"

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
    const auto s = detail::read(m_config, kGroup, kKeyKind,
        QStringLiteral("Movie"));
    return s == QLatin1String("Series")
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
}

void BrowseSettings::setKind(api::MediaKind k)
{
    detail::write(m_config, kGroup, kKeyKind,
        k == api::MediaKind::Series
            ? QStringLiteral("Series")
            : QStringLiteral("Movie"));
}

QList<int> BrowseSettings::genreIds() const
{
    const auto raw = detail::read(m_config, kGroup, kKeyGenres, QString {});
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
    detail::write(m_config, kGroup, kKeyGenres, parts.join(QLatin1Char(',')));
}

core::DateWindow BrowseSettings::dateWindow() const
{
    const auto s = detail::read(m_config, kGroup, kKeyDateWindow,
        QStringLiteral("year"));
    return core::dateWindowFromString(s, core::DateWindow::ThisYear);
}

void BrowseSettings::setDateWindow(core::DateWindow w)
{
    detail::write(m_config, kGroup, kKeyDateWindow,
        core::dateWindowToString(w));
}

int BrowseSettings::minRatingPct() const
{
    const int raw = detail::read(m_config, kGroup, kKeyMinRatingPct, 0);
    return std::clamp(raw, 0, 100);
}

void BrowseSettings::setMinRatingPct(int pct)
{
    detail::write(m_config, kGroup, kKeyMinRatingPct, pct);
}

api::DiscoverSort BrowseSettings::sort() const
{
    const auto s = detail::read(m_config, kGroup, kKeySort,
        QStringLiteral("popularity"));
    return discoverSortFromToken(s);
}

void BrowseSettings::setSort(api::DiscoverSort s)
{
    detail::write(m_config, kGroup, kKeySort, discoverSortToToken(s));
}

bool BrowseSettings::hideObscure() const
{
    return detail::read(m_config, kGroup, kKeyHideObscure, true);
}

void BrowseSettings::setHideObscure(bool on)
{
    detail::write(m_config, kGroup, kKeyHideObscure, on);
}

} // namespace kinema::config
