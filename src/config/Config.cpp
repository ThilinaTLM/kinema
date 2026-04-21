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

constexpr auto kKeyCachedOnly = "cachedOnly";
constexpr auto kKeySearchKind = "searchKind";
constexpr auto kKeyDefaultSort = "defaultSort";
constexpr auto kKeyBrowseSplitter = "browseSplitter";
constexpr auto kKeySeriesPaneSplitter = "seriesPaneSplitter";
constexpr auto kKeyExcludedResolutions = "excludedResolutions";
constexpr auto kKeyExcludedCategories = "excludedCategories";
constexpr auto kKeyKeywordBlocklist = "keywordBlocklist";
constexpr auto kKeyRDConfigured = "configured";
constexpr auto kKeyPlayerPreferred = "preferred";
constexpr auto kKeyPlayerCustomCmd = "customCommand";

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

core::torrentio::ConfigOptions Config::defaultTorrentioOptions() const
{
    core::torrentio::ConfigOptions opts;
    opts.sort = defaultSort();
    opts.excludedResolutions = excludedResolutions();
    opts.excludedCategories = excludedCategories();
    return opts;
}

} // namespace kinema::config
