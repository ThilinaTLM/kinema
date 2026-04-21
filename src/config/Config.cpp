// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "config/Config.h"

#include <KConfigGroup>
#include <KSharedConfig>

namespace kinema::config {

namespace {

constexpr auto kGroupGeneral = "General";
constexpr auto kGroupFilters = "Filters";
constexpr auto kGroupRD = "RealDebrid";
constexpr auto kGroupPlayer = "Player";
constexpr auto kGroupBrowse = "Browse";

constexpr auto kKeyCachedOnly = "cachedOnly";
constexpr auto kKeyCloseToTray = "closeToTray";
constexpr auto kKeySearchKind = "searchKind";
constexpr auto kKeyDefaultSort = "defaultSort";
constexpr auto kKeyBrowseSplitter = "browseSplitter";
constexpr auto kKeySeriesPaneSplitter = "seriesPaneSplitter";
constexpr auto kKeyDetailSplitter = "detailSplitter";
constexpr auto kKeyExcludedResolutions = "excludedResolutions";
constexpr auto kKeyExcludedCategories = "excludedCategories";
constexpr auto kKeyKeywordBlocklist = "keywordBlocklist";
constexpr auto kKeyRDConfigured = "configured";
constexpr auto kKeyPlayerPreferred = "preferred";
constexpr auto kKeyPlayerCustomCmd = "customCommand";
constexpr auto kKeyBrowseKind = "kind";
constexpr auto kKeyBrowseGenres = "genreIds";
constexpr auto kKeyBrowseDateWindow = "dateWindow";
constexpr auto kKeyBrowseMinRatingPct = "minRatingPct";
constexpr auto kKeyBrowseSort = "sort";
constexpr auto kKeyBrowseHideObscure = "hideObscure";

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

QStringList normalize(QStringList list)
{
    QStringList out;
    out.reserve(list.size());
    for (auto& s : list) {
        s = s.trimmed();
        if (!s.isEmpty()) {
            out.append(std::move(s));
        }
    }
    return out;
}

KConfigGroup group(const char* name)
{
    return KSharedConfig::openConfig()->group(QString::fromLatin1(name));
}

} // namespace

Config& Config::instance()
{
    static Config s;
    return s;
}

Config::Config() = default;

bool Config::hasRealDebrid() const
{
    return group(kGroupRD).readEntry(kKeyRDConfigured, false);
}

void Config::setHasRealDebrid(bool on)
{
    if (hasRealDebrid() == on) {
        return;
    }
    auto g = group(kGroupRD);
    g.writeEntry(kKeyRDConfigured, on);
    g.sync();
    Q_EMIT realDebridChanged(on);
}

bool Config::cachedOnly() const
{
    return group(kGroupGeneral).readEntry(kKeyCachedOnly, false);
}

bool Config::closeToTray() const
{
    return group(kGroupGeneral).readEntry(kKeyCloseToTray, true);
}

void Config::setCloseToTray(bool on)
{
    if (closeToTray() == on) {
        return;
    }
    auto g = group(kGroupGeneral);
    g.writeEntry(kKeyCloseToTray, on);
    g.sync();
}

void Config::setCachedOnly(bool on)
{
    if (cachedOnly() == on) {
        return;
    }
    auto g = group(kGroupGeneral);
    g.writeEntry(kKeyCachedOnly, on);
    g.sync();
    Q_EMIT cachedOnlyChanged(on);
}

api::MediaKind Config::searchKind() const
{
    const auto s = group(kGroupGeneral).readEntry(kKeySearchKind, QStringLiteral("Movie"));
    return s == QLatin1String("Series") ? api::MediaKind::Series : api::MediaKind::Movie;
}

void Config::setSearchKind(api::MediaKind k)
{
    auto g = group(kGroupGeneral);
    g.writeEntry(kKeySearchKind,
        k == api::MediaKind::Series ? QStringLiteral("Series") : QStringLiteral("Movie"));
    g.sync();
}

core::torrentio::SortMode Config::defaultSort() const
{
    const auto s = group(kGroupGeneral).readEntry(kKeyDefaultSort, QStringLiteral("seeders"));
    if (s == QLatin1String("size")) {
        return core::torrentio::SortMode::Size;
    }
    if (s == QLatin1String("qualitysize")) {
        return core::torrentio::SortMode::QualitySize;
    }
    return core::torrentio::SortMode::Seeders;
}

void Config::setDefaultSort(core::torrentio::SortMode m)
{
    if (defaultSort() == m) {
        return;
    }
    auto g = group(kGroupGeneral);
    g.writeEntry(kKeyDefaultSort, core::torrentio::toString(m));
    g.sync();
    Q_EMIT torrentioOptionsChanged();
}

QStringList Config::excludedResolutions() const
{
    return group(kGroupFilters).readEntry(kKeyExcludedResolutions, QStringList {});
}

void Config::setExcludedResolutions(QStringList list)
{
    list = normalize(std::move(list));
    if (excludedResolutions() == list) {
        return;
    }
    auto g = group(kGroupFilters);
    g.writeEntry(kKeyExcludedResolutions, list);
    g.sync();
    Q_EMIT torrentioOptionsChanged();
}

QStringList Config::excludedCategories() const
{
    return group(kGroupFilters).readEntry(kKeyExcludedCategories, QStringList {});
}

void Config::setExcludedCategories(QStringList list)
{
    list = normalize(std::move(list));
    if (excludedCategories() == list) {
        return;
    }
    auto g = group(kGroupFilters);
    g.writeEntry(kKeyExcludedCategories, list);
    g.sync();
    Q_EMIT torrentioOptionsChanged();
}

QStringList Config::keywordBlocklist() const
{
    return group(kGroupFilters).readEntry(kKeyKeywordBlocklist, QStringList {});
}

void Config::setKeywordBlocklist(QStringList list)
{
    list = normalize(std::move(list));
    if (keywordBlocklist() == list) {
        return;
    }
    auto g = group(kGroupFilters);
    g.writeEntry(kKeyKeywordBlocklist, list);
    g.sync();
    Q_EMIT keywordBlocklistChanged(list);
}

QByteArray Config::browseSplitterState() const
{
    return group(kGroupGeneral).readEntry(kKeyBrowseSplitter, QByteArray {});
}

void Config::setBrowseSplitterState(QByteArray state)
{
    auto g = group(kGroupGeneral);
    g.writeEntry(kKeyBrowseSplitter, state);
    g.sync();
}

QByteArray Config::seriesPaneSplitterState() const
{
    return group(kGroupGeneral).readEntry(kKeySeriesPaneSplitter, QByteArray {});
}

void Config::setSeriesPaneSplitterState(QByteArray state)
{
    auto g = group(kGroupGeneral);
    g.writeEntry(kKeySeriesPaneSplitter, state);
    g.sync();
}

QByteArray Config::detailSplitterState() const
{
    return group(kGroupGeneral).readEntry(kKeyDetailSplitter, QByteArray {});
}

void Config::setDetailSplitterState(QByteArray state)
{
    auto g = group(kGroupGeneral);
    g.writeEntry(kKeyDetailSplitter, state);
    g.sync();
}

core::player::Kind Config::preferredPlayer() const
{
    const auto s = group(kGroupPlayer).readEntry(kKeyPlayerPreferred, QStringLiteral("mpv"));
    return core::player::fromString(s).value_or(core::player::Kind::Mpv);
}

void Config::setPreferredPlayer(core::player::Kind k)
{
    if (preferredPlayer() == k) {
        return;
    }
    auto g = group(kGroupPlayer);
    g.writeEntry(kKeyPlayerPreferred, core::player::toString(k));
    g.sync();
    Q_EMIT preferredPlayerChanged(k);
}

QString Config::customPlayerCommand() const
{
    return group(kGroupPlayer).readEntry(kKeyPlayerCustomCmd, QString {});
}

void Config::setCustomPlayerCommand(const QString& command)
{
    auto g = group(kGroupPlayer);
    g.writeEntry(kKeyPlayerCustomCmd, command);
    g.sync();
}

// ---- Browse state -------------------------------------------------------

api::MediaKind Config::browseKind() const
{
    const auto s = group(kGroupBrowse).readEntry(
        kKeyBrowseKind, QStringLiteral("Movie"));
    return s == QLatin1String("Series")
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
}

void Config::setBrowseKind(api::MediaKind k)
{
    auto g = group(kGroupBrowse);
    g.writeEntry(kKeyBrowseKind,
        k == api::MediaKind::Series
            ? QStringLiteral("Series")
            : QStringLiteral("Movie"));
    g.sync();
}

QList<int> Config::browseGenreIds() const
{
    const auto raw = group(kGroupBrowse).readEntry(
        kKeyBrowseGenres, QString {});
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

void Config::setBrowseGenreIds(QList<int> ids)
{
    QStringList parts;
    parts.reserve(ids.size());
    for (int id : ids) {
        if (id > 0) {
            parts.append(QString::number(id));
        }
    }
    auto g = group(kGroupBrowse);
    g.writeEntry(kKeyBrowseGenres, parts.join(QLatin1Char(',')));
    g.sync();
}

core::DateWindow Config::browseDateWindow() const
{
    const auto s = group(kGroupBrowse).readEntry(
        kKeyBrowseDateWindow, QStringLiteral("year"));
    return core::dateWindowFromString(s, core::DateWindow::ThisYear);
}

void Config::setBrowseDateWindow(core::DateWindow w)
{
    auto g = group(kGroupBrowse);
    g.writeEntry(kKeyBrowseDateWindow, core::dateWindowToString(w));
    g.sync();
}

int Config::browseMinRatingPct() const
{
    const int raw = group(kGroupBrowse).readEntry(kKeyBrowseMinRatingPct, 0);
    // Clamp to sane bounds — if someone hand-edits the config we don't
    // want to feed a garbage vote_average.gte to TMDB.
    if (raw < 0) {
        return 0;
    }
    if (raw > 100) {
        return 100;
    }
    return raw;
}

void Config::setBrowseMinRatingPct(int pct)
{
    auto g = group(kGroupBrowse);
    g.writeEntry(kKeyBrowseMinRatingPct, pct);
    g.sync();
}

api::DiscoverSort Config::browseSort() const
{
    const auto s = group(kGroupBrowse).readEntry(
        kKeyBrowseSort, QStringLiteral("popularity"));
    return discoverSortFromToken(s);
}

void Config::setBrowseSort(api::DiscoverSort s)
{
    auto g = group(kGroupBrowse);
    g.writeEntry(kKeyBrowseSort, discoverSortToToken(s));
    g.sync();
}

bool Config::browseHideObscure() const
{
    return group(kGroupBrowse).readEntry(kKeyBrowseHideObscure, true);
}

void Config::setBrowseHideObscure(bool on)
{
    auto g = group(kGroupBrowse);
    g.writeEntry(kKeyBrowseHideObscure, on);
    g.sync();
}

core::torrentio::ConfigOptions Config::defaultTorrentioOptions() const
{
    core::torrentio::ConfigOptions opts;
    opts.sort = defaultSort();
    opts.excludedResolutions = excludedResolutions();
    opts.excludedCategories = excludedCategories();
    return opts;
}

} // namespace kinema::config
