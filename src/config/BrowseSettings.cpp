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

QString discoverSortToToken(domain::DiscoverSort s)
{
    switch (s) {
    case domain::DiscoverSort::Popularity:
        return QStringLiteral("popularity");
    case domain::DiscoverSort::ReleaseDate:
        return QStringLiteral("releaseDate");
    case domain::DiscoverSort::Rating:
        return QStringLiteral("rating");
    case domain::DiscoverSort::TitleAsc:
        return QStringLiteral("title");
    }
    return QStringLiteral("popularity");
}

domain::DiscoverSort discoverSortFromToken(const QString& s)
{
    if (s == QLatin1String("releaseDate")) {
        return domain::DiscoverSort::ReleaseDate;
    }
    if (s == QLatin1String("rating")) {
        return domain::DiscoverSort::Rating;
    }
    if (s == QLatin1String("title")) {
        return domain::DiscoverSort::TitleAsc;
    }
    return domain::DiscoverSort::Popularity;
}

} // namespace

BrowseSettings::BrowseSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

domain::MediaKind BrowseSettings::kind() const
{
    const auto s = detail::read(m_config, kGroup, kKeyKind,
        QStringLiteral("Movie"));
    return s == QLatin1String("Series")
        ? domain::MediaKind::Series
        : domain::MediaKind::Movie;
}

void BrowseSettings::setKind(domain::MediaKind k)
{
    detail::write(m_config, kGroup, kKeyKind,
        k == domain::MediaKind::Series
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
        QStringLiteral("year3"));
    return core::dateWindowFromString(s, core::DateWindow::Past3Years);
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

domain::DiscoverSort BrowseSettings::sort() const
{
    const auto s = detail::read(m_config, kGroup, kKeySort,
        QStringLiteral("popularity"));
    return discoverSortFromToken(s);
}

void BrowseSettings::setSort(domain::DiscoverSort s)
{
    detail::write(m_config, kGroup, kKeySort, discoverSortToToken(s));
}

bool BrowseSettings::hideObscure() const
{
    return detail::read(m_config, kGroup, kKeyHideObscure, false);
}

void BrowseSettings::setHideObscure(bool on)
{
    detail::write(m_config, kGroup, kKeyHideObscure, on);
}

} // namespace kinema::config
