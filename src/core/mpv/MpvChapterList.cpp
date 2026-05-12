// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/mpv/MpvChapterList.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QRegularExpression>

namespace kinema::core::chapters {

namespace {

/// Case-insensitive credit-marker title regex. Matches "End Credits",
/// "credits", "Ending", "End Titles", "End Card", etc. Deliberately
/// avoids bare "End" — too many false positives on act/finale
/// chapter names.
const QRegularExpression& creditsTitlePattern()
{
    static const QRegularExpression rx(
        QStringLiteral(
            R"(\b(end\s*credits?|credits|ending|end\s*titles?|end\s*card)\b)"),
        QRegularExpression::CaseInsensitiveOption);
    return rx;
}

constexpr double kUntitledTailMinFraction = 0.80;
constexpr double kUntitledTailMaxFraction = 0.97;

} // namespace

ChapterList parseChapterList(const QByteArray& json)
{
    QJsonParseError err {};
    const auto doc = QJsonDocument::fromJson(json, &err);
    if (err.error != QJsonParseError::NoError || !doc.isArray()) {
        return {};
    }
    ChapterList out;
    out.reserve(doc.array().size());
    for (const auto& v : doc.array()) {
        if (!v.isObject()) {
            continue;
        }
        const auto o = v.toObject();
        const auto t = o.value(QLatin1String("time"));
        if (!t.isDouble()) {
            // A chapter without a numeric time is useless for the
            // seek bar; skip silently.
            continue;
        }
        Chapter c;
        c.time = t.toDouble();
        c.title = o.value(QLatin1String("title")).toString();
        out.push_back(std::move(c));
    }
    return out;
}

QList<double> chapterTimes(const ChapterList& chapters)
{
    QList<double> out;
    out.reserve(chapters.size());
    for (const auto& c : chapters) {
        out.push_back(c.time);
    }
    return out;
}

std::optional<double> findCreditsStart(const ChapterList& chapters,
    double duration)
{
    if (chapters.isEmpty()) {
        return std::nullopt;
    }

    // (1) Titled match wins regardless of position.
    const auto& rx = creditsTitlePattern();
    for (const auto& c : chapters) {
        if (c.title.isEmpty()) {
            continue;
        }
        if (rx.match(c.title).hasMatch()) {
            return c.time;
        }
    }

    // (2) Untitled-last-chapter heuristic. Disabled without a real
    // duration since the window is relative.
    if (duration <= 0.0 || chapters.size() < 2) {
        return std::nullopt;
    }
    const auto& last = chapters.last();
    const double lo = duration * kUntitledTailMinFraction;
    const double hi = duration * kUntitledTailMaxFraction;
    if (last.time >= lo && last.time <= hi) {
        return last.time;
    }
    return std::nullopt;
}

} // namespace kinema::core::chapters
