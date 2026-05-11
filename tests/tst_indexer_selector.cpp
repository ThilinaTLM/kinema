// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Indexer.h"
#include "api/IndexerSelector.h"
#include "config/IndexerSettings.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

#include <QCoro/QCoroTask>

#include <memory>

using namespace kinema;

namespace {

class StubIndexer : public api::Indexer
{
public:
    explicit StubIndexer(api::IndexerKind k, QObject* parent = nullptr)
        : Indexer(parent)
        , m_kind(k)
    {
    }

    api::IndexerKind kind() const noexcept override { return m_kind; }
    QString displayName() const override
    {
        return QStringLiteral("Stub(%1)").arg(api::indexerKindToString(m_kind));
    }

    QCoro::Task<QList<api::Stream>> streams(api::MediaKind,
        QString) override
    {
        co_return QList<api::Stream> {};
    }

private:
    api::IndexerKind m_kind;
};

} // namespace

class TstIndexerSelector : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_tmpdir = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmpdir->isValid());
        m_config = KSharedConfig::openConfig(
            m_tmpdir->filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig);
    }

    void cleanup()
    {
        m_config.reset();
        m_tmpdir.reset();
    }

    void active_followsSettings()
    {
        config::IndexerSettings settings(m_config);
        api::IndexerSelector selector(settings);
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::Torrentio));
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::MediaFusion));

        // Default: Torrentio
        auto* a = selector.active();
        QVERIFY(a);
        QCOMPARE(a->kind(), api::IndexerKind::Torrentio);

        settings.setActiveIndexer(api::IndexerKind::MediaFusion);
        a = selector.active();
        QVERIFY(a);
        QCOMPARE(a->kind(), api::IndexerKind::MediaFusion);
    }

    void find_returnsIndexerRegardlessOfActive()
    {
        config::IndexerSettings settings(m_config);
        api::IndexerSelector selector(settings);
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::Torrentio));
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::MediaFusion));

        // Active is Torrentio but find() should still surface MediaFusion.
        auto* mf = selector.find(api::IndexerKind::MediaFusion);
        QVERIFY(mf);
        QCOMPARE(mf->kind(), api::IndexerKind::MediaFusion);
    }

    void active_returnsNullWhenNoIndexerRegisteredForKind()
    {
        config::IndexerSettings settings(m_config);
        settings.setActiveIndexer(api::IndexerKind::MediaFusion);

        api::IndexerSelector selector(settings);
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::Torrentio));

        // MediaFusion not registered → null.
        QVERIFY(!selector.active());
    }

    void registerIndexer_sameKindTwice_replacesPrevious()
    {
        config::IndexerSettings settings(m_config);
        api::IndexerSelector selector(settings);
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::Torrentio));

        auto* first = selector.find(api::IndexerKind::Torrentio);
        QVERIFY(first);

        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::Torrentio));

        auto* second = selector.find(api::IndexerKind::Torrentio);
        QVERIFY(second);
        QVERIFY(second != first);
        QCOMPARE(selector.all().size(), 1);
    }

    void activeIndexerChanged_signalReemitsFromSettings()
    {
        config::IndexerSettings settings(m_config);
        api::IndexerSelector selector(settings);
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::Torrentio));
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::MediaFusion));

        QSignalSpy spy(&selector,
            &api::IndexerSelector::activeIndexerChanged);
        settings.setActiveIndexer(api::IndexerKind::MediaFusion);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.at(0).at(0).value<api::IndexerKind>(),
            api::IndexerKind::MediaFusion);
    }

    void all_returnsRegistrationOrder()
    {
        config::IndexerSettings settings(m_config);
        api::IndexerSelector selector(settings);
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::MediaFusion));
        selector.registerIndexer(
            std::make_unique<StubIndexer>(api::IndexerKind::Torrentio));

        const auto all = selector.all();
        QCOMPARE(all.size(), 2);
        QCOMPARE(all.at(0)->kind(), api::IndexerKind::MediaFusion);
        QCOMPARE(all.at(1)->kind(), api::IndexerKind::Torrentio);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmpdir;
    KSharedConfigPtr m_config;
};

QTEST_GUILESS_MAIN(TstIndexerSelector)
#include "tst_indexer_selector.moc"
