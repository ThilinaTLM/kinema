// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/SubtitleSettings.h"

#include <KConfigGroup>

namespace kinema::config {

namespace {
constexpr auto kGroup = "Subtitles";
constexpr auto kPreferredLangs = "preferredLanguages";
constexpr auto kHearingImpaired = "hearingImpaired";
constexpr auto kForeignPartsOnly = "foreignPartsOnly";
constexpr auto kDefaultMode = "off";

QString clampMode(const QString& s)
{
    if (s == QLatin1String("include") || s == QLatin1String("only")
        || s == QLatin1String("off")) {
        return s;
    }
    return QString::fromLatin1(kDefaultMode);
}
} // namespace

SubtitleSettings::SubtitleSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

QStringList SubtitleSettings::preferredLanguages() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kPreferredLangs, QStringList {});
}

void SubtitleSettings::setPreferredLanguages(const QStringList& langs)
{
    if (preferredLanguages() == langs) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kPreferredLangs, langs);
    g.sync();
    Q_EMIT preferredLanguagesChanged(langs);
}

QString SubtitleSettings::hearingImpaired() const
{
    return clampMode(m_config->group(QString::fromLatin1(kGroup))
                         .readEntry(kHearingImpaired,
                             QString::fromLatin1(kDefaultMode)));
}

void SubtitleSettings::setHearingImpaired(const QString& mode)
{
    const auto m = clampMode(mode);
    if (hearingImpaired() == m) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kHearingImpaired, m);
    g.sync();
    Q_EMIT hearingImpairedChanged(m);
}

QString SubtitleSettings::foreignPartsOnly() const
{
    return clampMode(m_config->group(QString::fromLatin1(kGroup))
                         .readEntry(kForeignPartsOnly,
                             QString::fromLatin1(kDefaultMode)));
}

void SubtitleSettings::setForeignPartsOnly(const QString& mode)
{
    const auto m = clampMode(mode);
    if (foreignPartsOnly() == m) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kForeignPartsOnly, m);
    g.sync();
    Q_EMIT foreignPartsOnlyChanged(m);
}

} // namespace kinema::config
