// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/StreamTokens.h"

#include <QTest>

using namespace kinema;
using namespace kinema::core::stream_tokens;

namespace {

api::Stream makeStream(const QString& releaseName,
    const QString& detailsText = {},
    const QString& qualityLabel = {})
{
    api::Stream s;
    s.qualityLabel = qualityLabel;
    s.releaseName = releaseName;
    s.detailsText = detailsText;
    return s;
}

} // namespace

class TstStreamTokens : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void empty_returnsAllDefaults()
    {
        const auto t = parse({});
        QCOMPARE(t.source, Source::Unknown);
        QCOMPARE(t.codec, Codec::Unknown);
        QCOMPARE(t.hdr, Hdr::Sdr);
        QVERIFY(!t.tenBit);
        QVERIFY(t.audio.isEmpty());
        QVERIFY(t.languages.isEmpty());
        QVERIFY(!t.multiAudio);
        QVERIFY(t.releaseGroup.isEmpty());
    }

    // Real-world Torrentio row from the redesign screenshot.
    void from_s01_qxr_release()
    {
        const auto t = parse(makeStream(
            QStringLiteral("From (2022) Season 1 S01 (1080p AMZN WEB-DL "
                           "x265 HEVC 10bit EAC3 5.1 t3nzin) [QxR]")));
        QCOMPARE(t.source, Source::WebDl);
        QCOMPARE(t.codec, Codec::H265);
        QCOMPARE(t.hdr, Hdr::Sdr);
        QVERIFY(t.tenBit);
        QVERIFY(t.audio.contains(QStringLiteral("EAC3")));
        QCOMPARE(t.releaseGroup, QStringLiteral("QxR"));
    }

    void multi_french_acker_release()
    {
        const auto t = parse(makeStream(
            QStringLiteral("FROM.2022.S01.MULTI.VFF.1080p.WEB.EAC3.2.0.x264-ACKER"),
            QStringLiteral("\xF0\x9F\x87\xAB\xF0\x9F\x87\xB7"))); // 🇫🇷
        // "WEB" matches WebRip in our taxonomy (no "-DL").
        QCOMPARE(t.source, Source::WebRip);
        QCOMPARE(t.codec, Codec::H264);
        QVERIFY(t.audio.contains(QStringLiteral("EAC3")));
        QVERIFY(t.languages.contains(QStringLiteral("fr")));
        QVERIFY(t.multiAudio);
        QCOMPARE(t.releaseGroup, QStringLiteral("ACKER"));
    }

    void mgmp_2160p_webdl()
    {
        const auto t = parse(makeStream(
            QStringLiteral("FROM 2022 S01E01 Long Days Journey Into Night "
                           "2160p MGMP WEB-DL DDP5 1 H 264")));
        QCOMPARE(t.source, Source::WebDl);
        QCOMPARE(t.codec, Codec::H264);
        // DDP detection — channels capture is "5 1" → "5.1".
        bool foundDdp = false;
        for (const auto& a : t.audio) {
            if (a.startsWith(QStringLiteral("DDP"))) {
                foundDdp = true;
                break;
            }
        }
        QVERIFY(foundDdp);
    }

    void dolbyVision_beats_hdr10plus_beats_hdr10()
    {
        QCOMPARE(parse(makeStream(QStringLiteral("X.2160p.DV.HDR10.x265"))).hdr,
            Hdr::DolbyVision);
        QCOMPARE(parse(makeStream(QStringLiteral("X.2160p.HDR10+.x265"))).hdr,
            Hdr::Hdr10Plus);
        QCOMPARE(parse(makeStream(QStringLiteral("X.2160p.HDR10.x265"))).hdr,
            Hdr::Hdr10);
        QCOMPARE(parse(makeStream(QStringLiteral("X.2160p.HDR.x265"))).hdr,
            Hdr::Hdr10);
        QCOMPARE(parse(makeStream(QStringLiteral("X.1080p.x265"))).hdr,
            Hdr::Sdr);
    }

    void source_specific_before_general()
    {
        QCOMPARE(parse(makeStream(QStringLiteral(
            "Movie.2160p.BluRay.Remux.HEVC.DV"))).source,
            Source::BluRayRemux);
        QCOMPARE(parse(makeStream(QStringLiteral(
            "Movie.1080p.WEB-DL.DDP5.1"))).source,
            Source::WebDl);
        QCOMPARE(parse(makeStream(QStringLiteral(
            "Movie.1080p.WEBRip.x264"))).source,
            Source::WebRip);
        QCOMPARE(parse(makeStream(QStringLiteral(
            "Movie.1080p.BluRay.x265"))).source,
            Source::BluRay);
    }

    void codec_variants()
    {
        QCOMPARE(parse(makeStream(QStringLiteral("A.x265"))).codec, Codec::H265);
        QCOMPARE(parse(makeStream(QStringLiteral("A.HEVC"))).codec, Codec::H265);
        QCOMPARE(parse(makeStream(QStringLiteral("A.x264"))).codec, Codec::H264);
        QCOMPARE(parse(makeStream(QStringLiteral("A.AVC"))).codec, Codec::H264);
        QCOMPARE(parse(makeStream(QStringLiteral("A.AV1"))).codec, Codec::AV1);
        QCOMPARE(parse(makeStream(QStringLiteral("A.XviD"))).codec, Codec::Xvid);
    }

    void audio_atmos_wins_over_truehd_listed_first()
    {
        const auto t = parse(makeStream(
            QStringLiteral("Movie.2160p.UHD.BluRay.Atmos.TrueHD.7.1.x265")));
        // Atmos must be detected and listed before TrueHD per our
        // priority order.
        const int atmosIdx = t.audio.indexOf(QStringLiteral("Atmos"));
        const int trueHdIdx = t.audio.indexOf(QStringLiteral("TrueHD"));
        QVERIFY(atmosIdx >= 0);
        QVERIFY(trueHdIdx >= 0);
        QVERIFY(atmosIdx < trueHdIdx);
    }

    void audio_dts_hd_ma_wins_over_dts()
    {
        const auto t = parse(makeStream(
            QStringLiteral("Movie.1080p.BluRay.DTS-HD.MA.5.1.x264")));
        QVERIFY(t.audio.contains(QStringLiteral("DTS-HD MA")));
        QVERIFY(!t.audio.contains(QStringLiteral("DTS")));
    }

    void languages_detected_from_flags()
    {
        // 🇬🇧 + 🇪🇸
        const QString flags
            = QString::fromUtf8("\xF0\x9F\x87\xAC\xF0\x9F\x87\xA7"
                                "\xF0\x9F\x87\xAA\xF0\x9F\x87\xB8");
        const auto t = parse(makeStream(
            QStringLiteral("Movie.1080p.WEB.x264"), flags));
        QVERIFY(t.languages.contains(QStringLiteral("en")));
        QVERIFY(t.languages.contains(QStringLiteral("es")));
    }

    void languages_detected_from_iso_pair()
    {
        const auto t = parse(makeStream(
            QStringLiteral("From.S01.1080p.ITA-ENG.WEBRip.x265-V3SP4EV3R")));
        QVERIFY(t.languages.contains(QStringLiteral("it")));
        QVERIFY(t.languages.contains(QStringLiteral("en")));
        QCOMPARE(t.releaseGroup, QStringLiteral("V3SP4EV3R"));
    }

    void languages_detected_from_textual_hints()
    {
        QVERIFY(parse(makeStream(QStringLiteral(
            "Movie.2022.VFF.1080p.WEB.x264"))).languages.contains(
            QStringLiteral("fr")));
        QVERIFY(parse(makeStream(QStringLiteral(
            "Movie.2022.CASTELLANO.1080p.x265"))).languages.contains(
            QStringLiteral("es")));
        QVERIFY(parse(makeStream(QStringLiteral(
            "Movie.2022.GERMAN.1080p.x265"))).languages.contains(
            QStringLiteral("de")));
    }

    void multiAudio_set_when_dual_or_multi()
    {
        QVERIFY(parse(makeStream(QStringLiteral(
            "Movie.Dual.Audio.1080p.x264"))).multiAudio);
        QVERIFY(parse(makeStream(QStringLiteral(
            "Movie.Multi.Audio.1080p.x264"))).multiAudio);
        QVERIFY(parse(makeStream(QStringLiteral(
            "FROM.S01.MULTI.VFF.x264-ACKER"))).multiAudio);
        QVERIFY(!parse(makeStream(QStringLiteral(
            "Movie.1080p.x264"))).multiAudio);
    }

    void releaseGroup_bracket_form()
    {
        const auto t = parse(makeStream(
            QStringLiteral("Movie.1080p.WEB-DL.x265 [QxR]")));
        QCOMPARE(t.releaseGroup, QStringLiteral("QxR"));
    }

    void releaseGroup_dash_form()
    {
        const auto t = parse(makeStream(
            QStringLiteral("Movie.1080p.WEB-DL.x265-NOGRP")));
        QCOMPARE(t.releaseGroup, QStringLiteral("NOGRP"));
    }

    void releaseGroup_unparseable()
    {
        const auto t = parse(makeStream(
            QStringLiteral("Movie 1080p WEB-DL")));
        QVERIFY(t.releaseGroup.isEmpty());
    }

    void labels_are_localised_strings()
    {
        QCOMPARE(sourceLabel(Source::WebDl), QStringLiteral("WEB-DL"));
        QCOMPARE(sourceLabel(Source::BluRayRemux), QStringLiteral("BluRay Remux"));
        QVERIFY(sourceLabel(Source::Unknown).isEmpty());

        QCOMPARE(codecLabel(Codec::H265, false), QStringLiteral("x265"));
        QCOMPARE(codecLabel(Codec::H265, true), QStringLiteral("x265 10-bit"));
        QVERIFY(codecLabel(Codec::Unknown, false).isEmpty());

        QCOMPARE(hdrLabel(Hdr::DolbyVision), QStringLiteral("Dolby Vision"));
        QCOMPARE(hdrLabel(Hdr::Hdr10Plus), QStringLiteral("HDR10+"));
        QCOMPARE(hdrLabel(Hdr::Hdr10), QStringLiteral("HDR10"));
        QVERIFY(hdrLabel(Hdr::Sdr).isEmpty());
    }
};

QTEST_APPLESS_MAIN(TstStreamTokens)
#include "tst_stream_tokens.moc"
