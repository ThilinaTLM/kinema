// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/AudioTracksModel.h"

#include <QSignalSpy>
#include <QTest>

using kinema::core::tracks::Entry;
using kinema::core::tracks::TrackList;
using kinema::ui::player::AudioTracksModel;

class TestAudioTracksModel : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void filtersAudioOnly();
    void exposesRoles();
    void countSignal();
    void selectedIdMirrorsTracks();
};

namespace {

Entry makeEntry(const QString& type, int id, bool selected = false)
{
    Entry e;
    e.id = id;
    e.type = type;
    e.lang = QStringLiteral("eng");
    e.codec = QStringLiteral("eac3");
    e.channelCount = 6;
    e.selected = selected;
    return e;
}

} // namespace

void TestAudioTracksModel::filtersAudioOnly()
{
    AudioTracksModel m;
    TrackList list;
    list << makeEntry(QStringLiteral("video"), 1)
         << makeEntry(QStringLiteral("audio"), 2)
         << makeEntry(QStringLiteral("audio"), 3)
         << makeEntry(QStringLiteral("sub"),   4);
    m.reset(list);
    QCOMPARE(m.rowCount(), 2);
    QCOMPARE(m.count(), 2);
}

void TestAudioTracksModel::exposesRoles()
{
    AudioTracksModel m;
    TrackList list;
    list << makeEntry(QStringLiteral("audio"), 7, /*selected=*/true);
    m.reset(list);

    const auto idx = m.index(0);
    QCOMPARE(m.data(idx, AudioTracksModel::IdRole).toInt(), 7);
    QCOMPARE(m.data(idx, AudioTracksModel::SelectedRole).toBool(), true);
    QCOMPARE(m.data(idx, AudioTracksModel::LangRole).toString(),
        QStringLiteral("eng"));
    QCOMPARE(m.data(idx, AudioTracksModel::ChannelsRole).toInt(), 6);

    const auto names = m.roleNames();
    QVERIFY(names.contains(AudioTracksModel::IdRole));
    QVERIFY(names.contains(AudioTracksModel::TitleRole));
}

void TestAudioTracksModel::countSignal()
{
    AudioTracksModel m;
    QSignalSpy spy(&m, &AudioTracksModel::countChanged);

    TrackList list;
    list << makeEntry(QStringLiteral("audio"), 1);
    m.reset(list);
    QCOMPARE(spy.count(), 1);

    list << makeEntry(QStringLiteral("audio"), 2);
    m.reset(list);
    QCOMPARE(spy.count(), 2);

    // Same size \u2192 no extra emission.
    m.reset(list);
    QCOMPARE(spy.count(), 2);
}

void TestAudioTracksModel::selectedIdMirrorsTracks()
{
    AudioTracksModel m;
    TrackList list;
    list << makeEntry(QStringLiteral("audio"), 5, /*selected=*/false)
         << makeEntry(QStringLiteral("audio"), 6, /*selected=*/true);
    m.reset(list);
    QCOMPARE(m.selectedId(), 6);
}

QTEST_MAIN(TestAudioTracksModel)
#include "tst_player_audio_tracks_model.moc"
