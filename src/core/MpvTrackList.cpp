// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MpvTrackList.h"

#include <KLocalizedString>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QLocale>

namespace kinema::core::tracks {

namespace {

int readInt(const QJsonObject& o, const char* key, int fallback = 0)
{
    const auto v = o.value(QLatin1String(key));
    if (v.isDouble()) {
        return static_cast<int>(v.toDouble());
    }
    if (v.isString()) {
        bool ok = false;
        const int n = v.toString().toInt(&ok);
        return ok ? n : fallback;
    }
    return fallback;
}

bool readBool(const QJsonObject& o, const char* key, bool fallback = false)
{
    const auto v = o.value(QLatin1String(key));
    if (v.isBool()) {
        return v.toBool();
    }
    if (v.isDouble()) {
        return v.toDouble() != 0.0;
    }
    return fallback;
}

QString readString(const QJsonObject& o, const char* key)
{
    const auto v = o.value(QLatin1String(key));
    if (v.isString()) {
        return v.toString();
    }
    return {};
}

QString humanLanguage(const QString& code)
{
    if (code.isEmpty()) {
        return {};
    }
    // mpv emits ISO 639-2 ("eng") or ISO 639-1 ("en"). QLocale digests
    // either via languageToCode round-tripping; we give it the code
    // as-is and fall back to a titlecased copy when QLocale can't map
    // it (common for non-ISO tags like "und" or "mul").
    const QLocale loc(code);
    const auto lang = QLocale::languageToString(loc.language());
    if (lang.isEmpty()
        || QLocale(QStringLiteral("C")).language() == loc.language()) {
        // QLocale("und") / QLocale("xx") silently map to the C locale
        // — surface the raw code in that case so the user at least
        // sees what mpv sent.
        QString out = code;
        if (!out.isEmpty()) {
            out[0] = out[0].toUpper();
        }
        return out;
    }
    return lang;
}

QString codecDecor(const Entry& e)
{
    if (e.type == QLatin1String("audio")) {
        QString out = e.codec.toUpper();
        if (e.channelCount > 0) {
            switch (e.channelCount) {
            case 1: out += QLatin1String(" mono"); break;
            case 2: out += QLatin1String(" stereo"); break;
            case 6: out += QLatin1String(" 5.1"); break;
            case 8: out += QLatin1String(" 7.1"); break;
            default:
                out += QStringLiteral(" %1ch").arg(e.channelCount);
                break;
            }
        }
        return out.trimmed();
    }
    if (e.type == QLatin1String("sub") && !e.codec.isEmpty()) {
        return e.codec.toUpper();
    }
    return {};
}

} // namespace

TrackList parseTrackList(const QByteArray& json)
{
    QJsonParseError err {};
    const auto doc = QJsonDocument::fromJson(json, &err);
    if (err.error != QJsonParseError::NoError || !doc.isArray()) {
        return {};
    }
    TrackList out;
    out.reserve(doc.array().size());
    for (const auto& v : doc.array()) {
        if (!v.isObject()) {
            continue;
        }
        const auto o = v.toObject();
        Entry e;
        e.id = readInt(o, "id", -1);
        if (e.id < 0) {
            // An entry without an id is useless for selection; mpv
            // never emits this in practice, but guard anyway.
            continue;
        }
        e.type = readString(o, "type");
        e.title = readString(o, "title");
        e.lang = readString(o, "lang");
        e.codec = readString(o, "codec");
        e.selected = readBool(o, "selected");
        e.isDefault = readBool(o, "default");
        e.forced = readBool(o, "forced");
        e.external = readBool(o, "external");
        e.channelCount = readInt(o, "demux-channel-count");
        e.width = readInt(o, "demux-w");
        e.height = readInt(o, "demux-h");
        out.push_back(std::move(e));
    }
    return out;
}

QString formatLabel(const Entry& e)
{
    // Primary: prefer the explicit `title` mpv reports when present
    // (subs often carry meaningful titles like "English (SDH)").
    QString primary = e.title.trimmed();
    if (primary.isEmpty()) {
        primary = humanLanguage(e.lang);
    }
    if (primary.isEmpty()) {
        // Fall back to a positional label so the menu isn't blank.
        if (e.id > 0) {
            primary = i18nc(
                "@item:inmenu fallback track label when no title / "
                "language is known",
                "Track %1", e.id);
        } else {
            primary = i18nc("@item:inmenu", "Track");
        }
    }

    QStringList decorations;
    if (const auto d = codecDecor(e); !d.isEmpty()) {
        decorations.push_back(d);
    }
    if (e.forced) {
        decorations.push_back(i18nc(
            "@item:inmenu decoration on a forced subtitle track",
            "forced"));
    }
    if (e.external) {
        decorations.push_back(i18nc(
            "@item:inmenu decoration on a sidecar (external) track",
            "external"));
    }
    if (e.isDefault && !e.selected) {
        decorations.push_back(i18nc(
            "@item:inmenu decoration on the file's default track",
            "default"));
    }

    if (decorations.isEmpty()) {
        return primary;
    }
    return i18nc(
        "@item:inmenu track label followed by a parenthesised list "
        "of decorators — e.g. \"English (AC-3 5.1, default)\"",
        "%1 (%2)", primary, decorations.join(QStringLiteral(", ")));
}

namespace {

// Fill fields common to every entry (id, lang/title/codec, selected).
// Callers add the type-specific ones (`channels`, `forced`) afterwards.
QJsonObject commonJson(const Entry& e)
{
    QJsonObject o;
    o.insert(QStringLiteral("id"), e.id);
    if (!e.lang.isEmpty()) {
        o.insert(QStringLiteral("lang"), e.lang);
    }
    if (!e.title.isEmpty()) {
        o.insert(QStringLiteral("title"), e.title);
    }
    if (!e.codec.isEmpty()) {
        o.insert(QStringLiteral("codec"), e.codec);
    }
    if (e.selected) {
        o.insert(QStringLiteral("selected"), true);
    }
    if (e.isDefault) {
        o.insert(QStringLiteral("default"), true);
    }
    if (e.external) {
        o.insert(QStringLiteral("external"), true);
    }
    return o;
}

} // namespace

QByteArray toIpcJson(const TrackList& list)
{
    QJsonArray audio;
    QJsonArray subs;
    for (const auto& e : list) {
        if (e.type == QLatin1String("audio")) {
            QJsonObject o = commonJson(e);
            if (e.channelCount > 0) {
                o.insert(QStringLiteral("channels"), e.channelCount);
            }
            audio.append(o);
        } else if (e.type == QLatin1String("sub")) {
            QJsonObject o = commonJson(e);
            if (e.forced) {
                o.insert(QStringLiteral("forced"), true);
            }
            subs.append(o);
        }
        // Video tracks deliberately omitted — no picker for them.
    }
    QJsonObject root;
    root.insert(QStringLiteral("audio"), audio);
    root.insert(QStringLiteral("subtitle"), subs);
    return QJsonDocument(root).toJson(QJsonDocument::Compact);
}

} // namespace kinema::core::tracks
