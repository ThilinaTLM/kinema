// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MpvChapterList.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>

namespace kinema::core::chapters {

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

} // namespace kinema::core::chapters
