// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

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
        QVERIFY(!first.rdCached);
        QVERIFY(!first.rdDownload);
        QVERIFY(first.directUrl.isEmpty());
    }

    void detectsRdCachedFlag_andDirectUrl()
    {
        const auto doc = loadFixture("torrentio_stream_tt0133093.json");
        const auto streams = torrentio::parseStreams(doc);

        const auto& rdRow = streams.at(1);
        QCOMPARE(rdRow.resolution, QStringLiteral("2160p"));
        QVERIFY(rdRow.rdCached);
        QVERIFY(!rdRow.rdDownload);
        QCOMPARE(rdRow.directUrl,
            QUrl(QStringLiteral("https://real-debrid.com/d/ABCDEF/matrix.mkv")));
        // No 👤 tag in fixture → seeders is unset.
        QVERIFY(!rdRow.seeders.has_value());
    }

    void detectsRdDownloadFlag()
    {
        const auto doc = loadFixture("torrentio_stream_tt0133093.json");
        const auto streams = torrentio::parseStreams(doc);

        const auto& rdDownloadRow = streams.at(2);
        QVERIFY(!rdDownloadRow.rdCached);
        QVERIFY(rdDownloadRow.rdDownload);
        QVERIFY(rdDownloadRow.seeders.has_value());
        QCOMPARE(*rdDownloadRow.seeders, 62);
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
