// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/MpvWidget.h"

#include "kinema_debug.h"

#include <mpv/client.h>
#include <mpv/render.h>
#include <mpv/render_gl.h>

#include <QByteArray>
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

} // namespace

MpvWidget::MpvWidget(QWidget* parent)
    : QOpenGLWidget(parent)
{
    setFocusPolicy(Qt::StrongFocus);
    setMouseTracking(true);
    setAttribute(Qt::WA_OpaquePaintEvent);

    m_mpv = mpv_create();
    if (!m_mpv) {
        qCWarning(KINEMA) << "mpv_create failed";
        return;
    }

    // Options must be set before mpv_initialize(). Keep this minimal —
    // the user's ~/.config/mpv/mpv.conf + input.conf are loaded by
    // default (config=yes) so keybindings and scripts behave as in
    // standalone mpv.
    mpv_set_option_string(m_mpv, "config", "yes");
    mpv_set_option_string(m_mpv, "terminal", "no");
    mpv_set_option_string(m_mpv, "msg-level", "all=no,cplayer=info");
    mpv_set_option_string(m_mpv, "input-default-bindings", "yes");
    mpv_set_option_string(m_mpv, "input-vo-keyboard", "yes");
    mpv_set_option_string(m_mpv, "osc", "yes");
    mpv_set_option_string(m_mpv, "osd-bar", "yes");
    mpv_set_option_string(m_mpv, "hwdec", "auto-safe");
    mpv_set_option_string(m_mpv, "vo", "libmpv");
    // We feed direct URLs only — never YouTube pages — so ytdl is
    // unnecessary overhead and an extra dependency.
    mpv_set_option_string(m_mpv, "ytdl", "no");
    mpv_set_option_string(m_mpv, "keep-open", "no");

    mpv_request_log_messages(m_mpv, "info");

    if (mpv_initialize(m_mpv) < 0) {
        qCWarning(KINEMA) << "mpv_initialize failed";
        mpv_destroy(m_mpv);
        m_mpv = nullptr;
        return;
    }

    mpv_observe_property(m_mpv, kPropPause, "pause", MPV_FORMAT_FLAG);
    mpv_observe_property(m_mpv, kPropFullscreen, "fullscreen", MPV_FORMAT_FLAG);

    mpv_set_wakeup_callback(m_mpv, &onMpvWakeup, this);
}

MpvWidget::~MpvWidget()
{
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
            if (isRenderModule && level >= MPV_LOG_LEVEL_INFO) {
                const auto text = QString::fromUtf8(msg->text);
                if (text.contains(QLatin1String("OpenGL error"))) {
                    break;
                }
            }
            qCInfo(KINEMA).nospace()
                << "[mpv/" << msg->prefix << "] "
                << QString::fromUtf8(msg->text).trimmed();
            break;
        }
        case MPV_EVENT_FILE_LOADED:
            Q_EMIT fileLoaded();
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
            if (!p || p->format != MPV_FORMAT_FLAG || !p->data) {
                break;
            }
            const bool on = *static_cast<int*>(p->data) != 0;
            if (ev->reply_userdata == kPropPause) {
                m_paused = on;
            } else if (ev->reply_userdata == kPropFullscreen) {
                m_fullscreen = on;
                if (m_suppressFullscreenEcho) {
                    m_suppressFullscreenEcho = false;
                } else {
                    Q_EMIT fullscreenChanged(on);
                }
            }
            break;
        }
        default:
            break;
        }
    }
}

void MpvWidget::loadFile(const QUrl& url)
{
    if (!m_mpv) {
        return;
    }
    const auto urlStr = url.toString().toUtf8();
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
