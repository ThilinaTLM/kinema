// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TorrentioParse.h"
#include "core/HttpError.h"

#include <QFile>
#include <QJsonDocument>
#include <QTest>

using namespace kinema::api;

class TstTorrentioParse : public QObject
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
        const auto streams = torrentio::parseStreams(doc);

        // The fixture has 5 rows; the last one has neither hash nor URL
        // so parseStreams drops it. We expect 4.
        QCOMPARE(streams.size(), 4);
    }

    void extractsSeedersSizeProvider()
    {
        const auto doc = loadFixture("torrentio_stream_tt0133093.json");
        const auto streams = torrentio::parseStreams(doc);

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
        const auto streams = torrentio::parseStreams(doc);

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
        const auto streams = torrentio::parseStreams(doc);

        // Row 1 in the fixture carries an `url` field, simulating an
        // RD-resolved direct hoster link the parser passes through
        // verbatim. Routing decisions live in the download manager;
        // the parser just surfaces what Torrentio returned.
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
        const auto streams = torrentio::parseStreams(doc);

        const auto& noMetaRow = streams.at(3);
        QVERIFY(!noMetaRow.seeders.has_value());
        QVERIFY(!noMetaRow.sizeBytes.has_value());
        QVERIFY(noMetaRow.provider.isEmpty());
    }

    void throwsOnNonObjectTopLevel()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("[]"));
        QVERIFY_EXCEPTION_THROWN(
            torrentio::parseStreams(doc),
            kinema::core::HttpError);
    }

    void missingStreamsArray_returnsEmpty()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{}"));
        QVERIFY(torrentio::parseStreams(doc).isEmpty());
    }
};

QTEST_APPLESS_MAIN(TstTorrentioParse)
#include "tst_torrentio_parse.moc"
