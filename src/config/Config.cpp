// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "config/Config.h"

#include <KConfigGroup>
#include <KSharedConfig>

namespace kinema::config {

namespace {

constexpr auto kGroupGeneral = "General";
constexpr auto kGroupRD = "RealDebrid";
constexpr auto kGroupPlayer = "Player";

constexpr auto kKeyCachedOnly = "cachedOnly";
constexpr auto kKeySearchKind = "searchKind";
constexpr auto kKeyFocusSplitter = "focusSplitter";
constexpr auto kKeyRDConfigured = "configured";
constexpr auto kKeyPlayerPreferred = "preferred";
constexpr auto kKeyPlayerCustomCmd = "customCommand";

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
    opts.sort = core::torrentio::SortMode::Seeders;
    return opts;
}

} // namespace kinema::config
