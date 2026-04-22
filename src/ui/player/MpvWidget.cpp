// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/MpvWidget.h"

#include "config/PlayerSettings.h"
#include "core/MpvConfigPaths.h"
#include "kinema_debug.h"

#include <mpv/client.h>
#include <mpv/render.h>
#include <mpv/render_gl.h>

#include <QByteArray>
#include <QFileInfo>
#include <QKeyEvent>
#include <QMetaObject>
#include <QMouseEvent>
#include <QOpenGLContext>
#include <QWheelEvent>

#include <cstring>

namespace kinema::ui::player {

namespace {

// ---- libmpv callbacks ----------------------------------------------------
//
// These fire from arbitrary threads. We must NOT touch Qt state or call
// further mpv APIs here — the only safe thing is to bounce back onto
// the GUI thread via QMetaObject::invokeMethod(..., Qt::QueuedConnection).

void onMpvWakeup(void* ctx)
{
    auto* w = static_cast<MpvWidget*>(ctx);
    QMetaObject::invokeMethod(w, "onMpvEvents", Qt::QueuedConnection);
}

void onRenderUpdate(void* ctx)
{
    auto* w = static_cast<MpvWidget*>(ctx);
    QMetaObject::invokeMethod(w, "onRenderUpdate", Qt::QueuedConnection);
}

void* getProcAddress(void* /*ctx*/, const char* name)
{
    auto* glCtx = QOpenGLContext::currentContext();
    if (!glCtx) {
        return nullptr;
    }
    return reinterpret_cast<void*>(glCtx->getProcAddress(QByteArray(name)));
}

// Map a Qt key to an mpv key name understood by input.conf and
// mpv_command(["keydown"/"keyup", name]). Returns an empty string for
// keys we don't forward (modifiers on their own, etc.) — those are
// handled implicitly via the modifier prefixes on the main key.
QByteArray qtKeyToMpv(int key)
{
    switch (key) {
    case Qt::Key_Space:     return QByteArrayLiteral("SPACE");
    case Qt::Key_Return:
    case Qt::Key_Enter:     return QByteArrayLiteral("ENTER");
    case Qt::Key_Backspace: return QByteArrayLiteral("BS");
    case Qt::Key_Tab:       return QByteArrayLiteral("TAB");
    case Qt::Key_Left:      return QByteArrayLiteral("LEFT");
    case Qt::Key_Right:     return QByteArrayLiteral("RIGHT");
    case Qt::Key_Up:        return QByteArrayLiteral("UP");
    case Qt::Key_Down:      return QByteArrayLiteral("DOWN");
    case Qt::Key_PageUp:    return QByteArrayLiteral("PGUP");
    case Qt::Key_PageDown:  return QByteArrayLiteral("PGDWN");
    case Qt::Key_Home:      return QByteArrayLiteral("HOME");
    case Qt::Key_End:       return QByteArrayLiteral("END");
    case Qt::Key_Insert:    return QByteArrayLiteral("INS");
    case Qt::Key_Delete:    return QByteArrayLiteral("DEL");
    case Qt::Key_Escape:    return QByteArrayLiteral("ESC");
    case Qt::Key_Print:     return QByteArrayLiteral("PRINT");
    default:
        if (key >= Qt::Key_F1 && key <= Qt::Key_F24) {
            return QByteArrayLiteral("F") + QByteArray::number(key - Qt::Key_F1 + 1);
        }
        // Printable ASCII — mpv takes the literal character ("a", "1",
        // "/"). Uppercase letters and shift-variants are handled via
        // modifier prefixes; we always lowercase here so the `shift+`
        // prefix applies cleanly.
        if (key >= 0x20 && key <= 0x7E) {
            const char c = static_cast<char>(key < 0x7F ? key : 0);
            if (c >= 'A' && c <= 'Z') {
                return QByteArray(1, static_cast<char>(c + ('a' - 'A')));
            }
            return QByteArray(1, c);
        }
        return {};
    }
}

QByteArray mpvKeyWithModifiers(QKeyEvent* e)
{
    auto name = qtKeyToMpv(e->key());
    if (name.isEmpty()) {
        return {};
    }
    const auto mods = e->modifiers();
    QByteArray out;
    if (mods & Qt::ControlModifier) {
        out += "ctrl+";
    }
    if (mods & Qt::AltModifier) {
        out += "alt+";
    }
    if (mods & Qt::MetaModifier) {
        out += "meta+";
    }
    if ((mods & Qt::ShiftModifier) && name.size() > 1) {
        // For named keys ("LEFT", "F5") mpv expects an explicit
        // shift+ prefix. For single letters the shifted variant is
        // already the uppercase character; Qt has already applied
        // shift to the key code.
        out += "shift+";
    }
    out += name;
    return out;
}

QByteArray mpvMouseButton(Qt::MouseButton b)
{
    switch (b) {
    case Qt::LeftButton:   return QByteArrayLiteral("MBTN_LEFT");
    case Qt::RightButton:  return QByteArrayLiteral("MBTN_RIGHT");
    case Qt::MiddleButton: return QByteArrayLiteral("MBTN_MID");
    case Qt::BackButton:   return QByteArrayLiteral("MBTN_BACK");
    case Qt::ForwardButton:return QByteArrayLiteral("MBTN_FORWARD");
    default:               return {};
    }
}

// Property-change observer userdata tags.
constexpr uint64_t kPropPause = 1;
constexpr uint64_t kPropFullscreen = 2;
constexpr uint64_t kPropTimePos = 3;
constexpr uint64_t kPropDuration = 4;
constexpr uint64_t kPropVolume = 5;
constexpr uint64_t kPropMute = 6;
constexpr uint64_t kPropSpeed = 7;
constexpr uint64_t kPropPausedForCache = 8;
constexpr uint64_t kPropCacheBufferingState = 9;
constexpr uint64_t kPropDemuxerCacheTime = 10;
constexpr uint64_t kPropTrackList = 11;
constexpr uint64_t kPropChapterList = 12;
constexpr uint64_t kPropAid = 13;
constexpr uint64_t kPropSid = 14;
constexpr uint64_t kPropWidth = 15;
constexpr uint64_t kPropHeight = 16;
constexpr uint64_t kPropVideoCodec = 17;
constexpr uint64_t kPropAudioCodec = 18;
constexpr uint64_t kPropVfFps = 19;
constexpr uint64_t kPropVideoBitrate = 20;
constexpr uint64_t kPropAudioBitrate = 21;

int parseTrackIdString(const char* s)
{
    // mpv reports aid/sid as a string so it can encode "no" / "auto"
    // alongside concrete integers. Anything not numeric maps to -1
    // ("off") — that includes the auto-selection placeholder, which
    // the UI doesn't distinguish from off.
    if (!s || *s == 0) {
        return -1;
    }
    const QString v = QString::fromUtf8(s);
    bool ok = false;
    const int n = v.toInt(&ok);
    return ok ? n : -1;
}

/// Minimum delta (seconds) before we emit positionChanged. mpv fires
/// time-pos updates at decoding cadence; 250 ms granularity is plenty
/// for progress persistence and keeps signal traffic tame.
constexpr double kPositionEpsilonSec = 0.25;

/// Bound on the log-line ring buffer. 64 lines covers the typical
/// failure envelope (URL open, HTTP probe, demuxer retry) comfortably.
constexpr size_t kLogTailMax = 64;

} // namespace

MpvWidget::MpvWidget(const config::PlayerSettings& settings, QWidget* parent)
    : QOpenGLWidget(parent)
    , m_settings(settings)
{
    setFocusPolicy(Qt::StrongFocus);
    setMouseTracking(true);
    setAttribute(Qt::WA_OpaquePaintEvent);

    m_mpv = mpv_create();
    if (!m_mpv) {
        qCWarning(KINEMA) << "mpv_create failed";
        return;
    }

    // The primary libmpv handle is always named "main" — there is no
    // `client-name` option and `mpv_client_name()` is read-only. The
    // Lua chrome targets us back with `script-message-to main …`;
    // the host targets the script by *its* name
    // ("kinema-overlays") via `MpvIpc::send`.

    // Kinema owns the mpv config entirely. User ~/.config/mpv is
    // never consulted — we ship our own mpv.conf / input.conf and
    // translate Kinema-level preferences into mpv options below.
    // For a custom mpv experience users should pick the external mpv
    // backend in Settings.
    mpv_set_option_string(m_mpv, "config", "no");
    mpv_set_option_string(m_mpv, "terminal", "no");
    mpv_set_option_string(m_mpv, "msg-level", "all=no,cplayer=info");
    mpv_set_option_string(m_mpv, "input-default-bindings", "no");
    mpv_set_option_string(m_mpv, "input-vo-keyboard", "yes");

    const auto mpvConf = core::mpv_config::mpvConfPath();
    if (!mpvConf.isEmpty()) {
        qCInfo(KINEMA) << "loading shipped mpv config from" << mpvConf;
        mpv_load_config_file(m_mpv, mpvConf.toLocal8Bit().constData());
    } else {
        // Packaging bug or running from a bare tree without an
        // install. Keep the embedded player usable with a handful of
        // safe defaults; log once so the failure is traceable.
        qCWarning(KINEMA)
            << "Kinema mpv.conf not found — falling back to built-in defaults";
        mpv_set_option_string(m_mpv, "vo", "libmpv");
        mpv_set_option_string(m_mpv, "hwdec", "auto-safe");
        mpv_set_option_string(m_mpv, "osc", "no");
        mpv_set_option_string(m_mpv, "osd-bar", "yes");
        mpv_set_option_string(m_mpv, "cursor-autohide", "no");
        mpv_set_option_string(m_mpv, "ytdl", "no");
        mpv_set_option_string(m_mpv, "keep-open", "no");
    }

    const auto inputConf = core::mpv_config::inputConfPath();
    if (!inputConf.isEmpty()) {
        mpv_set_option_string(m_mpv, "input-conf",
            inputConf.toLocal8Bit().constData());
    }

    // Point mpv at the bundled icon font so libass can resolve the
    // `Kinema Icons` family the overlay script references. With
    // `config=no` mpv does not scan `~/.config/mpv/fonts`, so the
    // default font provider would otherwise miss our shipped font
    // and fontconfig would fall back to some unrelated family whose
    // glyph coverage happens to include the PUA codepoints we use
    // (typically a CJK font — the chrome then renders as Han
    // characters instead of Material Symbols).
    //
    // mpv 0.38 renamed `sub-font-dir` to `sub-fonts-dir` (plural)
    // and split the OSD font pipeline out behind `osd-fonts-dir`.
    // The overlay script draws through `mp.set_osd_ass`, so the
    // OSD provider is the one that matters; we set both so the
    // shipped font is visible to subtitle rendering too.
    const auto fontsDir = core::mpv_config::dataDir()
        + QLatin1String("/fonts");
    if (QFileInfo(fontsDir).isDir()) {
        const auto fontsDirBytes = fontsDir.toLocal8Bit();
        mpv_set_option_string(m_mpv, "sub-fonts-dir",
            fontsDirBytes.constData());
        mpv_set_option_string(m_mpv, "osd-fonts-dir",
            fontsDirBytes.constData());
    } else {
        qCWarning(KINEMA)
            << "kinema fonts dir missing at" << fontsDir
            << "\u2014 chrome icons will render as tofu";
    }

    // Point mpv at the Kinema overlay script imperatively.
    //
    // The chrome lives in a **script directory** — mpv treats any
    // directory passed via `scripts` as a Lua script bundle and
    // loads `main.lua` inside. The directory basename becomes the
    // mpv-side script name (`kinema-overlays`), which is what the
    // host targets via `script-message-to` in `core/MpvIpc.cpp`.
    //
    // Splitting the chrome across several sibling files keeps each
    // module small; `main.lua` prepends its own directory to
    // `package.path` so `require 'theme'` / `require 'ass'` etc.
    // resolve on every supported mpv release.
    const auto overlayDir = core::mpv_config::dataDir()
        + QLatin1String("/scripts/kinema-overlays");
    if (QFileInfo(overlayDir).isDir()) {
        mpv_set_option_string(m_mpv, "scripts",
            overlayDir.toLocal8Bit().constData());
    } else {
        qCWarning(KINEMA) << "kinema-overlays script dir not found at"
                          << overlayDir
                          << "\u2014 in-mpv chrome will not render";
    }

    // Translate Kinema-level preferences into mpv options before
    // initialize(). Live-applying these requires re-creating the
    // context, so the settings page notes that the preference takes
    // effect on the next play.
    if (!m_settings.hardwareDecoding()) {
        mpv_set_option_string(m_mpv, "hwdec", "no");
    }
    const auto alang = m_settings.preferredAudioLang();
    if (!alang.isEmpty()) {
        mpv_set_option_string(m_mpv, "alang",
            alang.toLocal8Bit().constData());
    }
    const auto slang = m_settings.preferredSubtitleLang();
    if (!slang.isEmpty()) {
        mpv_set_option_string(m_mpv, "slang",
            slang.toLocal8Bit().constData());
    }

    mpv_request_log_messages(m_mpv, "info");

    if (mpv_initialize(m_mpv) < 0) {
        qCWarning(KINEMA) << "mpv_initialize failed";
        mpv_destroy(m_mpv);
        m_mpv = nullptr;
        return;
    }

    m_ipc = new core::MpvIpc(m_mpv, this);
    // CLIENT_MESSAGE is enabled by default; requesting it explicitly
    // keeps the IPC surface honest across future mpv releases that
    // may flip the default off.
    mpv_request_event(m_mpv, MPV_EVENT_CLIENT_MESSAGE, 1);

    mpv_observe_property(m_mpv, kPropPause, "pause", MPV_FORMAT_FLAG);
    mpv_observe_property(m_mpv, kPropFullscreen, "fullscreen", MPV_FORMAT_FLAG);
    mpv_observe_property(m_mpv, kPropTimePos, "time-pos", MPV_FORMAT_DOUBLE);
    mpv_observe_property(m_mpv, kPropDuration, "duration", MPV_FORMAT_DOUBLE);
    mpv_observe_property(m_mpv, kPropVolume, "volume", MPV_FORMAT_DOUBLE);
    mpv_observe_property(m_mpv, kPropMute, "mute", MPV_FORMAT_FLAG);
    mpv_observe_property(m_mpv, kPropSpeed, "speed", MPV_FORMAT_DOUBLE);
    mpv_observe_property(m_mpv, kPropPausedForCache, "paused-for-cache",
        MPV_FORMAT_FLAG);
    mpv_observe_property(m_mpv, kPropCacheBufferingState,
        "cache-buffering-state", MPV_FORMAT_INT64);
    mpv_observe_property(m_mpv, kPropDemuxerCacheTime,
        "demuxer-cache-time", MPV_FORMAT_DOUBLE);
    // Track / chapter lists as NODE: we only need the change event,
    // and re-fetch the serialised JSON via mpv_get_property_string
    // when the event fires. This keeps the parser in core/ free of
    // any mpv_node traversal.
    mpv_observe_property(m_mpv, kPropTrackList, "track-list",
        MPV_FORMAT_NODE);
    mpv_observe_property(m_mpv, kPropChapterList, "chapter-list",
        MPV_FORMAT_NODE);
    // aid / sid as string so we can distinguish "no" from integer ids.
    mpv_observe_property(m_mpv, kPropAid, "aid", MPV_FORMAT_STRING);
    mpv_observe_property(m_mpv, kPropSid, "sid", MPV_FORMAT_STRING);
    // Video stats. width / height as INT64, fps / bitrate as DOUBLE,
    // codecs as STRING.
    mpv_observe_property(m_mpv, kPropWidth, "width", MPV_FORMAT_INT64);
    mpv_observe_property(m_mpv, kPropHeight, "height", MPV_FORMAT_INT64);
    mpv_observe_property(m_mpv, kPropVideoCodec, "video-codec",
        MPV_FORMAT_STRING);
    mpv_observe_property(m_mpv, kPropAudioCodec, "audio-codec",
        MPV_FORMAT_STRING);
    mpv_observe_property(m_mpv, kPropVfFps, "estimated-vf-fps",
        MPV_FORMAT_DOUBLE);
    mpv_observe_property(m_mpv, kPropVideoBitrate, "video-bitrate",
        MPV_FORMAT_DOUBLE);
    mpv_observe_property(m_mpv, kPropAudioBitrate, "audio-bitrate",
        MPV_FORMAT_DOUBLE);

    mpv_set_wakeup_callback(m_mpv, &onMpvWakeup, this);
}

MpvWidget::~MpvWidget()
{
    // Destroy the IPC bridge before tearing down the mpv context so
    // no late `mpv_command_async` from a queued signal races
    // `mpv_terminate_destroy`. Relying on Qt parent ownership would
    // fire ~MpvIpc from ~QObject, which runs after our own dtor
    // body — too late to be safe.
    delete m_ipc;
    m_ipc = nullptr;

    // Render context must be freed with the GL context current; if we
    // still hold an mpv handle after that, destroy it cleanly.
    maybeTeardownRenderContext();
    if (m_mpv) {
        mpv_terminate_destroy(m_mpv);
        m_mpv = nullptr;
    }
}

void MpvWidget::maybeTeardownRenderContext()
{
    if (!m_renderCtx) {
        return;
    }
    // If our GL context is still alive, make it current so mpv can
    // drop its GL resources on the right context. If it's already
    // gone (widget destroyed late), the best we can do is call free
    // anyway — mpv is defensive about this.
    auto* glCtx = context();
    if (glCtx && glCtx->isValid()) {
        makeCurrent();
        mpv_render_context_free(m_renderCtx);
        doneCurrent();
    } else {
        mpv_render_context_free(m_renderCtx);
    }
    m_renderCtx = nullptr;
}

void MpvWidget::initializeGL()
{
    if (!m_mpv) {
        return;
    }

    mpv_opengl_init_params glInit {};
    glInit.get_proc_address = &getProcAddress;
    glInit.get_proc_address_ctx = nullptr;

    mpv_render_param params[] = {
        { MPV_RENDER_PARAM_API_TYPE,
          const_cast<char*>(MPV_RENDER_API_TYPE_OPENGL) },
        { MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &glInit },
        { MPV_RENDER_PARAM_INVALID, nullptr },
    };

    if (mpv_render_context_create(&m_renderCtx, m_mpv, params) < 0) {
        qCWarning(KINEMA) << "mpv_render_context_create failed";
        m_renderCtx = nullptr;
        Q_EMIT mpvError(QStringLiteral("Failed to initialise mpv renderer"));
        return;
    }

    mpv_render_context_set_update_callback(
        m_renderCtx, &::kinema::ui::player::onRenderUpdate, this);

    // Tear down mpv's GL resources before Qt drops the GL context on
    // us (window close, compositor reset, etc.).
    connect(context(), &QOpenGLContext::aboutToBeDestroyed,
        this, [this] { maybeTeardownRenderContext(); });
}

void MpvWidget::paintGL()
{
    if (!m_renderCtx) {
        return;
    }

    mpv_opengl_fbo fbo {};
    fbo.fbo = static_cast<int>(defaultFramebufferObject());
    fbo.w = width() * devicePixelRatioF();
    fbo.h = height() * devicePixelRatioF();
    fbo.internal_format = 0;

    int flipY = 1;
    mpv_render_param params[] = {
        { MPV_RENDER_PARAM_OPENGL_FBO, &fbo },
        { MPV_RENDER_PARAM_FLIP_Y, &flipY },
        { MPV_RENDER_PARAM_INVALID, nullptr },
    };

    mpv_render_context_render(m_renderCtx, params);
}

void MpvWidget::onRenderUpdate()
{
    if (!m_renderCtx) {
        return;
    }
    const uint64_t flags = mpv_render_context_update(m_renderCtx);
    if (flags & MPV_RENDER_UPDATE_FRAME) {
        update();
    }
}

void MpvWidget::onMpvEvents()
{
    if (!m_mpv) {
        return;
    }
    while (true) {
        mpv_event* ev = mpv_wait_event(m_mpv, 0);
        if (!ev || ev->event_id == MPV_EVENT_NONE) {
            break;
        }
        switch (ev->event_id) {
        case MPV_EVENT_SHUTDOWN:
            return;
        case MPV_EVENT_LOG_MESSAGE: {
            const auto* msg = static_cast<mpv_event_log_message*>(ev->data);
            if (!msg || !msg->text) {
                break;
            }
            // Known-benign noise: libmpv's render module emits
            // `OpenGL error INVALID_ENUM` at info level during its
            // texture-format probing on NVIDIA's proprietary driver
            // (the fallback path succeeds, which is why playback
            // works). Drop those specific info messages but keep
            // every other prefix and every higher level so real
            // render-pipeline errors still surface.
            const auto level = msg->log_level;
            const bool isRenderModule = msg->prefix
                && std::strcmp(msg->prefix, "libmpv_render") == 0;
            const auto text = QString::fromUtf8(msg->text).trimmed();
            if (isRenderModule && level >= MPV_LOG_LEVEL_INFO
                && text.contains(QLatin1String("OpenGL error"))) {
                break;
            }
            qCInfo(KINEMA).nospace() << "[mpv/" << msg->prefix << "] "
                                     << text;
            // Retain a short tail so PlaybackController can classify
            // an end-file error against the leading HTTP / network
            // messages mpv emitted just before the failure.
            appendLogLine(QString::fromLatin1(msg->prefix)
                + QLatin1String(": ") + text);
            break;
        }
        case MPV_EVENT_FILE_LOADED:
            Q_EMIT fileLoaded();
            break;
        case MPV_EVENT_CLIENT_MESSAGE:
            if (m_ipc) {
                m_ipc->deliver(
                    static_cast<mpv_event_client_message*>(ev->data));
            }
            break;
        case MPV_EVENT_END_FILE: {
            const auto* ef = static_cast<mpv_event_end_file*>(ev->data);
            QString reason = QStringLiteral("unknown");
            if (ef) {
                switch (ef->reason) {
                case MPV_END_FILE_REASON_EOF:    reason = QStringLiteral("eof"); break;
                case MPV_END_FILE_REASON_STOP:   reason = QStringLiteral("stop"); break;
                case MPV_END_FILE_REASON_QUIT:   reason = QStringLiteral("quit"); break;
                case MPV_END_FILE_REASON_ERROR:  reason = QStringLiteral("error"); break;
                case MPV_END_FILE_REASON_REDIRECT: reason = QStringLiteral("redirect"); break;
                }
            }
            Q_EMIT endOfFile(reason);
            break;
        }
        case MPV_EVENT_PROPERTY_CHANGE: {
            const auto* p = static_cast<mpv_event_property*>(ev->data);
            if (!p) {
                break;
            }
            // NODE-format notifications arrive with `p->data` pointing
            // at an mpv_node; we don't read the node here — we
            // re-fetch via mpv_get_property_string for the pure
            // parser. Handle that before the generic data-null guard.
            if (p->format == MPV_FORMAT_NODE) {
                if (ev->reply_userdata == kPropTrackList) {
                    refreshTrackList();
                } else if (ev->reply_userdata == kPropChapterList) {
                    refreshChapterList();
                }
                break;
            }
            // STRING-format notifications deliver a char** that may
            // legitimately hold a null payload when the underlying
            // property has no value yet (e.g. aid before a file
            // loads). Tolerate that case per-tag below rather than
            // bailing out wholesale.
            if (!p->data && p->format != MPV_FORMAT_STRING) {
                break;
            }
            if (p->format == MPV_FORMAT_FLAG) {
                const bool on = *static_cast<int*>(p->data) != 0;
                switch (ev->reply_userdata) {
                case kPropPause:
                    m_paused = on;
                    Q_EMIT pausedChanged(on);
                    break;
                case kPropFullscreen:
                    m_fullscreen = on;
                    if (m_suppressFullscreenEcho) {
                        m_suppressFullscreenEcho = false;
                    } else {
                        Q_EMIT fullscreenChanged(on);
                    }
                    break;
                case kPropMute:
                    m_muted = on;
                    Q_EMIT muteChanged(on);
                    break;
                case kPropPausedForCache:
                    m_bufferingWaiting = on;
                    Q_EMIT bufferingChanged(
                        m_bufferingWaiting, m_bufferingPct);
                    break;
                default:
                    break;
                }
            } else if (p->format == MPV_FORMAT_DOUBLE) {
                const double v = *static_cast<double*>(p->data);
                switch (ev->reply_userdata) {
                case kPropTimePos:
                    if (m_lastPosition < 0.0
                        || qAbs(v - m_lastPosition) >= kPositionEpsilonSec) {
                        m_lastPosition = v;
                        Q_EMIT positionChanged(v);
                    }
                    break;
                case kPropDuration:
                    if (v > 0.0 && qAbs(v - m_lastDuration) > 0.01) {
                        m_lastDuration = v;
                        Q_EMIT durationChanged(v);
                    }
                    break;
                case kPropVolume:
                    // Small dead-band to keep slider jitter out of the UI.
                    if (qAbs(v - m_lastVolume) > 0.05) {
                        m_lastVolume = v;
                        Q_EMIT volumeChanged(v);
                    }
                    break;
                case kPropSpeed:
                    if (qAbs(v - m_lastSpeed) > 0.005) {
                        m_lastSpeed = v;
                        Q_EMIT speedChanged(v);
                    }
                    break;
                case kPropDemuxerCacheTime:
                    if (m_lastCacheAhead < 0.0
                        || qAbs(v - m_lastCacheAhead) > 0.25) {
                        m_lastCacheAhead = v;
                        Q_EMIT cacheAheadChanged(v);
                    }
                    m_stats.cacheSeconds = v;
                    Q_EMIT videoStatsChanged(m_stats);
                    break;
                case kPropVfFps:
                    m_stats.fps = v;
                    Q_EMIT videoStatsChanged(m_stats);
                    break;
                case kPropVideoBitrate:
                    m_stats.videoBitrate = v;
                    Q_EMIT videoStatsChanged(m_stats);
                    break;
                case kPropAudioBitrate:
                    m_stats.audioBitrate = v;
                    Q_EMIT videoStatsChanged(m_stats);
                    break;
                default:
                    break;
                }
            } else if (p->format == MPV_FORMAT_INT64) {
                const auto v = *static_cast<int64_t*>(p->data);
                switch (ev->reply_userdata) {
                case kPropCacheBufferingState:
                    m_bufferingPct = static_cast<int>(v);
                    Q_EMIT bufferingChanged(
                        m_bufferingWaiting, m_bufferingPct);
                    break;
                case kPropWidth:
                    m_stats.width = static_cast<int>(v);
                    Q_EMIT videoStatsChanged(m_stats);
                    break;
                case kPropHeight:
                    m_stats.height = static_cast<int>(v);
                    Q_EMIT videoStatsChanged(m_stats);
                    break;
                default:
                    break;
                }
            } else if (p->format == MPV_FORMAT_STRING) {
                // `p->data` is a `char**` per mpv_event_property docs.
                const char* const* pp = static_cast<char**>(p->data);
                const char* s = pp ? *pp : nullptr;
                switch (ev->reply_userdata) {
                case kPropAid: {
                    const int id = parseTrackIdString(s);
                    if (id != m_lastAid) {
                        m_lastAid = id;
                        Q_EMIT audioTrackChanged(id);
                    }
                    break;
                }
                case kPropSid: {
                    const int id = parseTrackIdString(s);
                    if (id != m_lastSid) {
                        m_lastSid = id;
                        Q_EMIT subtitleTrackChanged(id);
                    }
                    break;
                }
                case kPropVideoCodec:
                    m_stats.videoCodec = s ? QString::fromUtf8(s) : QString();
                    Q_EMIT videoStatsChanged(m_stats);
                    break;
                case kPropAudioCodec:
                    m_stats.audioCodec = s ? QString::fromUtf8(s) : QString();
                    Q_EMIT videoStatsChanged(m_stats);
                    break;
                default:
                    break;
                }
            }
            break;
        }
        default:
            break;
        }
    }
}

void MpvWidget::loadFile(const QUrl& url,
    std::optional<double> startSeconds)
{
    if (!m_mpv) {
        return;
    }
    // Every loadfile starts a fresh timeline; invalidate the cached
    // position/duration so the first new sample is always forwarded.
    m_lastPosition = -1.0;
    m_lastDuration = -1.0;
    m_lastCacheAhead = -1.0;
    m_bufferingWaiting = false;
    m_bufferingPct = 0;
    m_tracks.clear();
    m_chapters.clear();
    m_lastAid = -1;
    m_lastSid = -1;
    m_stats = VideoStats {};
    // Notify UI so residual chrome from the previous session clears
    // before the new property-change storm arrives.
    Q_EMIT trackListChanged(m_tracks);
    Q_EMIT chaptersChanged(m_chapters);

    // Seed the persisted volume before loadfile so the first decoded
    // frames come out at the expected level. -1 means "never saved";
    // leave mpv's own default (100) alone in that case.
    const double savedVolume = m_settings.rememberedVolume();
    if (savedVolume >= 0.0) {
        double v = savedVolume;
        mpv_set_property_async(m_mpv, 0, "volume",
            MPV_FORMAT_DOUBLE, &v);
    }

    // Use the fully-encoded form when handing off to mpv/ffmpeg.
    // QUrl::toString()'s default (PrettyDecoded) leaves spaces and
    // other reserved characters literal; libmpv / ffmpeg's HTTP
    // stack expects a properly percent-encoded URL and some upstream
    // servers reject the decoded form outright.
    const auto urlStr = url.toEncoded();

    if (startSeconds && *startSeconds > 0.0) {
        // `loadfile <url> [<flags> [<index> [<options>]]]` — options
        // are the FOURTH positional arg, and the third must be an
        // integer playlist index. Pass "-1" (mpv's "end of playlist"
        // sentinel) since `replace` doesn't use it. options is a
        // comma-separated list of key=value; mpv clamps negative /
        // out-of-range `start` values on its own.
        QByteArray opts = QByteArrayLiteral("start=")
            + QByteArray::number(*startSeconds, 'f', 3);
        const char* cmd[] = {
            "loadfile", urlStr.constData(), "replace", "-1",
            opts.constData(), nullptr,
        };
        if (mpv_command_async(m_mpv, 0, cmd) < 0) {
            Q_EMIT mpvError(
                QStringLiteral("Could not load file into mpv"));
        }
        return;
    }

    const char* cmd[] = { "loadfile", urlStr.constData(), nullptr };
    if (mpv_command_async(m_mpv, 0, cmd) < 0) {
        Q_EMIT mpvError(QStringLiteral("Could not load file into mpv"));
    }
}

void MpvWidget::stop()
{
    if (!m_mpv) {
        return;
    }
    const char* cmd[] = { "stop", nullptr };
    mpv_command_async(m_mpv, 0, cmd);
}

void MpvWidget::setPaused(bool paused)
{
    if (!m_mpv) {
        return;
    }
    int flag = paused ? 1 : 0;
    mpv_set_property_async(m_mpv, 0, "pause", MPV_FORMAT_FLAG, &flag);
}

void MpvWidget::setMpvFullscreen(bool on)
{
    if (!m_mpv) {
        return;
    }
    if (m_fullscreen == on) {
        return;
    }
    m_suppressFullscreenEcho = true;
    int flag = on ? 1 : 0;
    mpv_set_property_async(m_mpv, 0, "fullscreen", MPV_FORMAT_FLAG, &flag);
}

// ---- Transport helpers ----------------------------------------------------

void MpvWidget::seekAbsolute(double seconds)
{
    if (!m_mpv || seconds < 0.0) {
        return;
    }
    const auto target = QByteArray::number(seconds, 'f', 3);
    const char* cmd[] = { "seek", target.constData(), "absolute", nullptr };
    mpv_command_async(m_mpv, 0, cmd);
}

void MpvWidget::setVolumePercent(double v)
{
    if (!m_mpv) {
        return;
    }
    if (v < 0.0) {
        v = 0.0;
    } else if (v > 100.0) {
        v = 100.0;
    }
    mpv_set_property_async(m_mpv, 0, "volume", MPV_FORMAT_DOUBLE, &v);
}

void MpvWidget::setMuted(bool m)
{
    if (!m_mpv) {
        return;
    }
    int flag = m ? 1 : 0;
    mpv_set_property_async(m_mpv, 0, "mute", MPV_FORMAT_FLAG, &flag);
}

void MpvWidget::setSpeed(double s)
{
    if (!m_mpv || s <= 0.0) {
        return;
    }
    mpv_set_property_async(m_mpv, 0, "speed", MPV_FORMAT_DOUBLE, &s);
}

void MpvWidget::cyclePause()
{
    if (!m_mpv) {
        return;
    }
    const char* cmd[] = { "cycle", "pause", nullptr };
    mpv_command_async(m_mpv, 0, cmd);
}

void MpvWidget::setAudioTrack(int id)
{
    if (!m_mpv) {
        return;
    }
    const QByteArray v = id < 0
        ? QByteArrayLiteral("no")
        : QByteArray::number(id);
    mpv_set_property_string(m_mpv, "aid", v.constData());
}

void MpvWidget::setSubtitleTrack(int id)
{
    if (!m_mpv) {
        return;
    }
    const QByteArray v = id < 0
        ? QByteArrayLiteral("no")
        : QByteArray::number(id);
    mpv_set_property_string(m_mpv, "sid", v.constData());
}

void MpvWidget::setMediaTitle(const QString& title)
{
    if (!m_mpv) {
        return;
    }
    const QByteArray utf8 = title.toUtf8();
    mpv_set_property_string(m_mpv, "force-media-title",
        utf8.constData());
}

QByteArray MpvWidget::fetchPropertyJson(const char* name)
{
    if (!m_mpv) {
        return {};
    }
    char* s = mpv_get_property_string(m_mpv, name);
    if (!s) {
        return {};
    }
    QByteArray out(s);
    mpv_free(s);
    return out;
}

void MpvWidget::refreshTrackList()
{
    const auto json = fetchPropertyJson("track-list");
    m_tracks = core::tracks::parseTrackList(json);
    Q_EMIT trackListChanged(m_tracks);
}

void MpvWidget::refreshChapterList()
{
    const auto json = fetchPropertyJson("chapter-list");
    m_chapters = core::chapters::parseChapterList(json);
    Q_EMIT chaptersChanged(m_chapters);
}

void MpvWidget::appendLogLine(const QString& line)
{
    m_logTail.push_back(line);
    while (m_logTail.size() > kLogTailMax) {
        m_logTail.pop_front();
    }
}

QStringList MpvWidget::recentLogLines() const
{
    QStringList out;
    out.reserve(static_cast<int>(m_logTail.size()));
    for (const auto& s : m_logTail) {
        out.push_back(s);
    }
    return out;
}

// ---- input forwarding -----------------------------------------------------

void MpvWidget::forwardKey(QKeyEvent* e, bool down)
{
    if (!m_mpv) {
        return;
    }
    const auto name = mpvKeyWithModifiers(e);
    if (name.isEmpty()) {
        return;
    }
    const char* cmd[] = {
        down ? "keydown" : "keyup",
        name.constData(),
        nullptr,
    };
    mpv_command_async(m_mpv, 0, cmd);
}

void MpvWidget::keyPressEvent(QKeyEvent* e)
{
    // Escape is owned by the outer window ("leave fullscreen"). We
    // don't forward it into mpv — mpv's default binding would quit.
    if (e->key() == Qt::Key_Escape) {
        Q_EMIT leaveFullscreenRequested();
        e->accept();
        return;
    }
    // `f` without modifiers is the canonical fullscreen toggle; we
    // let the outer window drive Qt's fullscreen state instead of
    // letting mpv handle it alone, so the window chrome follows.
    if (e->key() == Qt::Key_F && e->modifiers() == Qt::NoModifier) {
        Q_EMIT toggleFullscreenRequested();
        e->accept();
        return;
    }
    forwardKey(e, /*down=*/true);
    e->accept();
}

void MpvWidget::keyReleaseEvent(QKeyEvent* e)
{
    if (e->key() == Qt::Key_Escape || e->key() == Qt::Key_F) {
        e->accept();
        return;
    }
    forwardKey(e, /*down=*/false);
    e->accept();
}

void MpvWidget::updateMousePos(const QPoint& pos)
{
    if (!m_mpv) {
        return;
    }
    const auto x = QByteArray::number(
        static_cast<int>(pos.x() * devicePixelRatioF()));
    const auto y = QByteArray::number(
        static_cast<int>(pos.y() * devicePixelRatioF()));
    const char* cmd[] = { "mouse", x.constData(), y.constData(), nullptr };
    mpv_command_async(m_mpv, 0, cmd);
}

void MpvWidget::mouseMoveEvent(QMouseEvent* e)
{
    updateMousePos(e->pos());
    e->accept();
}

void MpvWidget::forwardMouseButton(QMouseEvent* e, bool down)
{
    if (!m_mpv) {
        return;
    }
    const auto name = mpvMouseButton(e->button());
    if (name.isEmpty()) {
        return;
    }
    // Keep the cursor position fresh so OSC hit-testing works.
    updateMousePos(e->pos());
    const char* cmd[] = {
        down ? "keydown" : "keyup",
        name.constData(),
        nullptr,
    };
    mpv_command_async(m_mpv, 0, cmd);
}

void MpvWidget::mousePressEvent(QMouseEvent* e)
{
    forwardMouseButton(e, /*down=*/true);
    e->accept();
}

void MpvWidget::mouseReleaseEvent(QMouseEvent* e)
{
    forwardMouseButton(e, /*down=*/false);
    e->accept();
}

void MpvWidget::mouseDoubleClickEvent(QMouseEvent* e)
{
    if (e->button() == Qt::LeftButton) {
        Q_EMIT toggleFullscreenRequested();
        e->accept();
        return;
    }
    QOpenGLWidget::mouseDoubleClickEvent(e);
}

void MpvWidget::wheelEvent(QWheelEvent* e)
{
    if (!m_mpv) {
        return;
    }
    const QPoint angle = e->angleDelta();
    QByteArray name;
    if (angle.y() > 0) {
        name = QByteArrayLiteral("WHEEL_UP");
    } else if (angle.y() < 0) {
        name = QByteArrayLiteral("WHEEL_DOWN");
    } else if (angle.x() > 0) {
        name = QByteArrayLiteral("WHEEL_RIGHT");
    } else if (angle.x() < 0) {
        name = QByteArrayLiteral("WHEEL_LEFT");
    }
    if (name.isEmpty()) {
        QOpenGLWidget::wheelEvent(e);
        return;
    }
    const char* cmd[] = { "keypress", name.constData(), nullptr };
    mpv_command_async(m_mpv, 0, cmd);
    e->accept();
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
