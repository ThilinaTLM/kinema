// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/subtitles/MoviehashProbe.h"

#include "core/io/HttpClient.h"
#include "core/io/UrlRedactor.h"
#include "core/util/Moviehash.h"
#include "playback/events/PlaybackEventStream.h"
#include "kinema_log_controller.h"

#include <QByteArray>
#include <QNetworkRequest>

#include <variant>

namespace kinema::playback::subtitles {

namespace {

constexpr qint64 kBlock = 65536;

} // namespace

MoviehashProbe::MoviehashProbe(events::PlaybackEventStream& events,
    core::HttpClient* http, QObject* parent)
    : QObject(parent)
    , m_events(events)
    , m_http(http)
{
    connect(&m_events, &events::PlaybackEventStream::eventPublished,
        this, &MoviehashProbe::onEvent);
}

MoviehashProbe::~MoviehashProbe() = default;

void MoviehashProbe::onEvent(const events::PlaybackEvent& event)
{
    std::visit([this](const auto& payload) {
        using T = std::decay_t<decltype(payload)>;
        if constexpr (std::is_same_v<T, events::PlayableUrlReady>) {
            onPlayableUrl(payload);
        }
    }, event);
}

void MoviehashProbe::onPlayableUrl(
    const events::PlayableUrlReady& payload)
{
    if (!m_http) {
        return;
    }
    if (payload.url.scheme() != QLatin1String("https")) {
        // Local gateway URLs are http://127.0.0.1/...; OpenSubtitles
        // hash matching only meaningfully works against the
        // upstream HTTPS URL. core::HttpClient enforces HTTPS and
        // would throw on every probe otherwise.
        qCDebug(KINEMA_CONTROLLER)
            << "moviehash: skipping non-HTTPS URL"
            << core::redactUrlForLog(payload.url);
        return;
    }
    const auto epoch = ++m_epoch;
    auto task = kickoff(payload.sessionId, payload.url, epoch);
    Q_UNUSED(task);
}

QCoro::Task<void> MoviehashProbe::kickoff(
    PlaybackSessionId sessionId, QUrl url, quint64 epoch)
{
    if (!m_http) {
        co_return;
    }

    const auto rangeGet = [this, &url](qint64 start, qint64 end)
        -> QCoro::Task<QByteArray> {
        QNetworkRequest req(url);
        req.setRawHeader("Range",
            QByteArrayLiteral("bytes=")
                + QByteArray::number(start)
                + "-"
                + QByteArray::number(end));
        co_return co_await m_http->get(req);
    };

    qint64 size = 0;
    try {
        const auto headers = co_await m_http->head(QNetworkRequest(url));
        for (const auto& h : headers) {
            if (h.first.compare("Content-Length", Qt::CaseInsensitive) == 0) {
                bool ok = false;
                size = h.second.toLongLong(&ok);
                if (!ok) {
                    size = 0;
                }
                break;
            }
        }
    } catch (const std::exception& e) {
        qCDebug(KINEMA_CONTROLLER) << "moviehash: HEAD failed:" << e.what();
        co_return;
    }
    if (epoch != m_epoch) {
        co_return;
    }
    if (size <= 2 * kBlock) {
        qCDebug(KINEMA_CONTROLLER)
            << "moviehash: Content-Length too small or absent";
        co_return;
    }

    QByteArray head;
    QByteArray tail;
    try {
        head = co_await rangeGet(0, kBlock - 1);
    } catch (const std::exception& e) {
        qCDebug(KINEMA_CONTROLLER)
            << "moviehash: head Range GET failed:" << e.what();
        co_return;
    }
    if (epoch != m_epoch || head.size() != kBlock) {
        co_return;
    }
    try {
        tail = co_await rangeGet(size - kBlock, size - 1);
    } catch (const std::exception& e) {
        qCDebug(KINEMA_CONTROLLER)
            << "moviehash: tail Range GET failed:" << e.what();
        co_return;
    }
    if (epoch != m_epoch || tail.size() != kBlock) {
        co_return;
    }

    const QString hex = core::moviehash::compute(head, tail, size);
    if (hex.isEmpty() || epoch != m_epoch) {
        co_return;
    }
    qCDebug(KINEMA_CONTROLLER) << "moviehash: computed" << hex << "for"
                    << core::redactUrlForLog(url);
    m_events.publish(events::MoviehashComputed { sessionId, hex });
}

} // namespace kinema::playback::subtitles
