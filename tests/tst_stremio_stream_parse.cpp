// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/StremioStreamParse.h"
#include "core/HttpError.h"

#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTest>

using namespace kinema::api;

class TstStremioStreamParse : public QObject
{
    Q_OBJECT
private:
    static QJsonDocument loadFixture(const char* name)
    {
        QFile f(QStringLiteral(KINEMA_TEST_FIXTURES_DIR) + QLatin1Char('/')
            + QString::fromLatin1(name));
        if (!f.open(QIODevice::ReadOnly)) {
            qFatal("Cannot open fixture %s", name);
        }
        return QJsonDocument::fromJson(f.readAll());
    }

private Q_SLOTS:
    void parsesAllActionableStreams_skipsUnactionableRow()
    {
        const auto doc = loadFixture("torrentio_stream_tt0133093.json");
        const auto streams = stremio::parseStreams(doc);

        // The fixture has 5 rows; the last one has neither hash nor URL
        // so parseStreams drops it. We expect 4.
        QCOMPARE(streams.size(), 4);
    }

    void extractsSeedersSizeProvider()
    {
        const auto doc = loadFixture("torrentio_stream_tt0133093.json");
        const auto streams = stremio::parseStreams(doc);

        const auto& first = streams.at(0);
        QCOMPARE(first.resolution, QStringLiteral("1080p"));
        QCOMPARE(first.releaseName,
            QStringLiteral("The.Matrix.1999.1080p.BluRay.x264-NOGRP"));
        QVERIFY(first.seeders.has_value());
        QCOMPARE(*first.seeders, 1500);
        QVERIFY(first.sizeBytes.has_value());
        // 2.1 GB → 2.1 * 1024^3 bytes
        const qint64 expected = static_cast<qint64>(2.1 * 1024.0 * 1024.0 * 1024.0);
        QVERIFY(std::llabs(*first.sizeBytes - expected) < 1024);
        QCOMPARE(first.provider, QStringLiteral("ThePirateBay"));
        QVERIFY(first.directUrl.isEmpty());
    }

    void extractsFileIndexFilenameAndSources()
    {
        const auto doc = loadFixture("torrentio_stream_tt0133093.json");
        const auto streams = stremio::parseStreams(doc);

        const auto& first = streams.at(0);
        QCOMPARE(first.fileIndex, 0);
        QCOMPARE(first.fileNameHint,
            QStringLiteral("The.Matrix.1999.1080p.BluRay.x264-NOGRP.mkv"));
        QCOMPARE(first.sources.size(), 2);
        QCOMPARE(first.sources.at(0),
            QStringLiteral("tracker:udp://tracker.example.com:80"));

        // Rows that don't carry fileIdx default to -1 and an empty
        // filename / sources list.
        const auto& noMeta = streams.at(3);
        QCOMPARE(noMeta.fileIndex, -1);
        QVERIFY(noMeta.fileNameHint.isEmpty());
        QVERIFY(noMeta.sources.isEmpty());
    }

    void exposesDirectUrlWhenPresent()
    {
        const auto doc = loadFixture("torrentio_stream_tt0133093.json");
        const auto streams = stremio::parseStreams(doc);

        // Row 1 in the fixture carries an `url` field, simulating an
        // RD-resolved direct hoster link the parser passes through
        // verbatim. Routing decisions live in the download manager;
        // the parser just surfaces what the addon returned.
        const auto& urlRow = streams.at(1);
        QCOMPARE(urlRow.resolution, QStringLiteral("2160p"));
        QCOMPARE(urlRow.directUrl,
            QUrl(QStringLiteral("https://real-debrid.com/d/ABCDEF/matrix.mkv")));
        // No 👤 tag in fixture → seeders is unset.
        QVERIFY(!urlRow.seeders.has_value());

        // Row 2 has no `url` field; directUrl stays empty regardless
        // of any "[RD download]"-style tagging in the name.
        const auto& noUrlRow = streams.at(2);
        QVERIFY(noUrlRow.directUrl.isEmpty());
        QVERIFY(noUrlRow.seeders.has_value());
        QCOMPARE(*noUrlRow.seeders, 62);
    }

    void handlesMissingMetadataGracefully()
    {
        const auto doc = loadFixture("torrentio_stream_tt0133093.json");
        const auto streams = stremio::parseStreams(doc);

        const auto& noMetaRow = streams.at(3);
        QVERIFY(!noMetaRow.seeders.has_value());
        QVERIFY(!noMetaRow.sizeBytes.has_value());
        QVERIFY(noMetaRow.provider.isEmpty());
    }

    void throwsOnNonObjectTopLevel()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("[]"));
        QVERIFY_EXCEPTION_THROWN(
            stremio::parseStreams(doc),
            kinema::core::HttpError);
    }

    void missingStreamsArray_returnsEmpty()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{}"));
        QVERIFY(stremio::parseStreams(doc).isEmpty());
    }

    void description_preferredOverDeprecatedTitle()
    {
        // Modern Stremio spec: `description`. Deprecated `title`
        // serves as fallback. When both are present, description
        // wins.
        const QByteArray body = R"({
            "streams": [{
                "infoHash": "aa",
                "title": "Old.Title\nshould.be.ignored",
                "description": "New.Description.Release\nDetail line"
            }]
        })";
        const auto streams
            = stremio::parseStreams(QJsonDocument::fromJson(body));
        QCOMPARE(streams.size(), 1);
        QCOMPARE(streams.at(0).releaseName,
            QStringLiteral("New.Description.Release"));
        QCOMPARE(streams.at(0).detailsText, QStringLiteral("Detail line"));
    }

    void title_fallbackWhenDescriptionMissing()
    {
        // Torrentio backward-compat path.
        const QByteArray body = R"({
            "streams": [{
                "infoHash": "bb",
                "title": "Legacy.Release\nbits"
            }]
        })";
        const auto streams
            = stremio::parseStreams(QJsonDocument::fromJson(body));
        QCOMPARE(streams.size(), 1);
        QCOMPARE(streams.at(0).releaseName,
            QStringLiteral("Legacy.Release"));
    }

    void structuredSeed_preferredOverRegex()
    {
        // No emoji metadata in the description — seeders comes from
        // the structured `seed` field.
        const QByteArray body = R"({
            "streams": [{
                "infoHash": "cc",
                "description": "Some.Release.Without.Emoji",
                "seed": 14
            }]
        })";
        const auto streams
            = stremio::parseStreams(QJsonDocument::fromJson(body));
        QVERIFY(streams.at(0).seeders.has_value());
        QCOMPARE(*streams.at(0).seeders, 14);
    }

    void structuredSizebytes_preferredOverRegex()
    {
        const QByteArray body = R"({
            "streams": [{
                "infoHash": "dd",
                "description": "Some.Release",
                "sizebytes": 60333553090
            }]
        })";
        const auto streams
            = stremio::parseStreams(QJsonDocument::fromJson(body));
        QVERIFY(streams.at(0).sizeBytes.has_value());
        QCOMPARE(*streams.at(0).sizeBytes, qint64(60333553090LL));
    }

    void structuredQuality_preferredOverRegex()
    {
        const QByteArray body = R"({
            "streams": [{
                "infoHash": "ee",
                "description": "Release.Without.Resolution.Token",
                "quality": "4K"
            }]
        })";
        const auto streams
            = stremio::parseStreams(QJsonDocument::fromJson(body));
        QCOMPARE(streams.at(0).resolution, QStringLiteral("4k"));
    }

    void language_populatedWhenPresent_emptyOtherwise()
    {
        const QByteArray body = R"({
            "streams": [
                { "infoHash": "ff", "title": "x", "language": "ES" },
                { "infoHash": "gg", "title": "y" }
            ]
        })";
        const auto streams
            = stremio::parseStreams(QJsonDocument::fromJson(body));
        QCOMPARE(streams.size(), 2);
        QCOMPARE(streams.at(0).language, QStringLiteral("es"));
        QVERIFY(streams.at(1).language.isEmpty());
    }
};

QTEST_APPLESS_MAIN(TstStremioStreamParse)
#include "tst_stremio_stream_parse.moc"
