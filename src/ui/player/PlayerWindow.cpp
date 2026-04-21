// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/PlayerWindow.h"

#include "ui/player/MpvWidget.h"

#include <KLocalizedString>
#include <KSharedConfig>
#include <KConfigGroup>

#include <QByteArray>
#include <QCloseEvent>
#include <QGuiApplication>
#include <QHideEvent>
#include <QIcon>
#include <QKeyEvent>
#include <QScreen>
#include <QShowEvent>
#include <QString>
#include <QVBoxLayout>
#include <QWidget>

namespace kinema::ui::player {

namespace {

constexpr auto kConfigGroup = "PlayerWindow";
constexpr auto kConfigGeometryKey = "Geometry";

} // namespace

PlayerWindow::PlayerWindow(QWidget* parent)
    : QWidget(parent, Qt::Window)
{
    // App-consistent window chrome. The actual title is set per-play
    // by play(); the placeholder here is what shows if someone
    // toggles the window visible before loading a file.
    setWindowTitle(i18nc("@title:window", "Kinema Player"));
    setWindowIcon(QIcon::fromTheme(QStringLiteral("dev.tlmtech.kinema")));

    // Full-bleed video. The MpvWidget owns its own OSC; we don't
    // render any Qt-side controls.
    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_mpv = new MpvWidget(this);
    layout->addWidget(m_mpv);

    // Route mpv-side signals up to MainWindow (KNotification / status
    // bar) and into our own fullscreen/close handling.
    connect(m_mpv, &MpvWidget::fileLoaded,
        this, &PlayerWindow::fileLoaded);
    connect(m_mpv, &MpvWidget::mpvError,
        this, &PlayerWindow::mpvError);
    connect(m_mpv, &MpvWidget::endOfFile,
        this, [this](const QString& reason) {
            // Window has already served its purpose — stop + hide,
            // then let MainWindow react to the reason (e.g. show an
            // "ended with an error" status message).
            stopAndHide();
            Q_EMIT endOfFile(reason);
        });
    connect(m_mpv, &MpvWidget::toggleFullscreenRequested,
        this, &PlayerWindow::toggleFullscreen);
    connect(m_mpv, &MpvWidget::leaveFullscreenRequested,
        this, &PlayerWindow::leaveFullscreenOrClose);
    connect(m_mpv, &MpvWidget::fullscreenChanged,
        this, &PlayerWindow::mirrorMpvFullscreen);
}

PlayerWindow::~PlayerWindow() = default;

void PlayerWindow::play(const QUrl& url, const QString& title)
{
    m_hasEverLoaded = true;

    setWindowTitle(title.isEmpty()
        ? QStringLiteral("Kinema")
        : i18nc("@title:window window title with media title",
              "%1 — Kinema", title));

    // On first show, apply persisted or default geometry before the
    // window actually becomes visible (showEvent does this via
    // m_geometryApplied, but show() also triggers showEvent, so
    // setting geometry here ahead of show() is belt-and-braces for
    // the case where the window is already visible and play() is
    // just swapping files).
    if (!m_geometryApplied) {
        loadGeometry();
    }

    m_mpv->loadFile(url);

    show();
    raise();
    activateWindow();
    m_mpv->setFocus();
}

void PlayerWindow::stopAndHide()
{
    if (isFullScreen()) {
        // Leave fullscreen first so the next show() uses the
        // remembered windowed geometry, not the bare screen.
        m_applyingFullscreen = true;
        showNormal();
        if (m_mpv) {
            m_mpv->setMpvFullscreen(false);
        }
        m_applyingFullscreen = false;
    }
    if (m_mpv) {
        m_mpv->stop();
    }
    hide();
}

void PlayerWindow::closeEvent(QCloseEvent* e)
{
    // X button / window manager close. Persist geometry, stop
    // playback, hide. Never destroy — we want to reuse the libmpv
    // context for the next play.
    saveGeometryToConfig();
    stopAndHide();
    e->accept();
}

void PlayerWindow::keyPressEvent(QKeyEvent* e)
{
    if (e->key() == Qt::Key_Escape) {
        leaveFullscreenOrClose();
        e->accept();
        return;
    }
    QWidget::keyPressEvent(e);
}

void PlayerWindow::showEvent(QShowEvent* e)
{
    if (!m_geometryApplied) {
        loadGeometry();
    }
    QWidget::showEvent(e);
    Q_EMIT visibilityChanged(true);
}

void PlayerWindow::hideEvent(QHideEvent* e)
{
    // Persist on every hide (covers both the close-button path and
    // explicit hide() calls from stopAndHide()), so crashes between
    // open and quit don't lose the user's sizing.
    saveGeometryToConfig();
    QWidget::hideEvent(e);
    Q_EMIT visibilityChanged(false);
}

void PlayerWindow::toggleFullscreen()
{
    if (m_applyingFullscreen) {
        return;
    }
    m_applyingFullscreen = true;
    if (isFullScreen()) {
        showNormal();
        if (m_mpv) {
            m_mpv->setMpvFullscreen(false);
        }
    } else {
        showFullScreen();
        if (m_mpv) {
            m_mpv->setMpvFullscreen(true);
        }
    }
    if (m_mpv) {
        m_mpv->setFocus();
    }
    m_applyingFullscreen = false;
}

void PlayerWindow::leaveFullscreenOrClose()
{
    // Esc / mpv "leave fullscreen" request. Windowed → close the
    // window (which in turn stops playback and hides). Fullscreen
    // → just leave fullscreen, keep playing.
    if (isFullScreen()) {
        m_applyingFullscreen = true;
        showNormal();
        if (m_mpv) {
            m_mpv->setMpvFullscreen(false);
            m_mpv->setFocus();
        }
        m_applyingFullscreen = false;
        return;
    }
    close();
}

void PlayerWindow::mirrorMpvFullscreen(bool on)
{
    // mpv's `fullscreen` property flipped (e.g. user's own script or
    // OSC toggle). Mirror into Qt. The re-entrancy guard suppresses
    // the feedback loop when *we* were the one who set it.
    if (m_applyingFullscreen) {
        return;
    }
    m_applyingFullscreen = true;
    if (on && !isFullScreen()) {
        showFullScreen();
    } else if (!on && isFullScreen()) {
        showNormal();
    }
    m_applyingFullscreen = false;
}

void PlayerWindow::loadGeometry()
{
    m_geometryApplied = true;

    const auto group = KSharedConfig::openConfig()->group(
        QString::fromLatin1(kConfigGroup));
    const QByteArray saved = group.readEntry(
        QString::fromLatin1(kConfigGeometryKey), QByteArray {});
    if (!saved.isEmpty() && restoreGeometry(saved)) {
        return;
    }

    // First-run default: 1280x720 centred on the parent's screen
    // (falls back to the primary screen if we have no parent).
    resize(1280, 720);
    QScreen* screen = nullptr;
    if (auto* p = parentWidget()) {
        screen = p->screen();
    }
    if (!screen) {
        screen = QGuiApplication::primaryScreen();
    }
    if (screen) {
        const QRect avail = screen->availableGeometry();
        move(avail.center() - rect().center());
    }
}

void PlayerWindow::saveGeometryToConfig()
{
    auto group = KSharedConfig::openConfig()->group(
        QString::fromLatin1(kConfigGroup));
    group.writeEntry(
        QString::fromLatin1(kConfigGeometryKey), saveGeometry());
    group.sync();
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
