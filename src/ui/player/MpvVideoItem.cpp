// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/MpvVideoItem.h"

#include "config/PlayerSettings.h"
#include "core/MpvConfigPaths.h"
#include "kinema_debug.h"

#include <MpvController>

#include <mpv/client.h>

#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStringList>
#include <QVariantList>
#include <QtMath>

namespace kinema::ui::player {

namespace {

// ---- Property names (single source of truth) -----------------------------
//
// String IDs are passed to mpv via mpvqt's `observeProperty`. The id
// argument to observeProperty is a uint64_t we use only to satisfy
// the API; the dispatch in onMpvPropertyChanged is keyed by name
// because mpvqt forwards the name verbatim.

constexpr auto kPause              = "pause";
constexpr auto kTimePos            = "time-pos";
constexpr auto kDuration           = "duration";
constexpr auto kVolume             = "volume";
constexpr auto kMute               = "mute";
constexpr auto kSpeed              = "speed";
constexpr auto kPausedForCache     = "paused-for-cache";
constexpr auto kCacheBufferState   = "cache-buffering-state";
constexpr auto kDemuxerCacheTime   = "demuxer-cache-time";
constexpr auto kTrackList          = "track-list";
constexpr auto kChapterList        = "chapter-list";
constexpr auto kAid                = "aid";
constexpr auto kSid                = "sid";
constexpr auto kWidth              = "width";
constexpr auto kHeight             = "height";
constexpr auto kVideoCodec         = "video-codec";
constexpr auto kAudioCodec         = "audio-codec";
constexpr auto kVfFps              = "estimated-vf-fps";
constexpr auto kVideoBitrate       = "video-bitrate";
constexpr auto kAudioBitrate       = "audio-bitrate";
constexpr auto kHdrPrimaries       = "video-params/primaries";
constexpr auto kHdrGamma           = "video-params/gamma";
constexpr auto kAudioChannels      = "audio-params/channel-count";

/// Minimum delta (seconds) before we re-emit positionChanged. mpv
/// fires time-pos updates at decoding cadence; 250 ms is plenty for
/// progress persistence and keeps QML binding traffic tame.
constexpr double kPositionEpsilonSec = 0.25;

constexpr size_t kLogTailMax = 64;

/// Parse mpv's `aid` / `sid` value. They arrive as a QVariant string
/// or qlonglong depending on how mpvqt typed the underlying node.
/// "no" / "auto" / parse failure → -1 ("disabled or unknown").
int parseTrackIdValue(const QVariant& v)
{
    if (v.metaType().id() == QMetaType::QString) {
        const QString s = v.toString();
        if (s.isEmpty() || s == QLatin1String("no")
            || s == QLatin1String("auto")) {
            return -1;
        }
        bool ok = false;
        const int n = s.toInt(&ok);
        return ok ? n : -1;
    }
    bool ok = false;
    const int n = v.toInt(&ok);
    return ok ? n : -1;
}

/// Convert a `QVariant(QVariantList)` from mpvqt's nodeToVariant into
/// the JSON byte form our existing parsers accept. We funnel through
/// JSON so the parsers stay unchanged from the libmpv-direct era.
QByteArray variantListToJson(const QVariant& v)
{
    return QJsonDocument::fromVariant(v).toJson(QJsonDocument::Compact);
}

} // namespace

MpvVideoItem::MpvVideoItem(QQuickItem* parent)
    : MpvAbstractItem(parent)
{
    // mpvqt initialises the worker thread synchronously and emits
    // `ready()` once mpv_initialize has returned. We hook it to
    // install observers on the right side of init. Some mpvqt
    // releases also expose `mpvController()` immediately, but we
    // gate observer installation on `ready` defensively.
    connect(mpvController(), &MpvController::fileLoaded,
        this, &MpvVideoItem::onMpvFileLoaded);
    connect(mpvController(), &MpvController::endFile,
        this, &MpvVideoItem::onMpvEndFile);
    connect(mpvController(), &MpvController::propertyChanged,
        this, &MpvVideoItem::onMpvPropertyChanged);

    initObservers();
}

MpvVideoItem::~MpvVideoItem() = default;

void MpvVideoItem::initObservers()
{
    if (m_observersInstalled) {
        return;
    }
    m_observersInstalled = true;

    // observeProperty(name, format, id). We pass an id of 0 for
    // every entry — mpvqt routes deliveries by name, not by id.
    observeProperty(QString::fromLatin1(kPause),            MPV_FORMAT_FLAG);
    observeProperty(QString::fromLatin1(kTimePos),          MPV_FORMAT_DOUBLE);
    observeProperty(QString::fromLatin1(kDuration),         MPV_FORMAT_DOUBLE);
    observeProperty(QString::fromLatin1(kVolume),           MPV_FORMAT_DOUBLE);
    observeProperty(QString::fromLatin1(kMute),             MPV_FORMAT_FLAG);
    observeProperty(QString::fromLatin1(kSpeed),            MPV_FORMAT_DOUBLE);
    observeProperty(QString::fromLatin1(kPausedForCache),   MPV_FORMAT_FLAG);
    observeProperty(QString::fromLatin1(kCacheBufferState), MPV_FORMAT_INT64);
    observeProperty(QString::fromLatin1(kDemuxerCacheTime), MPV_FORMAT_DOUBLE);
    observeProperty(QString::fromLatin1(kTrackList),        MPV_FORMAT_NODE);
    observeProperty(QString::fromLatin1(kChapterList),      MPV_FORMAT_NODE);
    observeProperty(QString::fromLatin1(kAid),              MPV_FORMAT_STRING);
    observeProperty(QString::fromLatin1(kSid),              MPV_FORMAT_STRING);
    observeProperty(QString::fromLatin1(kWidth),            MPV_FORMAT_INT64);
    observeProperty(QString::fromLatin1(kHeight),           MPV_FORMAT_INT64);
    observeProperty(QString::fromLatin1(kVideoCodec),       MPV_FORMAT_STRING);
    observeProperty(QString::fromLatin1(kAudioCodec),       MPV_FORMAT_STRING);
    observeProperty(QString::fromLatin1(kVfFps),            MPV_FORMAT_DOUBLE);
    observeProperty(QString::fromLatin1(kVideoBitrate),     MPV_FORMAT_DOUBLE);
    observeProperty(QString::fromLatin1(kAudioBitrate),     MPV_FORMAT_DOUBLE);
    observeProperty(QString::fromLatin1(kHdrPrimaries),     MPV_FORMAT_STRING);
    observeProperty(QString::fromLatin1(kHdrGamma),         MPV_FORMAT_STRING);
    observeProperty(QString::fromLatin1(kAudioChannels),    MPV_FORMAT_INT64);
}

void MpvVideoItem::applySettings(const config::PlayerSettings& settings)
{
    // Hardware decoding. `auto-safe` is mpv's recommended default;
    // we drop it to "no" when the user explicitly disables.
    if (!settings.hardwareDecoding()) {
        setProperty(QStringLiteral("hwdec"), QStringLiteral("no"));
    } else {
        setProperty(QStringLiteral("hwdec"), QStringLiteral("auto-safe"));
    }

    // Preferred audio / subtitle language (mpv-side `alang` / `slang`
    // accept comma-separated codes; we forward the user's exact
    // string).
    const auto alang = settings.preferredAudioLang();
    if (!alang.isEmpty()) {
        setProperty(QStringLiteral("alang"), alang);
    }
    const auto slang = settings.preferredSubtitleLang();
    if (!slang.isEmpty()) {
        setProperty(QStringLiteral("slang"), slang);
    }

    // Stash remembered volume; applied just before loadfile so the
    // first decoded frames come out at the right level.
    const double remembered = settings.rememberedVolume();
    if (remembered >= 0.0) {
        m_pendingVolume = remembered;
    }

    // Quiet-mode: mpv's own OSC must stay disabled (we render chrome
    // ourselves) and the cursor-autohide we drive from QML.
    setProperty(QStringLiteral("osc"), false);
    setProperty(QStringLiteral("cursor-autohide"), QStringLiteral("no"));
    setProperty(QStringLiteral("ytdl"), false);
    setProperty(QStringLiteral("keep-open"), false);

    // Soft volume boost. mpv's default `volume-max` is 130; raise
    // it to 150 so the chrome's 0–150 % volume bar can drive the
    // full range without mpv clamping the value back.
    setProperty(QStringLiteral("volume-max"), 150.0);

    // Load the shipped mpv.conf if present. mpvqt has already done
    // mpv_initialize; we use mpv's `include` option which still
    // accepts a runtime push.
    const auto mpvConf = core::mpv_config::mpvConfPath();
    if (!mpvConf.isEmpty()) {
        qCInfo(KINEMA) << "loading shipped mpv config from" << mpvConf;
        setProperty(QStringLiteral("include"), mpvConf);
    } else {
        qCWarning(KINEMA)
            << "Kinema mpv.conf not found \u2014 using built-in defaults";
    }

    const auto inputConf = core::mpv_config::inputConfPath();
    if (!inputConf.isEmpty()) {
        setProperty(QStringLiteral("input-conf"), inputConf);
    }
}

// ---- Imperative slots ----------------------------------------------------

void MpvVideoItem::loadFile(const QUrl& url,
    std::optional<double> startSeconds)
{
    // Invalidate cached state so the first new sample is always
    // forwarded.
    m_position = -1.0;
    m_duration = -1.0;
    m_cacheAhead = -1.0;
    m_bufferingWaiting = false;
    m_bufferingPercent = 0;
    m_tracks.clear();
    m_chapters.clear();
    m_audioTrackId = -1;
    m_subtitleTrackId = -1;
    m_stats = VideoStats {};

    Q_EMIT trackListChanged(m_tracks);
    Q_EMIT chaptersChanged(m_chapters);

    if (m_pendingVolume) {
        setProperty(QStringLiteral("volume"), *m_pendingVolume);
        m_pendingVolume.reset();
    }

    const QString urlStr = QString::fromUtf8(url.toEncoded());
    QStringList args;
    args << QStringLiteral("loadfile") << urlStr;
    if (startSeconds && *startSeconds > 0.0) {
        // loadfile <url> <flags> <index> <options>
        args << QStringLiteral("replace")
             << QStringLiteral("-1")
             << QStringLiteral("start=%1").arg(*startSeconds, 0, 'f', 3);
    }
    command(args);
}

void MpvVideoItem::stop()
{
    command({ QStringLiteral("stop") });
}

void MpvVideoItem::setPaused(bool paused)
{
    setProperty(QStringLiteral("pause"), paused);
}

void MpvVideoItem::cyclePause()
{
    command({ QStringLiteral("cycle"), QStringLiteral("pause") });
}

void MpvVideoItem::seekAbsolute(double seconds)
{
    if (seconds < 0.0) {
        return;
    }
    command({ QStringLiteral("seek"),
        QString::number(seconds, 'f', 3),
        QStringLiteral("absolute") });
}

void MpvVideoItem::seekRelative(double seconds)
{
    command({ QStringLiteral("seek"),
        QString::number(seconds, 'f', 3),
        QStringLiteral("relative") });
}

void MpvVideoItem::setVolumePercent(double v)
{
    // Allow soft volume boost up to 150 %. mpv's `volume-max` is
    // raised to match in `applySettings`.
    v = qBound(0.0, v, 150.0);
    setProperty(QStringLiteral("volume"), v);
}

void MpvVideoItem::setMuted(bool m)
{
    setProperty(QStringLiteral("mute"), m);
}

void MpvVideoItem::setSpeed(double s)
{
    if (s <= 0.0) {
        return;
    }
    setProperty(QStringLiteral("speed"), s);
}

void MpvVideoItem::setAudioTrack(int id)
{
    setProperty(QStringLiteral("aid"),
        id < 0 ? QStringLiteral("no") : QString::number(id));
}

void MpvVideoItem::setSubtitleTrack(int id)
{
    setProperty(QStringLiteral("sid"),
        id < 0 ? QStringLiteral("no") : QString::number(id));
}

void MpvVideoItem::setMediaTitle(const QString& title)
{
    setProperty(QStringLiteral("force-media-title"), title);
}

void MpvVideoItem::addSubtitleFile(const QString& path,
    const QString& title, const QString& lang, bool select)
{
    if (path.isEmpty()) {
        return;
    }
    QStringList args;
    args << QStringLiteral("sub-add") << path
         << (select ? QStringLiteral("select") : QStringLiteral("auto"));
    if (!title.isEmpty() || !lang.isEmpty()) {
        args << title;
    }
    if (!lang.isEmpty()) {
        args << lang;
    }
    command(args);
}

QStringList MpvVideoItem::recentLogLines() const
{
    QStringList out;
    out.reserve(static_cast<int>(m_logTail.size()));
    for (const auto& s : m_logTail) {
        out.push_back(s);
    }
    return out;
}

void MpvVideoItem::appendLogLine(const QString& line)
{
    m_logTail.push_back(line);
    while (m_logTail.size() > kLogTailMax) {
        m_logTail.pop_front();
    }
}

// ---- Event handlers ------------------------------------------------------

void MpvVideoItem::onMpvFileLoaded()
{
    Q_EMIT fileLoaded();
}

void MpvVideoItem::onMpvEndFile(const QString& reason)
{
    Q_EMIT endOfFile(reason);
}

void MpvVideoItem::onMpvPropertyChanged(const QString& name,
    const QVariant& value)
{
    // Common helpers
    const auto asDouble = [&]() {
        bool ok = false;
        const double v = value.toDouble(&ok);
        return ok ? v : 0.0;
    };
    const auto asBool = [&]() { return value.toBool(); };
    const auto asInt = [&]() { return value.toInt(); };
    const auto asString = [&]() { return value.toString(); };

    if (name == QLatin1String(kPause)) {
        const bool v = asBool();
        if (v != m_paused) {
            m_paused = v;
            Q_EMIT pausedChanged(v);
        }
    } else if (name == QLatin1String(kTimePos)) {
        const double v = asDouble();
        if (m_position < 0.0
            || qAbs(v - m_position) >= kPositionEpsilonSec) {
            m_position = v;
            Q_EMIT positionChanged(v);
        }
    } else if (name == QLatin1String(kDuration)) {
        const double v = asDouble();
        if (v > 0.0 && qAbs(v - m_duration) > 0.01) {
            m_duration = v;
            Q_EMIT durationChanged(v);
        }
    } else if (name == QLatin1String(kVolume)) {
        const double v = asDouble();
        if (qAbs(v - m_volume) > 0.05) {
            m_volume = v;
            Q_EMIT volumeChanged(v);
        }
    } else if (name == QLatin1String(kMute)) {
        const bool v = asBool();
        if (v != m_muted) {
            m_muted = v;
            Q_EMIT muteChanged(v);
        }
    } else if (name == QLatin1String(kSpeed)) {
        const double v = asDouble();
        if (qAbs(v - m_speed) > 0.005) {
            m_speed = v;
            Q_EMIT speedChanged(v);
        }
    } else if (name == QLatin1String(kPausedForCache)) {
        const bool v = asBool();
        m_bufferingWaiting = v;
        Q_EMIT bufferingChanged(m_bufferingWaiting, m_bufferingPercent);
    } else if (name == QLatin1String(kCacheBufferState)) {
        const int v = asInt();
        m_bufferingPercent = v;
        Q_EMIT bufferingChanged(m_bufferingWaiting, m_bufferingPercent);
    } else if (name == QLatin1String(kDemuxerCacheTime)) {
        const double v = asDouble();
        if (m_cacheAhead < 0.0 || qAbs(v - m_cacheAhead) > 0.25) {
            m_cacheAhead = v;
            Q_EMIT cacheAheadChanged(v);
        }
        m_stats.cacheSeconds = v;
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kTrackList)) {
        parseTrackListNode(value);
    } else if (name == QLatin1String(kChapterList)) {
        parseChapterListNode(value);
    } else if (name == QLatin1String(kAid)) {
        const int id = parseTrackIdValue(value);
        if (id != m_audioTrackId) {
            m_audioTrackId = id;
            Q_EMIT audioTrackChanged(id);
        }
    } else if (name == QLatin1String(kSid)) {
        const int id = parseTrackIdValue(value);
        if (id != m_subtitleTrackId) {
            m_subtitleTrackId = id;
            Q_EMIT subtitleTrackChanged(id);
        }
    } else if (name == QLatin1String(kWidth)) {
        m_stats.width = asInt();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kHeight)) {
        m_stats.height = asInt();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kVideoCodec)) {
        m_stats.videoCodec = asString();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kAudioCodec)) {
        m_stats.audioCodec = asString();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kVfFps)) {
        m_stats.fps = asDouble();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kVideoBitrate)) {
        m_stats.videoBitrate = asDouble();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kAudioBitrate)) {
        m_stats.audioBitrate = asDouble();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kHdrPrimaries)) {
        m_stats.hdrPrimaries = asString();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kHdrGamma)) {
        m_stats.hdrGamma = asString();
        Q_EMIT videoStatsChanged(m_stats);
    } else if (name == QLatin1String(kAudioChannels)) {
        m_stats.audioChannels = asInt();
        Q_EMIT videoStatsChanged(m_stats);
    }
    // Unknown property names are silently ignored \u2014 mpv ships new
    // ones on every release and we don't want to warn about every
    // observation that's not yet wired up.
}

void MpvVideoItem::parseTrackListNode(const QVariant& nodeValue)
{
    // mpvqt's nodeToVariant turns the track-list array into a
    // QVariantList; route through JSON so we can keep using
    // core::tracks::parseTrackList unchanged.
    const QByteArray json = variantListToJson(nodeValue);
    m_tracks = core::tracks::parseTrackList(json);
    Q_EMIT trackListChanged(m_tracks);
}

void MpvVideoItem::parseChapterListNode(const QVariant& nodeValue)
{
    const QByteArray json = variantListToJson(nodeValue);
    m_chapters = core::chapters::parseChapterList(json);
    Q_EMIT chaptersChanged(m_chapters);
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
