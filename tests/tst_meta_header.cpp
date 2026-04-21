// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Media.h"
#include "core/HttpClient.h"
#include "ui/ImageLoader.h"
#include "ui/widgets/MetaHeaderWidget.h"

#include <QApplication>
#include <QLabel>
#include <QTest>

using kinema::ui::MetaHeaderWidget;

/**
 * MetaHeaderWidget smoke test. The widget composes an ImageLoader and
 * (optionally) a TmdbClient; we pass a nullptr for the latter so the
 * SimilarStrip isn't constructed and we don't need a live network.
 */
class TstMetaHeader : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        m_http = std::make_unique<kinema::core::HttpClient>();
        m_loader = std::make_unique<kinema::ui::ImageLoader>(m_http.get());
    }

    void cleanupTestCase()
    {
        m_loader.reset();
        m_http.reset();
    }

    void testDefaultConstructsWithoutTmdb()
    {
        MetaHeaderWidget w(m_loader.get(), /*tmdb=*/nullptr);
        // Must not crash on idle / loading / error flips.
        w.showIdle();
        w.showLoading();
        w.showError(QStringLiteral("boom"));
        w.showIdle();
    }

    void testSetMetaPopulatesTitle()
    {
        MetaHeaderWidget w(m_loader.get(), /*tmdb=*/nullptr);

        kinema::api::MetaDetail meta;
        meta.summary.title = QStringLiteral("Arrival");
        meta.summary.year = 2016;
        meta.summary.description = QStringLiteral("Linguist meets heptapods.");
        meta.genres = { QStringLiteral("Drama"),
            QStringLiteral("Science Fiction") };
        meta.runtimeMinutes = 116;
        meta.summary.imdbRating = 7.9;

        w.setMeta(meta);

        // The first QLabel child descending from the content area now
        // carries the formatted title.
        const auto labels = w.findChildren<QLabel*>();
        bool foundTitle = false;
        bool foundMetaLine = false;
        for (const auto* lbl : labels) {
            if (lbl->text() == QStringLiteral("Arrival (2016)")) {
                foundTitle = true;
            }
            if (lbl->text().contains(QStringLiteral("Drama"))
                && lbl->text().contains(QStringLiteral("116 min"))) {
                foundMetaLine = true;
            }
        }
        QVERIFY(foundTitle);
        QVERIFY(foundMetaLine);
    }

    void testSetSimilarContextNoCrashWithoutTmdb()
    {
        MetaHeaderWidget w(m_loader.get(), /*tmdb=*/nullptr);
        // Must be a silent no-op — the strip wasn't constructed.
        w.setSimilarContext(kinema::api::MediaKind::Movie,
            QStringLiteral("tt0000000"));
    }

private:
    std::unique_ptr<kinema::core::HttpClient> m_http;
    std::unique_ptr<kinema::ui::ImageLoader> m_loader;
};

QTEST_MAIN(TstMetaHeader)
#include "tst_meta_header.moc"
