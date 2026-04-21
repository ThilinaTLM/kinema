// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/CinemetaClient.h"
#include "api/TmdbClient.h"
#include "api/TorrentioClient.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/TokenStore.h"

#include <QCoro/QCoroSignal>
#include <QCoro/QCoroTask>

#include <QCoreApplication>
#include <QFile>
#include <QHash>
#include <QJsonDocument>
#include <QNetworkRequest>
#include <QSet>
#include <QTimer>

#include <optional>
#include <stdexcept>

namespace kinema::tests {

inline void drainEvents(int rounds = 2)
{
    for (int i = 0; i < rounds; ++i) {
        QCoreApplication::processEvents();
    }
}

inline QCoro::Task<void> nextEventLoopTurn()
{
    QTimer timer;
    timer.setSingleShot(true);
    timer.start(0);
    co_await qCoro(&timer, &QTimer::timeout);
}

inline QJsonDocument loadJsonFixture(const char* name)
{
    QFile f(QStringLiteral(KINEMA_TEST_FIXTURES_DIR) + QLatin1Char('/')
        + QString::fromLatin1(name));
    if (!f.open(QIODevice::ReadOnly)) {
        throw std::runtime_error("could not open fixture");
    }
    return QJsonDocument::fromJson(f.readAll());
}

class FakeHttpClient : public core::HttpClient
{
public:
    struct Call {
        bool json = false;
        bool usedRequest = false;
        QUrl url;
        QNetworkRequest request;
    };

    explicit FakeHttpClient(QObject* parent = nullptr)
        : HttpClient(parent)
    {
    }

    QList<Call> calls;
    QList<QJsonDocument> jsonReplies;
    QList<QByteArray> byteReplies;
    std::optional<core::HttpError> nextError;

    QCoro::Task<QByteArray> get(QUrl url) override
    {
        calls.append({ false, false, url, {} });
        if (nextError) {
            throw *nextError;
        }
        co_return byteReplies.isEmpty() ? QByteArray {} : byteReplies.takeFirst();
    }

    QCoro::Task<QJsonDocument> getJson(QUrl url) override
    {
        calls.append({ true, false, url, {} });
        if (nextError) {
            throw *nextError;
        }
        co_return jsonReplies.isEmpty() ? QJsonDocument {} : jsonReplies.takeFirst();
    }

    QCoro::Task<QByteArray> get(QNetworkRequest request) override
    {
        calls.append({ false, true, {}, request });
        if (nextError) {
            throw *nextError;
        }
        co_return byteReplies.isEmpty() ? QByteArray {} : byteReplies.takeFirst();
    }

    QCoro::Task<QJsonDocument> getJson(QNetworkRequest request) override
    {
        calls.append({ true, true, {}, request });
        if (nextError) {
            throw *nextError;
        }
        co_return jsonReplies.isEmpty() ? QJsonDocument {} : jsonReplies.takeFirst();
    }
};

class FakeTmdbClient : public api::TmdbClient
{
public:
    explicit FakeTmdbClient(QObject* parent = nullptr)
        : TmdbClient(nullptr, parent)
    {
    }

    int movieLookupCalls = 0;
    int seriesLookupCalls = 0;
    QString movieImdbId;
    QString seriesImdbId;
    std::optional<core::HttpError> movieLookupError;
    std::optional<core::HttpError> seriesLookupError;

    QCoro::Task<QString> imdbIdForTmdbMovie(int) override
    {
        ++movieLookupCalls;
        if (movieLookupError) {
            throw *movieLookupError;
        }
        co_return movieImdbId;
    }

    QCoro::Task<QString> imdbIdForTmdbSeries(int) override
    {
        ++seriesLookupCalls;
        if (seriesLookupError) {
            throw *seriesLookupError;
        }
        co_return seriesImdbId;
    }
};

class FakeTorrentioClient : public api::TorrentioClient
{
public:
    struct ScriptedCall {
        QList<api::Stream> streams;
        std::optional<core::HttpError> error;
        bool suspend = false;
    };

    explicit FakeTorrentioClient(QObject* parent = nullptr)
        : TorrentioClient(nullptr, parent)
    {
    }

    QList<ScriptedCall> scriptedCalls;
    int callCount = 0;
    api::MediaKind lastKind = api::MediaKind::Movie;
    QString lastStreamId;
    core::torrentio::ConfigOptions lastOptions;

    QCoro::Task<QList<api::Stream>> streams(api::MediaKind kind,
        QString streamId,
        core::torrentio::ConfigOptions opts = {}) override
    {
        ++callCount;
        lastKind = kind;
        lastStreamId = streamId;
        lastOptions = std::move(opts);

        ScriptedCall scripted;
        if (!scriptedCalls.isEmpty()) {
            scripted = scriptedCalls.takeFirst();
        }
        if (scripted.suspend) {
            co_await nextEventLoopTurn();
        }
        if (scripted.error) {
            throw *scripted.error;
        }
        co_return scripted.streams;
    }
};

class FakeCinemetaClient : public api::CinemetaClient
{
public:
    struct MetaScript {
        api::MetaDetail detail;
        std::optional<core::HttpError> error;
        bool suspend = false;
    };

    struct SeriesScript {
        api::SeriesDetail detail;
        std::optional<core::HttpError> error;
        bool suspend = false;
    };

    explicit FakeCinemetaClient(QObject* parent = nullptr)
        : CinemetaClient(nullptr, parent)
    {
    }

    QList<MetaScript> metaScripts;
    QList<SeriesScript> seriesScripts;
    int metaCalls = 0;
    int seriesCalls = 0;
    api::MediaKind lastMetaKind = api::MediaKind::Movie;
    QString lastMetaImdbId;
    QString lastSeriesImdbId;

    QCoro::Task<api::MetaDetail> meta(api::MediaKind kind, QString imdbId) override
    {
        ++metaCalls;
        lastMetaKind = kind;
        lastMetaImdbId = imdbId;

        MetaScript scripted;
        if (!metaScripts.isEmpty()) {
            scripted = metaScripts.takeFirst();
        }
        if (scripted.suspend) {
            co_await nextEventLoopTurn();
        }
        if (scripted.error) {
            throw *scripted.error;
        }
        co_return scripted.detail;
    }

    QCoro::Task<api::SeriesDetail> seriesMeta(QString imdbId) override
    {
        ++seriesCalls;
        lastSeriesImdbId = imdbId;

        SeriesScript scripted;
        if (!seriesScripts.isEmpty()) {
            scripted = seriesScripts.takeFirst();
        }
        if (scripted.suspend) {
            co_await nextEventLoopTurn();
        }
        if (scripted.error) {
            throw *scripted.error;
        }
        co_return scripted.detail;
    }
};

class FakeTokenStore : public core::TokenStore
{
public:
    explicit FakeTokenStore(QObject* parent = nullptr)
        : TokenStore(parent)
    {
    }

    QHash<QString, QString> values;
    QSet<QString> failingReads;
    QSet<QString> failingWrites;
    QSet<QString> failingRemoves;
    QString failureMessage { QStringLiteral("token-store failure") };
    QStringList readKeys;
    QStringList writtenKeys;
    QStringList removedKeys;

    QCoro::Task<QString> read(QString key) override
    {
        readKeys.append(key);
        if (failingReads.contains(key)) {
            throw core::TokenStoreError(failureMessage);
        }
        co_return values.value(key);
    }

    QCoro::Task<void> write(QString key, QString value) override
    {
        writtenKeys.append(key);
        if (failingWrites.contains(key)) {
            throw core::TokenStoreError(failureMessage);
        }
        values.insert(key, value);
        co_return;
    }

    QCoro::Task<void> remove(QString key) override
    {
        removedKeys.append(key);
        if (failingRemoves.contains(key)) {
            throw core::TokenStoreError(failureMessage);
        }
        values.remove(key);
        co_return;
    }
};

} // namespace kinema::tests
