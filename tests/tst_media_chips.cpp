// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MediaChips.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QString>
#include <QStringList>
#include <QTest>

using namespace kinema::core::media_chips;

namespace {

QStringList parse(const QByteArray& json)
{
    const auto doc = QJsonDocument::fromJson(json);
    const auto arr = doc.array();
    QStringList out;
    out.reserve(arr.size());
    for (const auto& v : arr) {
        out.append(v.toString());
    }
    return out;
}

} // namespace

class TstMediaChips : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void empty_input_produces_empty_array()
    {
        ChipInputs in;
        const auto chips = parse(toIpcJson(in));
        QCOMPARE(chips.size(), 0);
    }

    void resolution_4k_for_uhd()
    {
        ChipInputs in;
        in.videoHeight = 2160;
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("4K")));

        in.videoHeight = 2178; // 4K Blu-ray crop
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("4K")));
    }

    void resolution_canonical_tiers()
    {
        const QList<QPair<int, QString>> cases = {
            { 1440, QStringLiteral("1440p") },
            { 1080, QStringLiteral("1080p") },
            { 1088, QStringLiteral("1080p") }, // rounded-up encoding height
            {  720, QStringLiteral("720p")  },
            {  480, QStringLiteral("480p")  },
            {  360, QStringLiteral("360p")  },
        };
        for (const auto& c : cases) {
            ChipInputs in;
            in.videoHeight = c.first;
            const auto chips = parse(toIpcJson(in));
            QVERIFY2(chips.contains(c.second),
                qPrintable(QStringLiteral("height %1 -> expected %2, got %3")
                               .arg(c.first).arg(c.second,
                                   chips.join(QLatin1Char(',')))));
        }
    }

    void resolution_raw_fallback_for_offladder()
    {
        ChipInputs in;
        in.videoHeight = 576;
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("576p")));
    }

    void resolution_zero_is_omitted()
    {
        ChipInputs in;
        in.videoHeight = 0;
        QCOMPARE(parse(toIpcJson(in)).size(), 0);
    }

    void hdr10_from_bt2020_pq()
    {
        ChipInputs in;
        in.videoHeight  = 2160;
        in.hdrPrimaries = QStringLiteral("bt.2020");
        in.hdrGamma     = QStringLiteral("pq");
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("HDR10")));
    }

    void hlg_from_bt2020_hlg()
    {
        ChipInputs in;
        in.hdrPrimaries = QStringLiteral("bt.2020");
        in.hdrGamma     = QStringLiteral("hlg");
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("HLG")));
    }

    void dolbyvision_from_dolby_marker()
    {
        ChipInputs in;
        in.hdrGamma = QStringLiteral("dolbyvision");
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("DV")));
    }

    void sdr_is_not_tagged()
    {
        ChipInputs in;
        in.hdrPrimaries = QStringLiteral("bt.709");
        in.hdrGamma     = QStringLiteral("bt.1886");
        const auto chips = parse(toIpcJson(in));
        for (const auto& c : chips) {
            QVERIFY(c != QStringLiteral("HDR10"));
            QVERIFY(c != QStringLiteral("HLG"));
            QVERIFY(c != QStringLiteral("DV"));
        }
    }

    void video_codec_friendly_names()
    {
        const QList<QPair<QString, QString>> cases = {
            { QStringLiteral("hevc"), QStringLiteral("H.265") },
            { QStringLiteral("h264"), QStringLiteral("H.264") },
            { QStringLiteral("avc1"), QStringLiteral("H.264") },
            { QStringLiteral("av1"),  QStringLiteral("AV1")   },
            { QStringLiteral("vp9"),  QStringLiteral("VP9")   },
        };
        for (const auto& c : cases) {
            ChipInputs in;
            in.videoCodec = c.first;
            QVERIFY2(parse(toIpcJson(in)).contains(c.second),
                qPrintable(QStringLiteral("codec %1 -> expected %2")
                               .arg(c.first, c.second)));
        }
    }

    void video_codec_fallback_uppercases_raw()
    {
        ChipInputs in;
        in.videoCodec = QStringLiteral("prores");
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("PRORES")));
    }

    void audio_label_combines_codec_and_layout()
    {
        ChipInputs in;
        in.audioCodec    = QStringLiteral("eac3");
        in.audioChannels = 6;
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("EAC3 5.1")));

        in.audioCodec    = QStringLiteral("aac");
        in.audioChannels = 2;
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("AAC 2.0")));

        in.audioCodec    = QStringLiteral("dts");
        in.audioChannels = 8;
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("DTS 7.1")));
    }

    void audio_channels_zero_drops_layout()
    {
        ChipInputs in;
        in.audioCodec = QStringLiteral("aac");
        QVERIFY(parse(toIpcJson(in)).contains(QStringLiteral("AAC")));
    }

    void full_chip_row_order_is_res_hdr_video_audio()
    {
        ChipInputs in;
        in.videoHeight   = 2160;
        in.hdrPrimaries  = QStringLiteral("bt.2020");
        in.hdrGamma      = QStringLiteral("pq");
        in.videoCodec    = QStringLiteral("hevc");
        in.audioCodec    = QStringLiteral("eac3");
        in.audioChannels = 6;
        const auto chips = parse(toIpcJson(in));
        QCOMPARE(chips, (QStringList{
            QStringLiteral("4K"),
            QStringLiteral("HDR10"),
            QStringLiteral("H.265"),
            QStringLiteral("EAC3 5.1"),
        }));
    }
};

QTEST_APPLESS_MAIN(TstMediaChips)
#include "tst_media_chips.moc"
