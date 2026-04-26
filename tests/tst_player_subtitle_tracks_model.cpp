// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/SubtitleTracksModel.h"

#include <QTest>

using kinema::core::tracks::Entry;
using kinema::core::tracks::TrackList;
using kinema::ui::player::SubtitleTracksModel;

class TestSubtitleTracksModel : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void filtersSubsOnly();
    void exposesForcedAndExternal();
};

void TestSubtitleTracksModel::filtersSubsOnly()
{
    SubtitleTracksModel m;
    TrackList list;
    Entry video; video.id = 1; video.type = QStringLiteral("video");
    Entry audio; audio.id = 2; audio.type = QStringLiteral("audio");
    Entry sub;   sub.id = 3;   sub.type = QStringLiteral("sub");
    list << video << audio << sub;
    m.reset(list);
    QCOMPARE(m.rowCount(), 1);
    QCOMPARE(m.count(), 1);
}

void TestSubtitleTracksModel::exposesForcedAndExternal()
{
    SubtitleTracksModel m;
    TrackList list;
    Entry sub;
    sub.id = 4;
    sub.type = QStringLiteral("sub");
    sub.forced = true;
    sub.external = true;
    sub.selected = true;
    list << sub;
    m.reset(list);

    const auto idx = m.index(0);
    QCOMPARE(m.data(idx, SubtitleTracksModel::ForcedRole).toBool(), true);
    QCOMPARE(m.data(idx, SubtitleTracksModel::ExternalRole).toBool(), true);
    QCOMPARE(m.data(idx, SubtitleTracksModel::SelectedRole).toBool(), true);
    QCOMPARE(m.selectedId(), 4);
}

QTEST_MAIN(TestSubtitleTracksModel)
#include "tst_player_subtitle_tracks_model.moc"
