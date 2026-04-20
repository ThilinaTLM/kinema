// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "config/Config.h"

#include <KConfigGroup>
#include <KSharedConfig>

namespace kinema::config {

namespace {

constexpr auto kGroupGeneral = "General";
constexpr auto kGroupRD = "RealDebrid";

constexpr auto kKeyCachedOnly = "cachedOnly";
constexpr auto kKeySearchKind = "searchKind";
constexpr auto kKeyFocusSplitter = "focusSplitter";
constexpr auto kKeyRDConfigured = "configured";

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

QByteArray Config::focusSplitterState() const
{
    return group(kGroupGeneral).readEntry(kKeyFocusSplitter, QByteArray {});
}

void Config::setFocusSplitterState(QByteArray state)
{
    auto g = group(kGroupGeneral);
    g.writeEntry(kKeyFocusSplitter, state);
    g.sync();
}

core::torrentio::ConfigOptions Config::defaultTorrentioOptions() const
{
    core::torrentio::ConfigOptions opts;
    opts.sort = core::torrentio::SortMode::Seeders;
    return opts;
}

} // namespace kinema::config
