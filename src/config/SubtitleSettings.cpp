// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/SubtitleSettings.h"

#include "config/ConfigAccess.h"

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
    return detail::read(m_config, kGroup, kPreferredLangs, QStringList {});
}

void SubtitleSettings::setPreferredLanguages(const QStringList& langs)
{
    if (preferredLanguages() == langs) {
        return;
    }
    detail::write(m_config, kGroup, kPreferredLangs, langs);
    Q_EMIT preferredLanguagesChanged(langs);
}

QString SubtitleSettings::hearingImpaired() const
{
    return clampMode(detail::read(m_config, kGroup, kHearingImpaired,
        QString::fromLatin1(kDefaultMode)));
}

void SubtitleSettings::setHearingImpaired(const QString& mode)
{
    const auto m = clampMode(mode);
    if (hearingImpaired() == m) {
        return;
    }
    detail::write(m_config, kGroup, kHearingImpaired, m);
    Q_EMIT hearingImpairedChanged(m);
}

QString SubtitleSettings::foreignPartsOnly() const
{
    return clampMode(detail::read(m_config, kGroup, kForeignPartsOnly,
        QString::fromLatin1(kDefaultMode)));
}

void SubtitleSettings::setForeignPartsOnly(const QString& mode)
{
    const auto m = clampMode(mode);
    if (foreignPartsOnly() == m) {
        return;
    }
    detail::write(m_config, kGroup, kForeignPartsOnly, m);
    Q_EMIT foreignPartsOnlyChanged(m);
}

} // namespace kinema::config
