// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/StreamTokens.h"

#include <KLocalizedString>

#include <QChar>
#include <QRegularExpression>
#include <QSet>

namespace kinema::core::stream_tokens {

namespace {

// All regexes are case-insensitive and run over a concatenation of
// `qualityLabel + releaseName + detailsText` separated by spaces.
// Order matters in `findSource` and `findHdr`: the longer / more
// specific match must be tried before its substring.

QString blob(const api::Stream& s)
{
    QStringList parts;
    if (!s.qualityLabel.isEmpty()) {
        parts << s.qualityLabel;
    }
    if (!s.releaseName.isEmpty()) {
        parts << s.releaseName;
    }
    if (!s.detailsText.isEmpty()) {
        parts << s.detailsText;
    }
    return parts.join(QLatin1Char(' '));
}

bool contains(const QString& haystack, const QString& pattern)
{
    static const auto opts = QRegularExpression::CaseInsensitiveOption;
    QRegularExpression re(pattern, opts);
    return re.match(haystack).hasMatch();
}

Source findSource(const QString& text)
{
    // Specific before general.
    if (contains(text, QStringLiteral("\\bBlu[- ]?Ray[. _-]?Remux\\b"))
        || contains(text, QStringLiteral("\\bBDRemux\\b"))) {
        return Source::BluRayRemux;
    }
    if (contains(text, QStringLiteral("\\bWEB[- ]DL\\b"))) {
        return Source::WebDl;
    }
    if (contains(text, QStringLiteral("\\bWEB[- ]?Rip\\b"))
        || contains(text, QStringLiteral("\\bWEB\\b"))) {
        return Source::WebRip;
    }
    if (contains(text, QStringLiteral("\\bBlu[- ]?Ray\\b"))
        || contains(text, QStringLiteral("\\bBDRip\\b"))
        || contains(text, QStringLiteral("\\bBRRip\\b"))) {
        return Source::BluRay;
    }
    if (contains(text, QStringLiteral("\\bHDTV\\b"))
        || contains(text, QStringLiteral("\\bPDTV\\b"))) {
        return Source::Hdtv;
    }
    if (contains(text, QStringLiteral("\\bDVD(?:Rip|Scr)?\\b"))) {
        return Source::Dvd;
    }
    if (contains(text, QStringLiteral("\\b(?:CAM|HDCAM|TS|HDTS|TC|SCR)\\b"))) {
        return Source::Cam;
    }
    return Source::Unknown;
}

Codec findCodec(const QString& text)
{
    if (contains(text, QStringLiteral("\\b(?:x265|HEVC|H[. ]?265)\\b"))) {
        return Codec::H265;
    }
    if (contains(text, QStringLiteral("\\b(?:x264|AVC|H[. ]?264)\\b"))) {
        return Codec::H264;
    }
    if (contains(text, QStringLiteral("\\bAV1\\b"))) {
        return Codec::AV1;
    }
    if (contains(text, QStringLiteral("\\bVP9\\b"))) {
        return Codec::VP9;
    }
    if (contains(text, QStringLiteral("\\b(?:XviD|DivX)\\b"))) {
        return Codec::Xvid;
    }
    return Codec::Unknown;
}

Hdr findHdr(const QString& text)
{
    // Dolby Vision beats HDR10+ beats HDR10.
    if (contains(text, QStringLiteral("\\b(?:DV|DoVi|Dolby[. ]?Vision)\\b"))) {
        return Hdr::DolbyVision;
    }
    // \b doesn't fire after `+` (non-word), so anchor on the right with
    // a non-word lookahead instead.
    if (contains(text, QStringLiteral("\\bHDR10\\+(?:\\W|$)"))
        || contains(text, QStringLiteral("\\bHDR10Plus\\b"))) {
        return Hdr::Hdr10Plus;
    }
    if (contains(text, QStringLiteral("\\bHDR(?:10)?\\b"))) {
        return Hdr::Hdr10;
    }
    return Hdr::Sdr;
}

bool findTenBit(const QString& text)
{
    return contains(text, QStringLiteral("\\b10[- ]?bit\\b"))
        || contains(text, QStringLiteral("Main\\s*10"));
}

// Audio: order matters again — match the more specific / longer
// label first so e.g. "DTS-HD MA" wins over "DTS".
QStringList findAudio(const QString& text)
{
    struct Pat {
        const char* re;
        const char* label;
    };
    static const Pat patterns[] = {
        // Object-based / immersive first.
        { "\\bAtmos\\b", "Atmos" },
        { "\\bDTS[- ]?X\\b", "DTS:X" },
        // Lossless next.
        { "\\bDTS[- ]?HD(?:[. ]?MA)?\\b", "DTS-HD MA" },
        { "\\bTrueHD\\b", "TrueHD" },
        { "\\bFLAC\\b", "FLAC" },
        // Lossy.
        { "\\bDDP[. ]?(\\d(?:[. ]\\d)?)\\b", "DDP" },
        { "\\bE[- ]?AC[- ]?3\\b", "EAC3" },
        { "\\bDD[. ]?(\\d(?:[. ]\\d)?)\\b", "DD" },
        { "\\bAC[- ]?3\\b", "AC3" },
        // Avoid matching the "DTS" prefix in "DTS-HD" / "DTS-X".
        { "\\bDTS\\b(?![- ]?(?:HD|X))", "DTS" },
        { "\\bAAC(?:[. ]?(\\d(?:[. ]\\d)?))?\\b", "AAC" },
        { "\\bMP3\\b", "MP3" },
        { "\\bOpus\\b", "Opus" },
    };
    QStringList out;
    QSet<QString> seen;
    for (const auto& p : patterns) {
        QRegularExpression re(QString::fromLatin1(p.re),
            QRegularExpression::CaseInsensitiveOption);
        const auto m = re.match(text);
        if (!m.hasMatch()) {
            continue;
        }
        QString label = QString::fromLatin1(p.label);
        // For the channelled-codec families, append the channel
        // count when captured (e.g. "DDP 5.1").
        if (m.lastCapturedIndex() >= 1) {
            const auto channels = m.captured(1).replace(
                QLatin1Char('.'), QLatin1Char(' ')).trimmed()
                .replace(QLatin1Char(' '), QLatin1Char('.'));
            if (!channels.isEmpty()) {
                label = label + QLatin1Char(' ') + channels;
            }
        }
        if (!seen.contains(label)) {
            seen.insert(label);
            out << label;
        }
    }
    return out;
}

// --- language detection ---------------------------------------------

// Map regional-indicator pair → ISO 639-1.
struct FlagMap {
    char a;
    char b;
    const char* iso;
};
// Cover the most common Torrentio language flags.
constexpr FlagMap kFlagMap[] = {
    { 'G', 'B', "en" }, // 🇬🇧
    { 'U', 'S', "en" }, // 🇺🇸
    { 'F', 'R', "fr" },
    { 'E', 'S', "es" },
    { 'I', 'T', "it" },
    { 'D', 'E', "de" },
    { 'P', 'T', "pt" },
    { 'B', 'R', "pt" }, // pt-BR collapsed to pt
    { 'J', 'P', "ja" },
    { 'K', 'R', "ko" },
    { 'C', 'N', "zh" },
    { 'T', 'W', "zh" },
    { 'H', 'K', "zh" },
    { 'R', 'U', "ru" },
    { 'P', 'L', "pl" },
    { 'N', 'L', "nl" },
    { 'S', 'E', "sv" },
    { 'N', 'O', "no" },
    { 'D', 'K', "da" },
    { 'F', 'I', "fi" },
    { 'T', 'R', "tr" },
    { 'A', 'R', "ar" },
    { 'I', 'N', "hi" }, // best-effort: India flag → Hindi
    { 'I', 'L', "he" },
    { 'G', 'R', "el" },
    { 'C', 'Z', "cs" },
    { 'H', 'U', "hu" },
    { 'R', 'O', "ro" },
    { 'U', 'A', "uk" },
    { 'V', 'N', "vi" },
    { 'T', 'H', "th" },
    { 'I', 'D', "id" },
};

void appendUnique(QStringList& out, const QString& code)
{
    if (code.isEmpty()) {
        return;
    }
    if (!out.contains(code)) {
        out << code;
    }
}

void scanFlagEmoji(const QString& text, QStringList& out)
{
    // Regional-indicator code points are U+1F1E6..U+1F1FF (A..Z).
    // QString stores them as surrogate pairs. Iterate code points.
    auto it = text.cbegin();
    const auto end = text.cend();
    while (it != end) {
        const QChar c1 = *it;
        if (!c1.isHighSurrogate() || (it + 1) == end) {
            ++it;
            continue;
        }
        const QChar c2 = *(it + 1);
        if (!c2.isLowSurrogate()) {
            ++it;
            continue;
        }
        const uint cp1 = QChar::surrogateToUcs4(c1, c2);
        if (cp1 < 0x1F1E6 || cp1 > 0x1F1FF || (it + 2) == end) {
            it += 2;
            continue;
        }
        // Look ahead for the second half of the flag.
        const QChar c3 = *(it + 2);
        if (!c3.isHighSurrogate() || (it + 3) == end) {
            it += 2;
            continue;
        }
        const QChar c4 = *(it + 3);
        if (!c4.isLowSurrogate()) {
            it += 2;
            continue;
        }
        const uint cp2 = QChar::surrogateToUcs4(c3, c4);
        if (cp2 < 0x1F1E6 || cp2 > 0x1F1FF) {
            it += 2;
            continue;
        }
        const char a = static_cast<char>('A' + (cp1 - 0x1F1E6));
        const char b = static_cast<char>('A' + (cp2 - 0x1F1E6));
        for (const auto& f : kFlagMap) {
            if (f.a == a && f.b == b) {
                appendUnique(out, QString::fromLatin1(f.iso));
                break;
            }
        }
        it += 4;
    }
}

void scanLanguageHints(const QString& text, QStringList& out, bool& multiAudio)
{
    // Dual / Multi audio.
    if (contains(text, QStringLiteral("\\b(?:Dual|Multi)[. ]?Audio\\b"))
        || contains(text, QStringLiteral("\\bMULTI\\b"))) {
        multiAudio = true;
    }

    // ISO-pair shorthand commonly used by foreign release groups,
    // e.g. "ITA-ENG", "ENG-FRE", "POR-ENG".
    static const QRegularExpression isoTriple(
        QStringLiteral("\\b([A-Z]{3})-([A-Z]{3})(?:-([A-Z]{3}))?\\b"));
    {
        auto it = isoTriple.globalMatch(text);
        while (it.hasNext()) {
            auto m = it.next();
            for (int i = 1; i <= m.lastCapturedIndex(); ++i) {
                const auto code3 = m.captured(i).toUpper();
                // Map common 3-letter → 2-letter.
                static const QHash<QString, QString> map3to1 {
                    { QStringLiteral("ENG"), QStringLiteral("en") },
                    { QStringLiteral("FRE"), QStringLiteral("fr") },
                    { QStringLiteral("FRA"), QStringLiteral("fr") },
                    { QStringLiteral("ITA"), QStringLiteral("it") },
                    { QStringLiteral("SPA"), QStringLiteral("es") },
                    { QStringLiteral("ESP"), QStringLiteral("es") },
                    { QStringLiteral("GER"), QStringLiteral("de") },
                    { QStringLiteral("DEU"), QStringLiteral("de") },
                    { QStringLiteral("POR"), QStringLiteral("pt") },
                    { QStringLiteral("RUS"), QStringLiteral("ru") },
                    { QStringLiteral("JPN"), QStringLiteral("ja") },
                    { QStringLiteral("KOR"), QStringLiteral("ko") },
                    { QStringLiteral("CHI"), QStringLiteral("zh") },
                    { QStringLiteral("ZHO"), QStringLiteral("zh") },
                    { QStringLiteral("DUT"), QStringLiteral("nl") },
                    { QStringLiteral("NLD"), QStringLiteral("nl") },
                    { QStringLiteral("SWE"), QStringLiteral("sv") },
                    { QStringLiteral("NOR"), QStringLiteral("no") },
                    { QStringLiteral("DAN"), QStringLiteral("da") },
                    { QStringLiteral("FIN"), QStringLiteral("fi") },
                    { QStringLiteral("POL"), QStringLiteral("pl") },
                    { QStringLiteral("CZE"), QStringLiteral("cs") },
                    { QStringLiteral("CES"), QStringLiteral("cs") },
                    { QStringLiteral("HUN"), QStringLiteral("hu") },
                    { QStringLiteral("ROM"), QStringLiteral("ro") },
                    { QStringLiteral("RON"), QStringLiteral("ro") },
                    { QStringLiteral("TUR"), QStringLiteral("tr") },
                    { QStringLiteral("ARA"), QStringLiteral("ar") },
                    { QStringLiteral("HEB"), QStringLiteral("he") },
                    { QStringLiteral("HIN"), QStringLiteral("hi") },
                    { QStringLiteral("VIE"), QStringLiteral("vi") },
                    { QStringLiteral("THA"), QStringLiteral("th") },
                    { QStringLiteral("IND"), QStringLiteral("id") },
                    { QStringLiteral("UKR"), QStringLiteral("uk") },
                    { QStringLiteral("ELL"), QStringLiteral("el") },
                    { QStringLiteral("GRE"), QStringLiteral("el") },
                };
                appendUnique(out, map3to1.value(code3));
            }
        }
    }

    // Free-form textual hints sprinkled into release names.
    struct Hint {
        const char* re;
        const char* iso;
    };
    static const Hint hints[] = {
        { "\\bV(?:FF|FQ|FI|OST(?:FR)?)\\b", "fr" },
        { "\\bTRUEFRENCH\\b", "fr" },
        { "\\bFRENCH\\b", "fr" },
        { "\\bCASTELLANO\\b", "es" },
        { "\\bLATINO\\b", "es" },
        { "\\bSPANISH\\b", "es" },
        { "\\bITALIAN\\b", "it" },
        { "\\bGERMAN\\b", "de" },
        { "\\b(?:RUS|RUSSIAN)\\b", "ru" },
        { "\\bJAPANESE\\b", "ja" },
        { "\\bKOREAN\\b", "ko" },
        { "\\bCHINESE\\b", "zh" },
        { "\\bENGLISH\\b", "en" },
    };
    for (const auto& h : hints) {
        QRegularExpression re(QString::fromLatin1(h.re),
            QRegularExpression::CaseInsensitiveOption);
        if (re.match(text).hasMatch()) {
            appendUnique(out, QString::fromLatin1(h.iso));
        }
    }
}

// Common technical tokens that look like release groups but aren't.
// When the trailing dash regex captures one of these, we treat the
// release name as having no parseable group rather than mis-attributing
// (e.g. "WEB-DL" → "DL" is not a release group).
bool looksLikeTechnicalToken(const QString& token)
{
    static const QSet<QString> deny {
        QStringLiteral("DL"), QStringLiteral("RIP"), QStringLiteral("WEB"),
        QStringLiteral("HD"), QStringLiteral("UHD"), QStringLiteral("SD"),
        QStringLiteral("HEVC"), QStringLiteral("AVC"), QStringLiteral("DV"),
        QStringLiteral("HDR"), QStringLiteral("REMUX"), QStringLiteral("BR"),
        QStringLiteral("BD"), QStringLiteral("DVD"), QStringLiteral("CAM"),
        QStringLiteral("TS"), QStringLiteral("TC"),
        QStringLiteral("265"), QStringLiteral("264"),
        QStringLiteral("REPACK"), QStringLiteral("PROPER"),
        QStringLiteral("EXTENDED"), QStringLiteral("INTERNAL"),
        QStringLiteral("LIMITED"), QStringLiteral("UNRATED"),
    };
    return deny.contains(token.toUpper());
}

QString findReleaseGroup(const QString& releaseName)
{
    // Trailing "-GROUP" or "[GROUP]" near the end of the release name.
    static const QRegularExpression bracketRe(
        QStringLiteral("\\[([A-Za-z0-9._]{2,16})\\]\\s*$"));
    auto m = bracketRe.match(releaseName);
    if (m.hasMatch() && !looksLikeTechnicalToken(m.captured(1))) {
        return m.captured(1);
    }
    static const QRegularExpression dashRe(
        QStringLiteral("[-]([A-Za-z0-9]{2,16})(?:\\.[A-Za-z0-9]{1,4})?\\s*$"));
    m = dashRe.match(releaseName);
    if (m.hasMatch() && !looksLikeTechnicalToken(m.captured(1))) {
        return m.captured(1);
    }
    return {};
}

} // namespace

Tokens parse(const api::Stream& s)
{
    Tokens t;
    const QString text = blob(s);
    if (text.isEmpty()) {
        return t;
    }

    t.source = findSource(text);
    t.codec = findCodec(text);
    t.hdr = findHdr(text);
    t.tenBit = findTenBit(text);
    t.audio = findAudio(text);

    scanFlagEmoji(text, t.languages);
    scanLanguageHints(text, t.languages, t.multiAudio);

    t.releaseGroup = findReleaseGroup(s.releaseName);

    return t;
}

QString sourceLabel(Source s)
{
    switch (s) {
    case Source::WebDl:
        return i18nc("@label stream source", "WEB-DL");
    case Source::WebRip:
        return i18nc("@label stream source", "WEBRip");
    case Source::BluRayRemux:
        return i18nc("@label stream source", "BluRay Remux");
    case Source::BluRay:
        return i18nc("@label stream source", "BluRay");
    case Source::Hdtv:
        return i18nc("@label stream source", "HDTV");
    case Source::Dvd:
        return i18nc("@label stream source", "DVD");
    case Source::Cam:
        return i18nc("@label stream source", "CAM");
    case Source::Unknown:
        return {};
    }
    return {};
}

QString codecLabel(Codec c, bool tenBit)
{
    QString base;
    switch (c) {
    case Codec::H265:
        base = i18nc("@label video codec", "x265");
        break;
    case Codec::H264:
        base = i18nc("@label video codec", "x264");
        break;
    case Codec::AV1:
        base = i18nc("@label video codec", "AV1");
        break;
    case Codec::VP9:
        base = i18nc("@label video codec", "VP9");
        break;
    case Codec::Xvid:
        base = i18nc("@label video codec", "XviD");
        break;
    case Codec::Unknown:
        return {};
    }
    if (tenBit) {
        base += QStringLiteral(" ") + i18nc("@label codec bit depth", "10-bit");
    }
    return base;
}

QString hdrLabel(Hdr h)
{
    switch (h) {
    case Hdr::DolbyVision:
        return i18nc("@label HDR profile", "Dolby Vision");
    case Hdr::Hdr10Plus:
        return i18nc("@label HDR profile", "HDR10+");
    case Hdr::Hdr10:
        return i18nc("@label HDR profile", "HDR10");
    case Hdr::Sdr:
        return {};
    }
    return {};
}

} // namespace kinema::core::stream_tokens
