// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MpvChapterList.h"
#include "core/MpvTrackList.h"

#include <QByteArray>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QTest>

using namespace kinema::core;

namespace {

QByteArray readFixture(const char* relative)
{
    const QString path = QStringLiteral(KINEMA_TEST_FIXTURES_DIR "/")
        + QString::fromLatin1(relative);
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly)) {
        return {};
    }
    return f.readAll();
}

} // namespace

class TestMpvTrackListParse : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void parsesMultiTrackFixture()
    {
        const auto json = readFixture("mpv/track_list_multi.json");
        QVERIFY2(!json.isEmpty(), "fixture missing");

        const auto list = tracks::parseTrackList(json);
        QCOMPARE(list.size(), 6);

        // Video track at index 0.
        QCOMPARE(list[0].type, QStringLiteral("video"));
        QCOMPARE(list[0].id, 1);
        QCOMPARE(list[0].width, 1920);
        QCOMPARE(list[0].height, 1080);
        QVERIFY(list[0].selected);
        QCOMPARE(list[0].codec, QStringLiteral("h264"));

        // First audio: English AC-3 5.1, selected, default, title "Main".
        const auto& eng = list[1];
        QCOMPARE(eng.type, QStringLiteral("audio"));
        QCOMPARE(eng.id, 1);
        QCOMPARE(eng.lang, QStringLiteral("eng"));
        QCOMPARE(eng.codec, QStringLiteral("ac3"));
        QCOMPARE(eng.channelCount, 6);
        QVERIFY(eng.selected);
        QVERIFY(eng.isDefault);
        QVERIFY(!eng.forced);
        QVERIFY(!eng.external);

        // Second audio: Japanese stereo, not selected.
        const auto& jpn = list[2];
        QCOMPARE(jpn.type, QStringLiteral("audio"));
        QCOMPARE(jpn.id, 2);
        QCOMPARE(jpn.lang, QStringLiteral("jpn"));
        QCOMPARE(jpn.channelCount, 2);
        QVERIFY(!jpn.selected);

        // Subtitle 1: English selected default.
        QCOMPARE(list[3].type, QStringLiteral("sub"));
        QCOMPARE(list[3].id, 1);
        QVERIFY(list[3].selected);
        QVERIFY(list[3].isDefault);

        // Subtitle 2: English forced.
        QCOMPARE(list[4].type, QStringLiteral("sub"));
        QCOMPARE(list[4].id, 2);
        QVERIFY(list[4].forced);
        QVERIFY(!list[4].selected);

        // Subtitle 3: external sidecar, explicit title.
        QCOMPARE(list[5].type, QStringLiteral("sub"));
        QCOMPARE(list[5].id, 3);
        QVERIFY(list[5].external);
        QCOMPARE(list[5].title, QStringLiteral("Director commentary"));
    }

    // The explicit title always wins over the resolved language in
    // the menu label; language-only tracks fall back to the language.
    void formatLabelPrefersTitleThenLanguage()
    {
        tracks::Entry titled;
        titled.id = 1;
        titled.type = QStringLiteral("sub");
        titled.title = QStringLiteral("English (SDH)");
        titled.lang = QStringLiteral("eng");
        const auto a = tracks::formatLabel(titled);
        QVERIFY(a.startsWith(QStringLiteral("English (SDH)")));

        tracks::Entry langOnly;
        langOnly.id = 2;
        langOnly.type = QStringLiteral("audio");
        langOnly.lang = QStringLiteral("jpn");
        langOnly.codec = QStringLiteral("aac");
        langOnly.channelCount = 2;
        const auto b = tracks::formatLabel(langOnly);
        QVERIFY(b.contains(QStringLiteral("Japanese")));
        QVERIFY(b.contains(QStringLiteral("AAC")));
        QVERIFY(b.contains(QStringLiteral("stereo")));
    }

    // Empty metadata still produces a non-empty label so the menu
    // never shows a blank row.
    void formatLabelFallsBackToPositional()
    {
        tracks::Entry blank;
        blank.id = 3;
        blank.type = QStringLiteral("sub");
        const auto label = tracks::formatLabel(blank);
        QVERIFY(!label.isEmpty());
        QVERIFY(label.contains(QStringLiteral("3")));
    }

    // Forced / external decorators are appended; the "default" tag is
    // suppressed on the already-selected track to avoid noise.
    void formatLabelDecorators()
    {
        tracks::Entry forced;
        forced.id = 2;
        forced.type = QStringLiteral("sub");
        forced.lang = QStringLiteral("eng");
        forced.codec = QStringLiteral("subrip");
        forced.forced = true;
        const auto a = tracks::formatLabel(forced);
        QVERIFY(a.contains(QStringLiteral("forced")));

        tracks::Entry selectedDefault;
        selectedDefault.id = 1;
        selectedDefault.type = QStringLiteral("audio");
        selectedDefault.lang = QStringLiteral("eng");
        selectedDefault.codec = QStringLiteral("ac3");
        selectedDefault.isDefault = true;
        selectedDefault.selected = true;
        selectedDefault.channelCount = 6;
        const auto b = tracks::formatLabel(selectedDefault);
        QVERIFY(!b.contains(QStringLiteral("default")));
    }

    // Garbage JSON must not crash; returns empty.
    void malformedJsonReturnsEmpty()
    {
        QCOMPARE(
            tracks::parseTrackList(QByteArrayLiteral("not json")).size(),
            0);
        QCOMPARE(
            tracks::parseTrackList(QByteArrayLiteral("{\"a\":1}")).size(),
            0);
        QCOMPARE(
            tracks::parseTrackList(QByteArrayLiteral("[]")).size(),
            0);
    }

    // Per-entry malformations are tolerated — the good entries still
    // come through.
    void tolerantPerEntry()
    {
        const auto out = tracks::parseTrackList(QByteArrayLiteral(
            "[{\"id\":1,\"type\":\"audio\",\"lang\":\"eng\"},"
            " 42,"
            " {\"type\":\"audio\"},"
            " {\"id\":2,\"type\":\"audio\",\"lang\":\"fre\"}]"));
        QCOMPARE(out.size(), 2);
        QCOMPARE(out[0].lang, QStringLiteral("eng"));
        QCOMPARE(out[1].lang, QStringLiteral("fre"));
    }

    // --- chapter-list -------------------------------------------------

    void parsesChapterFixture()
    {
        const auto json = readFixture("mpv/chapter_list.json");
        QVERIFY2(!json.isEmpty(), "fixture missing");
        const auto list = chapters::parseChapterList(json);
        QCOMPARE(list.size(), 4);
        QCOMPARE(list[0].time, 0.0);
        QCOMPARE(list[0].title, QStringLiteral("Opening"));
        QCOMPARE(list[3].title, QStringLiteral("Credits"));

        const auto times = chapters::chapterTimes(list);
        QCOMPARE(times.size(), 4);
        QCOMPARE(times[1], 92.0);
    }

    // --- toIpcJson ----------------------------------------------------

    // Empty list collapses to the documented `{"audio":[],"subtitle":[]}`
    // shape — the Lua side assumes both keys are always present.
    void toIpcJsonEmpty()
    {
        const auto bytes = tracks::toIpcJson({});
        QJsonParseError err {};
        const auto doc = QJsonDocument::fromJson(bytes, &err);
        QCOMPARE(err.error, QJsonParseError::NoError);
        QVERIFY(doc.isObject());
        const auto root = doc.object();
        QVERIFY(root.value(QStringLiteral("audio")).isArray());
        QVERIFY(root.value(QStringLiteral("subtitle")).isArray());
        QCOMPARE(root.value(QStringLiteral("audio")).toArray().size(), 0);
        QCOMPARE(root.value(QStringLiteral("subtitle")).toArray().size(), 0);
    }

    // Single audio + single subtitle with no selection. Field
    // presence is tested via `contains`, not string comparison, so
    // Qt's JSON serialiser key ordering doesn't matter.
    void toIpcJsonSingleOfEach()
    {
        tracks::Entry a;
        a.id = 1;
        a.type = QStringLiteral("audio");
        a.lang = QStringLiteral("eng");
        a.codec = QStringLiteral("eac3");
        a.channelCount = 6;

        tracks::Entry s;
        s.id = 1;
        s.type = QStringLiteral("sub");
        s.lang = QStringLiteral("eng");
        s.codec = QStringLiteral("subrip");

        const auto bytes = tracks::toIpcJson({ a, s });
        const auto root = QJsonDocument::fromJson(bytes).object();
        const auto audio = root.value(QStringLiteral("audio")).toArray();
        const auto subs = root.value(QStringLiteral("subtitle")).toArray();
        QCOMPARE(audio.size(), 1);
        QCOMPARE(subs.size(), 1);

        const auto ao = audio.at(0).toObject();
        QCOMPARE(ao.value(QStringLiteral("id")).toInt(), 1);
        QCOMPARE(ao.value(QStringLiteral("lang")).toString(),
            QStringLiteral("eng"));
        QCOMPARE(ao.value(QStringLiteral("codec")).toString(),
            QStringLiteral("eac3"));
        QCOMPARE(ao.value(QStringLiteral("channels")).toInt(), 6);
        QVERIFY(!ao.contains(QStringLiteral("selected")));

        const auto so = subs.at(0).toObject();
        QCOMPARE(so.value(QStringLiteral("id")).toInt(), 1);
        QVERIFY(!so.contains(QStringLiteral("selected")));
        QVERIFY(!so.contains(QStringLiteral("forced")));
    }

    // Multi-track: selected audio, forced subtitle, and a video
    // track that must be dropped.
    void toIpcJsonMultiDropsVideoAndKeepsSelection()
    {
        tracks::Entry video;
        video.id = 1;
        video.type = QStringLiteral("video");

        tracks::Entry aSel;
        aSel.id = 1;
        aSel.type = QStringLiteral("audio");
        aSel.lang = QStringLiteral("eng");
        aSel.selected = true;

        tracks::Entry aOther;
        aOther.id = 2;
        aOther.type = QStringLiteral("audio");
        aOther.lang = QStringLiteral("jpn");

        tracks::Entry sForced;
        sForced.id = 1;
        sForced.type = QStringLiteral("sub");
        sForced.lang = QStringLiteral("eng");
        sForced.forced = true;

        const auto bytes
            = tracks::toIpcJson({ video, aSel, aOther, sForced });
        const auto root = QJsonDocument::fromJson(bytes).object();
        const auto audio = root.value(QStringLiteral("audio")).toArray();
        const auto subs = root.value(QStringLiteral("subtitle")).toArray();
        QCOMPARE(audio.size(), 2);
        QCOMPARE(subs.size(), 1);

        // Video dropped — check ids emitted are strictly audio/sub.
        QCOMPARE(audio.at(0).toObject().value(QStringLiteral("id")).toInt(), 1);
        QVERIFY(audio.at(0).toObject().value(QStringLiteral("selected")).toBool());
        QCOMPARE(audio.at(1).toObject().value(QStringLiteral("id")).toInt(), 2);
        QVERIFY(!audio.at(1).toObject().contains(QStringLiteral("selected")));

        const auto so = subs.at(0).toObject();
        QVERIFY(so.value(QStringLiteral("forced")).toBool());
        QVERIFY(!so.contains(QStringLiteral("selected")));
    }

    void malformedChaptersAreDropped()
    {
        const auto list = chapters::parseChapterList(QByteArrayLiteral(
            "[{\"time\":1.5,\"title\":\"ok\"},"
            " {\"title\":\"no time\"},"
            " null,"
            " {\"time\":10.0}]"));
        QCOMPARE(list.size(), 2);
        QCOMPARE(list[0].time, 1.5);
        QCOMPARE(list[1].time, 10.0);
        QVERIFY(list[1].title.isEmpty());
    }
};

QTEST_GUILESS_MAIN(TestMpvTrackListParse)
#include "tst_mpv_track_list_parse.moc"
