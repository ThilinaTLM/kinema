// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/ChapterSkipPolicy.h"

#include <KLocalizedString>

#include <QRegularExpression>

namespace kinema::playback::policy {

namespace {

const QRegularExpression& skipRx()
{
    static const QRegularExpression rx(
        QStringLiteral("^(intro|opening|outro|ending|credits|end credits)\\b"),
        QRegularExpression::CaseInsensitiveOption);
    return rx;
}
const QRegularExpression& creditsRx()
{
    static const QRegularExpression rx(
        QStringLiteral("^(credits|end credits)\\b"),
        QRegularExpression::CaseInsensitiveOption);
    return rx;
}
const QRegularExpression& outroRx()
{
    static const QRegularExpression rx(
        QStringLiteral("^(outro|ending)\\b"),
        QRegularExpression::CaseInsensitiveOption);
    return rx;
}

} // namespace

SkipKind classifyChapterTitle(const QString& title)
{
    const QString t = title.trimmed();
    if (t.contains(creditsRx())) {
        return SkipKind::Credits;
    }
    if (t.contains(outroRx())) {
        return SkipKind::Outro;
    }
    return SkipKind::Intro;
}

QString skipButtonLabel(SkipKind kind)
{
    switch (kind) {
    case SkipKind::Credits:
        return i18nc("@action:button", "Skip credits");
    case SkipKind::Outro:
        return i18nc("@action:button", "Skip outro");
    case SkipKind::Intro:
        break;
    }
    return i18nc("@action:button", "Skip intro");
}

QString skipChapterKind(SkipKind kind)
{
    switch (kind) {
    case SkipKind::Credits:
        return QStringLiteral("credits");
    case SkipKind::Outro:
        return QStringLiteral("outro");
    case SkipKind::Intro:
        break;
    }
    return QStringLiteral("intro");
}

std::optional<SkipChapter> activeSkipChapter(
    const core::chapters::ChapterList& chapters,
    double seconds,
    double duration)
{
    for (int i = 0; i < chapters.size(); ++i) {
        const auto& ch = chapters[i];
        const QString title = ch.title.trimmed();
        if (!skipRx().match(title).hasMatch()) {
            continue;
        }
        const double start = ch.time;
        double end = -1.0;
        if (i + 1 < chapters.size()) {
            end = chapters[i + 1].time;
        } else if (duration > start) {
            end = duration;
        }
        if (end <= start) {
            continue;
        }
        if (seconds >= start && seconds < end) {
            SkipChapter out;
            out.kind = classifyChapterTitle(title);
            out.startSec = start;
            out.endSec = end;
            out.chapterTitle = title;
            return out;
        }
    }
    return std::nullopt;
}

} // namespace kinema::playback::policy
