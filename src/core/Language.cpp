// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Language.h"

#include <QHash>

namespace kinema::core::language {

namespace {

// 25 most common ISO 639-2 codes for subtitles. The full list is much
// longer; unknown codes round-trip through displayName() as the raw
// code so we don't need to ship a complete table to keep the UI sane.
const QHash<QString, QString>& table()
{
    static const QHash<QString, QString> kTable = {
        { QStringLiteral("eng"), QStringLiteral("English") },
        { QStringLiteral("spa"), QStringLiteral("Spanish") },
        { QStringLiteral("fre"), QStringLiteral("French") },
        { QStringLiteral("fra"), QStringLiteral("French") },
        { QStringLiteral("ger"), QStringLiteral("German") },
        { QStringLiteral("deu"), QStringLiteral("German") },
        { QStringLiteral("ita"), QStringLiteral("Italian") },
        { QStringLiteral("por"), QStringLiteral("Portuguese") },
        { QStringLiteral("pob"), QStringLiteral("Portuguese (BR)") },
        { QStringLiteral("rus"), QStringLiteral("Russian") },
        { QStringLiteral("ukr"), QStringLiteral("Ukrainian") },
        { QStringLiteral("pol"), QStringLiteral("Polish") },
        { QStringLiteral("dut"), QStringLiteral("Dutch") },
        { QStringLiteral("nld"), QStringLiteral("Dutch") },
        { QStringLiteral("swe"), QStringLiteral("Swedish") },
        { QStringLiteral("nor"), QStringLiteral("Norwegian") },
        { QStringLiteral("dan"), QStringLiteral("Danish") },
        { QStringLiteral("fin"), QStringLiteral("Finnish") },
        { QStringLiteral("ara"), QStringLiteral("Arabic") },
        { QStringLiteral("heb"), QStringLiteral("Hebrew") },
        { QStringLiteral("tur"), QStringLiteral("Turkish") },
        { QStringLiteral("gre"), QStringLiteral("Greek") },
        { QStringLiteral("ell"), QStringLiteral("Greek") },
        { QStringLiteral("hun"), QStringLiteral("Hungarian") },
        { QStringLiteral("ces"), QStringLiteral("Czech") },
        { QStringLiteral("cze"), QStringLiteral("Czech") },
        { QStringLiteral("rum"), QStringLiteral("Romanian") },
        { QStringLiteral("ron"), QStringLiteral("Romanian") },
        { QStringLiteral("bul"), QStringLiteral("Bulgarian") },
        { QStringLiteral("hin"), QStringLiteral("Hindi") },
        { QStringLiteral("jpn"), QStringLiteral("Japanese") },
        { QStringLiteral("kor"), QStringLiteral("Korean") },
        { QStringLiteral("chi"), QStringLiteral("Chinese") },
        { QStringLiteral("zho"), QStringLiteral("Chinese") },
        { QStringLiteral("vie"), QStringLiteral("Vietnamese") },
        { QStringLiteral("tha"), QStringLiteral("Thai") },
        { QStringLiteral("ind"), QStringLiteral("Indonesian") },
        { QStringLiteral("msa"), QStringLiteral("Malay") },
        { QStringLiteral("may"), QStringLiteral("Malay") },
    };
    return kTable;
}

} // namespace

QString displayName(const QString& iso639_2)
{
    if (iso639_2.isEmpty()) {
        return iso639_2;
    }
    const auto code = iso639_2.toLower();
    const auto& t = table();
    const auto it = t.constFind(code);
    if (it != t.cend()) {
        return *it;
    }
    return iso639_2;
}

QString codeForDisplayName(const QString& englishName)
{
    const auto needle = englishName.trimmed().toLower();
    if (needle.isEmpty()) {
        return {};
    }
    const auto& t = table();
    for (auto it = t.cbegin(); it != t.cend(); ++it) {
        if (it.value().toLower() == needle) {
            return it.key();
        }
    }
    return {};
}

QList<CommonLanguage> commonLanguages()
{
    // Ordered by English display name to match the settings page
    // dropdown. Codes use OpenSubtitles' preferred bibliographic
    // 639-2/B variants where the API differs (fre/ger/dut/cze/rum/chi/may).
    return {
        { QStringLiteral("ara"), QStringLiteral("Arabic") },
        { QStringLiteral("bul"), QStringLiteral("Bulgarian") },
        { QStringLiteral("chi"), QStringLiteral("Chinese") },
        { QStringLiteral("cze"), QStringLiteral("Czech") },
        { QStringLiteral("dan"), QStringLiteral("Danish") },
        { QStringLiteral("dut"), QStringLiteral("Dutch") },
        { QStringLiteral("eng"), QStringLiteral("English") },
        { QStringLiteral("fin"), QStringLiteral("Finnish") },
        { QStringLiteral("fre"), QStringLiteral("French") },
        { QStringLiteral("ger"), QStringLiteral("German") },
        { QStringLiteral("gre"), QStringLiteral("Greek") },
        { QStringLiteral("heb"), QStringLiteral("Hebrew") },
        { QStringLiteral("hin"), QStringLiteral("Hindi") },
        { QStringLiteral("hun"), QStringLiteral("Hungarian") },
        { QStringLiteral("ind"), QStringLiteral("Indonesian") },
        { QStringLiteral("ita"), QStringLiteral("Italian") },
        { QStringLiteral("jpn"), QStringLiteral("Japanese") },
        { QStringLiteral("kor"), QStringLiteral("Korean") },
        { QStringLiteral("nor"), QStringLiteral("Norwegian") },
        { QStringLiteral("pol"), QStringLiteral("Polish") },
        { QStringLiteral("por"), QStringLiteral("Portuguese") },
        { QStringLiteral("pob"), QStringLiteral("Portuguese (BR)") },
        { QStringLiteral("rum"), QStringLiteral("Romanian") },
        { QStringLiteral("rus"), QStringLiteral("Russian") },
        { QStringLiteral("spa"), QStringLiteral("Spanish") },
        { QStringLiteral("swe"), QStringLiteral("Swedish") },
        { QStringLiteral("tha"), QStringLiteral("Thai") },
        { QStringLiteral("tur"), QStringLiteral("Turkish") },
        { QStringLiteral("ukr"), QStringLiteral("Ukrainian") },
        { QStringLiteral("vie"), QStringLiteral("Vietnamese") },
    };
}

} // namespace kinema::core::language
