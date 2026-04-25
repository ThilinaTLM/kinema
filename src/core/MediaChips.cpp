// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MediaChips.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QLatin1String>
#include <QString>
#include <QStringList>

namespace kinema::core::media_chips {

namespace {

QString resolutionLabel(int height)
{
    if (height <= 0) {
        return {};
    }
    if (height >= 2000) {
        return QStringLiteral("4K");
    }
    // Normal tiers snap to conventional labels; fall back to raw
    // "<n>p" for anything off the ladder. We treat a small
    // tolerance around the canonical heights so 1088-high content
    // (common EAC encoding rounding) still reads as 1080p.
    static constexpr struct {
        int canonical;
        const char* label;
    } tiers[] = {
        { 1440, "1440p" },
        { 1080, "1080p" },
        {  720,  "720p" },
        {  480,  "480p" },
        {  360,  "360p" },
    };
    for (const auto& t : tiers) {
        if (height >= t.canonical - 8 && height <= t.canonical + 16) {
            return QString::fromLatin1(t.label);
        }
    }
    return QStringLiteral("%1p").arg(height);
}

QString hdrLabel(const QString& primaries, const QString& gamma)
{
    const QString p = primaries.trimmed().toLower();
    const QString g = gamma.trimmed().toLower();

    // Dolby Vision is signalled by mpv via either an explicit
    // `dolbyvision` token in the gamma string, or by a DV-specific
    // primaries name. Match loosely.
    if (g.contains(QLatin1String("dolby"))
        || p.contains(QLatin1String("dolby"))) {
        return QStringLiteral("DV");
    }
    if (p == QLatin1String("bt.2020") || p == QLatin1String("bt2020")) {
        if (g == QLatin1String("pq") || g == QLatin1String("st2084")) {
            return QStringLiteral("HDR10");
        }
        if (g == QLatin1String("hlg") || g == QLatin1String("arib-std-b67")) {
            return QStringLiteral("HLG");
        }
    }
    return {};
}

QString videoCodecLabel(const QString& codec)
{
    const QString c = codec.trimmed().toLower();
    if (c.isEmpty()) {
        return {};
    }
    if (c == QLatin1String("hevc") || c == QLatin1String("h265")
        || c == QLatin1String("h.265")) {
        return QStringLiteral("H.265");
    }
    if (c == QLatin1String("h264") || c == QLatin1String("avc")
        || c == QLatin1String("avc1") || c == QLatin1String("h.264")) {
        return QStringLiteral("H.264");
    }
    if (c == QLatin1String("av1")) {
        return QStringLiteral("AV1");
    }
    if (c == QLatin1String("vp9")) {
        return QStringLiteral("VP9");
    }
    if (c == QLatin1String("vp8")) {
        return QStringLiteral("VP8");
    }
    if (c == QLatin1String("mpeg2video")
        || c == QLatin1String("mpeg2")) {
        return QStringLiteral("MPEG-2");
    }
    // Fallback: uppercase the raw codec short name so at least the
    // chip reads like a format tag rather than an internal id.
    return codec.trimmed().toUpper();
}

QString channelLayout(int ch)
{
    switch (ch) {
    case 1: return QStringLiteral("1.0");
    case 2: return QStringLiteral("2.0");
    case 3: return QStringLiteral("2.1");
    case 4: return QStringLiteral("4.0");
    case 5: return QStringLiteral("4.1");
    case 6: return QStringLiteral("5.1");
    case 7: return QStringLiteral("6.1");
    case 8: return QStringLiteral("7.1");
    default:
        if (ch > 8) {
            return QStringLiteral("%1ch").arg(ch);
        }
        return {};
    }
}

QString audioCodecShort(const QString& codec)
{
    const QString c = codec.trimmed().toLower();
    if (c.isEmpty()) {
        return {};
    }
    // Preserve recognisable names; collapse long internal ids to a
    // short marketing-style tag.
    if (c == QLatin1String("aac")
        || c == QLatin1String("ac3") || c == QLatin1String("ac-3")
        || c == QLatin1String("eac3") || c == QLatin1String("e-ac-3")
        || c == QLatin1String("dts") || c == QLatin1String("dts-hd")
        || c == QLatin1String("truehd") || c == QLatin1String("opus")
        || c == QLatin1String("flac") || c == QLatin1String("mp3")
        || c == QLatin1String("vorbis")) {
        if (c == QLatin1String("e-ac-3")) return QStringLiteral("EAC3");
        if (c == QLatin1String("ac-3"))   return QStringLiteral("AC3");
        return codec.trimmed().toUpper();
    }
    return codec.trimmed().toUpper();
}

QString audioLabel(const QString& codec, int ch)
{
    const QString name = audioCodecShort(codec);
    const QString layout = channelLayout(ch);
    if (name.isEmpty() && layout.isEmpty()) {
        return {};
    }
    if (name.isEmpty()) {
        return layout;
    }
    if (layout.isEmpty()) {
        return name;
    }
    return QStringLiteral("%1 %2").arg(name, layout);
}

} // namespace

QByteArray toIpcJson(const ChipInputs& in)
{
    QStringList chips;
    chips.reserve(4);

    const QString res = resolutionLabel(in.videoHeight);
    if (!res.isEmpty()) {
        chips.append(res);
    }
    const QString hdr = hdrLabel(in.hdrPrimaries, in.hdrGamma);
    if (!hdr.isEmpty()) {
        chips.append(hdr);
    }
    const QString vc = videoCodecLabel(in.videoCodec);
    if (!vc.isEmpty()) {
        chips.append(vc);
    }
    const QString audio = audioLabel(in.audioCodec, in.audioChannels);
    if (!audio.isEmpty()) {
        chips.append(audio);
    }

    QJsonArray arr;
    for (const auto& c : chips) {
        arr.append(c);
    }
    return QJsonDocument(arr).toJson(QJsonDocument::Compact);
}

} // namespace kinema::core::media_chips
